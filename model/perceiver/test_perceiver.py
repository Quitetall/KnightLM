"""CPU tests. Skip when numpy/Pillow/torch (or the Rust CLI) are missing, as in the fast CI tier."""
import re
import unittest
from pathlib import Path

try:
    import numpy as np
    import torch
    from PIL import Image  # noqa: F401
    HAVE = True
except ImportError:  # pragma: no cover
    HAVE = False

START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR"
MID = "r1bq1rk1/1pp2pp1/p1n1pb1p/2Pp4/1P1P4/P1Q1PNP1/1B3P1P/R3KB1R"
FULL = MID + " w - - 0 13"


class FenTests(unittest.TestCase):
    def test_roundtrip_and_refusals(self):
        from model.perceiver.fen import placement_to_labels, labels_to_placement, flip_labels

        for p in (START, MID, "8/8/8/8/8/8/8/8"):
            self.assertEqual(labels_to_placement(placement_to_labels(p)), p)
        self.assertEqual(placement_to_labels(START)[0], 10)  # black rook on a8
        self.assertEqual(flip_labels(flip_labels(placement_to_labels(MID))), placement_to_labels(MID))
        for bad in ("8/8/8", "9/8/8/8/8/8/8/8", "x7/8/8/8/8/8/8/8", "ppppppppp/8/8/8/8/8/8/8"):
            with self.assertRaises(ValueError):
                placement_to_labels(bad)
        with self.assertRaises(ValueError):
            labels_to_placement([0] * 63)


@unittest.skipUnless(HAVE, "numpy/Pillow/torch not installed")
class DataTests(unittest.TestCase):
    def test_render_is_deterministic_and_styles_differ(self):
        from model.perceiver.render import render_board
        from model.perceiver.styles import VECTOR_ONLY

        a, b = VECTOR_ONLY[0], VECTOR_ONLY[1]
        x1 = np.asarray(render_board(MID, b, 7))
        x2 = np.asarray(render_board(MID, b, 7))
        self.assertTrue((x1 == x2).all())
        self.assertFalse((x1 == np.asarray(render_board(MID, b, 8))).all())  # noise seed matters
        self.assertFalse((np.asarray(render_board(MID, a, 7)) == x1).all())  # styles differ

    def test_flip_gives_same_labels(self):
        from model.perceiver.render import render_board, board_to_cells
        from model.perceiver.styles import Style, VECTOR_ONLY
        from dataclasses import replace

        base = replace(VECTOR_ONLY[0], noise=0.0, jpeg=0, blur=0.0)
        flipped = replace(base, flip=True)
        c0 = board_to_cells(render_board(MID, base), False)
        c1 = board_to_cells(render_board(MID, flipped), True)
        self.assertTrue((c0 == c1).all())

    def test_manifest_licences(self):
        from model.perceiver import dataset, pieces
        from model.perceiver.styles import ALL_STYLES

        m = dataset.manifest([s for s in ALL_STYLES if dataset.available(s)], 0)
        self.assertEqual(m["piece_sets"][0]["third_party_assets"], [])
        permissive = {"CC0-1.0", "Apache-2.0", "MIT"}
        for entry in m["piece_sets"][1:]:
            self.assertIn(entry["licence"], permissive)
        with self.assertRaises(ValueError):
            pieces.svg_sprites("cburnett")

    def test_no_train_style_in_heldout(self):
        from model.perceiver.styles import TRAIN_STYLES, HELDOUT_STYLES

        self.assertFalse({s.name for s in TRAIN_STYLES} & {s.name for s in HELDOUT_STYLES})
        train_art = {(s.pieces, s.light, s.dark) for s in TRAIN_STYLES}
        self.assertFalse(train_art & {(s.pieces, s.light, s.dark) for s in HELDOUT_STYLES})


@unittest.skipUnless(HAVE, "numpy/Pillow/torch not installed")
class SmokeTrainTests(unittest.TestCase):
    def test_overfits_and_scorecard(self):
        from model.perceiver import dataset
        from model.perceiver.train import fit
        from model.perceiver.evaluate import scorecard
        from model.perceiver.model import SquareCNN, predict
        from model.perceiver.styles import VECTOR_ONLY, TRAIN_STYLES, HELDOUT_STYLES

        torch.set_num_threads(2)
        positions = [START, MID, "4k3/8/8/3Q4/8/8/PPP5/4K2R", "r3k2r/8/8/8/8/8/8/R3K2R",
                     "8/5k2/3p4/1p1Pp2p/pP2Pp1P/P4P1K/8/8", "6k1/5ppp/8/8/8/8/5PPP/3R2K1"]
        styles = [s for s in TRAIN_STYLES if s.pieces.startswith("vector")][:2]
        cells, labels, _, _ = dataset.build(positions, styles, 0, 6)
        self.assertEqual(len(cells), 6 * 2 * 64)
        model = fit(cells, labels, epochs=12, lr=5e-3, batch=128)
        acc = float((predict(model, cells) == labels).mean())
        self.assertGreaterEqual(acc, 0.98, f"smoke fit did not overfit: {acc}")
        # untrained net is far from that, so the check above is not vacuous
        torch.manual_seed(1)
        self.assertLess(float((predict(SquareCNN(), cells) == labels).mean()), 0.8)
        card = scorecard(model, positions, VECTOR_ONLY, 1, 2)
        self.assertEqual(set(card["styles"]), {s.name for s in VECTOR_ONLY})
        self.assertIn("heldout", card)
        for row in card["styles"].values():
            self.assertTrue(0.0 <= row["square_accuracy"] <= 1.0)
            self.assertLessEqual(row["board_exact_match"], row["square_accuracy"] + 1e-9)
        self.assertGreater(sum(map(sum, card["confusion"])), 0)


@unittest.skipUnless(HAVE, "numpy/Pillow/torch not installed")
class LedgerTests(unittest.TestCase):
    def setUp(self):
        from model.perceiver import observation

        self.obs = observation
        self.binary = observation.cli_binary()
        if not self.binary:
            self.skipTest("knightlm CLI not built")

    def test_schema_version_matches_rust(self):
        src = (Path(__file__).resolve().parents[2] / "crates/core/src/lib.rs").read_text()
        m = re.search(r"pub const SCHEMA_VERSION:\s*u32\s*=\s*(\d+)", src)
        self.assertTrue(m)
        self.assertEqual(int(m.group(1)), self.obs.SCHEMA_VERSION)

    def test_match_accepted_and_mismatch_is_perception_failure(self):
        from model.perceiver.fen import placement_to_labels

        labels = placement_to_labels(MID)
        good = self.obs.to_observation(labels, "img-1")
        self.assertEqual(set(good), {"schema_version", "image_id", "placement"})
        self.assertTrue(self.obs.verify(FULL, good)["ok"])
        bad = list(labels)
        bad[0] = 0  # drop the a8 rook
        r = self.obs.verify(FULL, self.obs.to_observation(bad, "img-2"))
        self.assertFalse(r["ok"])
        self.assertIn("perception failure", r["detail"])
        swapped = list(labels)
        swapped[0], swapped[1] = swapped[1], swapped[0]
        self.assertFalse(self.obs.verify(FULL, self.obs.to_observation(swapped, "img-3"))["ok"])

    def test_observation_has_no_state_fields(self):
        with self.assertRaises(ValueError):
            self.obs.to_observation([0] * 64, "")


if __name__ == "__main__":
    unittest.main()
