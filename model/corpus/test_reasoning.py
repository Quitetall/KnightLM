"""Reasoning corpus: generator vs the explanation scorecard, render format, and audit."""

import json
from pathlib import Path
import random
import re
import shutil
import tempfile
import unittest

from model import prompt
from model.corpus import pipeline, reasoning
from model.corpus.render import render_completion, render_play
from model.eval import explain
from model.corpus.pipeline import MANIFEST, REASONING_FORMAT, read_corpus, sha256_file, write_corpus
from model.corpus.test_pipeline import fixture_splits, plant_leak, provenance
from model.eval.explain import Board
from model.tests import _payload  # noqa: F401  (puts the BLUT payload on sys.path)
from knightlm_blut import materialize as mat

ROOT = Path(__file__).resolve().parents[2]
BINARY = next((p for p in (ROOT / "target/debug/knightlm", ROOT / "target/release/knightlm") if p.exists()), None)
START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
BANNED = re.compile(r"\b(best|strong|strongest|good|great|winning|wins|brilliant|blunder|mistake|should|"
                    r"must|threat\w*|forced|excellent|better|weak)\b", re.I)


FIXTURES = [  # (fen, move): castling, en passant, promotion, check, a hanging piece, a pinned defender
    ("r3k2r/pppq1ppp/2n2n2/3pp3/3PP3/2N2N2/PPPQ1PPP/R3K2R w KQkq - 0 1", "e1g1"),
    ("r3k2r/pppq1ppp/2n2n2/3pp3/3PP3/2N2N2/PPPQ1PPP/R3K2R w KQkq - 0 1", "e1c1"),
    ("r3k2r/pppq1ppp/2n2n2/3pp3/3PP3/2N2N2/PPPQ1PPP/R3K2R b KQkq - 0 1", "e8g8"),
    ("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1", "e5d6"),
    ("8/P6k/8/8/8/8/6K1/8 w - - 0 1", "a7a8q"),
    ("8/P6k/8/8/8/8/6K1/8 w - - 0 1", "a7a8n"),
    ("3r2k1/8/8/8/8/8/3Q4/3K4 w - - 0 1", "d2d8"),
    ("4k3/8/8/4n3/8/8/4Q3/4K3 w - - 0 1", "e2e5"),
    ("4k3/4r3/8/8/8/8/4B3/4K3 w - - 0 1", "e1d1"),
    ("rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2", "e4d5"),
    ("4k3/8/8/8/8/8/4r3/4K3 w - - 0 1", "e1e2"),
]


def walk(games=120, plies=70, seed=20261120):
    """Positions from seeded random legal playouts plus FIXTURES: (fen, facts) via the notation binary."""
    rng = random.Random(seed)
    fens = [START] * games
    seen = [(f, x, m) for (f, m), x in zip(
        FIXTURES, reasoning.compute_facts(BINARY, [{"fen": f, "move": m} for f, m in FIXTURES]), strict=True)]
    for _ in range(plies):
        live = [(i, f) for i, f in enumerate(fens) if f]
        if not live:
            break
        info = reasoning.compute_facts(BINARY, [{"fen": f, "move": None} for _, f in live])
        picks = [(i, {"fen": f, "move": rng.choice(x["legal_moves"]) if x["legal_moves"] else None})
                 for (i, f), x in zip(live, info, strict=True)]
        for i, p in picks:
            if p["move"] is None:
                fens[i] = None
        picks = [(i, p) for i, p in picks if p["move"]]
        full = reasoning.compute_facts(BINARY, [p for _, p in picks])
        for (i, p), f in zip(picks, full, strict=True):
            seen.append((f["fen"], f, p["move"]))
            fens[i] = f["move"]["fen_after"]
    return seen


@unittest.skipUnless(BINARY, "knightlm binary not built (cargo build -p knightlm-cli)")
class ScorecardOverGeneratedReasoning(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.positions = walk()
        cls.texts = []
        for fen, facts, uci in cls.positions:
            cls.texts.append((fen, facts, uci, reasoning.reasoning_text(facts, uci)))

    def test_many_positions_and_features(self):
        self.assertGreaterEqual(len(self.texts), 3000)
        text = " ".join(t[3] for t in self.texts)
        for needle in ("in check", "capturing the", "giving check", "castling", "en passant", "promoting", "developing", "pushing",
                       "undefended", "attacked and", "is up", "Material"):
            self.assertIn(needle, text, needle)

    def test_scorecard_finds_zero_failed_claims(self):
        results = []
        for fen, facts, uci, text in self.texts:
            result = explain.check_explanation(fen, facts["legal_moves"], text, uci)
            self.assertEqual(result["failures"], [], (fen, uci, text))
            self.assertEqual(result["failed_total"], 0)
            results.append(result)
        card = explain.aggregate(results)
        self.assertEqual(card["failed_total"], 0)
        self.assertEqual(card["explanations_without_checkable_claims"], 0)
        self.assertGreater(card["checkable_total"], 2 * len(results))
        for kind in ("piece_on_square", "move_legal", "material", "capture_target", "move_check"):
            self.assertGreater(card["claim_types"][kind]["checkable"], 0, kind)
        # nothing the scorecard saw but could not check: no hypothetical or illegal-move wording
        self.assertEqual(sum(card["unchecked_claims"].values()), 0, card["unchecked_claims"])

    def test_length_wording_and_determinism(self):
        for fen, facts, uci, text in self.texts:
            words = len(text.split())
            self.assertTrue(20 <= words <= 60, (words, text))
            self.assertLessEqual(len(text), reasoning.MAX_CHARS)
            self.assertIsNone(BANNED.search(text), text)
            self.assertIn(uci, text)
            self.assertEqual(reasoning.reasoning_text(facts, uci), text)
            parsed = prompt.parse_play_completion(render_completion(text, uci))
            self.assertEqual(parsed, {"reasoning": text, "move": uci})
            self.assertIn(uci, facts["legal_moves"])

    def test_counts_match_an_independent_board(self):
        """Count facts the scorecard does not check, verified on explain.Board (no shakmaty)."""
        for fen, facts, uci, text in self.texts[::7]:
            board, legal = Board(fen), facts["legal_moves"]
            self.assertEqual(facts["legal_count"], len(legal))
            self.assertEqual(facts["legal_capture_count"], sum(board.is_capture(m) for m in legal))
            self.assertEqual(facts["legal_check_count"], sum(board.gives_check(m) for m in legal))
            totals = board.material()
            self.assertEqual((facts["points"]["white"], facts["points"]["black"]), (totals["w"], totals["b"]))
            self.assertEqual(facts["in_check"], board.attacked(
                next(s for s, p in board.pieces.items() if p == (board.turn, "K")), "b" if board.turn == "w" else "w"))
            us, them = board.turn, "b" if board.turn == "w" else "w"
            for c in facts["captures"]:
                self.assertEqual(c["defended"], board.attacked(c["square"], them))
            for c in facts["own_attacked"]:
                self.assertTrue(board.attacked(c["square"], them))
                self.assertEqual(c["defended"], board.attacked(c["square"], us))
            if facts["legal_count"]:
                self.assertTrue(re.search(rf"\b{facts['legal_count']} legal move", text), text)

    def test_planted_wrong_facts_are_caught(self):
        planted = 0
        for fen, facts, uci, text in self.texts:
            lie = None
            m = re.search(r"capture the (\w+) on (\w\d)", text)
            if m:
                wrong = "queen" if m.group(1) != "queen" else "rook"
                lie = text.replace(m.group(0), f"capture the {wrong} on {m.group(2)}")
            elif re.search(r"is (up|ahead by) (\d+) point", text):
                lie = re.sub(r"(is (?:up|ahead by)) (\d+)", lambda g: f"{g.group(1)} {int(g.group(2)) + 5}", text, count=1)
            if lie is None:
                continue
            planted += 1
            result = explain.check_explanation(fen, facts["legal_moves"], lie, uci)
            self.assertGreater(result["failed_total"], 0, lie)
            if planted >= 200:
                break
        self.assertGreaterEqual(planted, 100)
        fen, facts, uci, text = self.texts[0]
        for lie in (text.replace(uci, "a1a8"), text + " Black has nine queens.", text + " e4 is empty and the king on a1."):
            self.assertGreater(explain.check_explanation(fen, facts["legal_moves"], lie, uci)["failed_total"], 0, lie)


class RenderFormat(unittest.TestCase):
    PACKET = {"schema_version": 1, "fen": START, "legal_moves": ["g1f3"], "facts": {}}

    def test_completion_round_trips_through_the_play_parser(self):
        text = render_completion('Say "hi" \u2713', "g1f3")
        self.assertEqual(prompt.parse_play_completion(text), {"reasoning": 'Say "hi" \u2713', "move": "g1f3"})
        self.assertEqual(text, prompt.format_play_completion('Say "hi" \u2713', "g1f3"))

    def test_prompt_is_the_play_prompt_and_in_schema(self):
        row = render_play(self.PACKET, "x", "g1f3")
        self.assertEqual(row["prompt"], prompt.play_text(self.PACKET))
        self.assertTrue(row["prompt"].endswith(prompt.packet_json(self.PACKET)))
        self.assertIn("submit_move", row["prompt"])
        schema = next(t for t in prompt.load_tool_schemas()["modes"]["play"] if t["name"] == "submit_move")
        self.assertEqual(schema["parameters"]["properties"]["reasoning"]["maxLength"], reasoning.MAX_CHARS)


def with_reasoning(rows):
    return [dict(row, reasoning=f"Fixture position. My move is {row['target']}, moving a piece.") for row in rows]


class ReasoningCorpusFormat(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, True)
        train, holdout = fixture_splits()
        self.train, self.holdout = with_reasoning(train), with_reasoning(holdout)

    def write(self, name="corpus", **changes):
        root = self.tmp / name
        extra = {"reasoning": {"generator": reasoning.GENERATOR_VERSION}}
        write_corpus(root, self.train, self.holdout, 10, {**provenance(), **extra}, REASONING_FORMAT)
        return root

    def test_round_trip_and_materialized_rows_use_the_play_prompt(self):
        root = self.write()
        train, holdout = read_corpus(root)
        self.assertEqual((len(train), len(holdout)), (len(self.train), len(self.holdout)))
        out = self.tmp / "mat"
        receipt = mat.materialize(root, out, _payload.ROOT)
        records = [json.loads(line) for line in (out / "dataset.jsonl").read_bytes().splitlines()]
        self.assertEqual(receipt["rows"], len(records))
        self.assertIn("render_py_sha256", receipt)
        for record, row in zip(records, train, strict=True):
            self.assertEqual(sorted(record), ["completion", "prompt"])
            self.assertEqual(record, render_play(row["packet"], row["reasoning"], row["target"]))
            self.assertEqual(prompt.parse_play_completion(record["completion"]),
                             {"reasoning": row["reasoning"], "move": row["target"]})
            self.assertTrue(record["prompt"].endswith(prompt.packet_json(row["packet"])))
            self.assertNotIn(row["reasoning"], record["prompt"])  # loss-masked side never carries the answer
        self.assertTrue(mat.materialize(root, out, _payload.ROOT)["reused"])

    def test_training_preflight_checks_the_play_prompt_parity(self):
        import contextlib
        import io
        from model.train import run

        root = self.write()
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = run.main(["--corpus", str(root), "--run-root", str(self.tmp / "runs"), "--run", "r", "--skip-model"])
        self.assertEqual(code, 0, out.getvalue())
        self.assertIn("[ok     ] prompt_parity", out.getvalue())
        self.assertEqual(run.check_prompt_parity(self.train[:3])["rows"], 3)

    def test_every_planted_leak_is_still_refused(self):
        for kind in ("dup_game_id", "game_id", "family", "position_key", "time"):
            with self.subTest(kind=kind):
                root = self.write(f"leak-{kind}")
                plant_leak(root, kind)
                with self.assertRaises(ValueError):
                    read_corpus(root)
                with self.assertRaises(mat.MaterializeError):
                    mat.materialize(root, self.tmp / f"out-{kind}", _payload.ROOT)

    def test_bad_reasoning_is_refused_at_write_and_read(self):
        for name, bad in (("missing", None), ("empty", " "), ("long", "x " * 300),
                          ("wrong move", "My move is a1a2."), ("newline", "a\nMy move is e2e4")):
            with self.subTest(name):
                rows = [dict(r) for r in self.train]
                if bad is None:
                    del rows[0]["reasoning"]
                else:
                    rows[0]["reasoning"] = bad
                with self.assertRaisesRegex(ValueError, "invalid reasoning"):
                    write_corpus(self.tmp / f"w-{name}", rows, self.holdout, 10, provenance(), REASONING_FORMAT)
        root = self.write("tamper")  # an edit after the build, with checksums fixed up, is still caught by shape
        manifest = json.loads((root / MANIFEST).read_text())
        shard = manifest["shards"]["train"]["files"][0]
        path = root / shard["file"]
        path.write_text(path.read_text().replace(" moving a piece.", "x" * 500, 1))
        shard["sha256"] = sha256_file(path)
        (root / MANIFEST).write_text(json.dumps(manifest))
        with self.assertRaisesRegex(ValueError, "invalid reasoning"):
            read_corpus(root)

    def test_formats_cannot_be_confused(self):
        with self.assertRaisesRegex(ValueError, "not the reasoning format"):
            write_corpus(self.tmp / "v2", self.train, self.holdout, 10, provenance())  # v2 format, reasoning rows
        root = self.write("named")
        manifest = json.loads((root / MANIFEST).read_text())
        del manifest["reasoning"]
        (root / MANIFEST).write_text(json.dumps(manifest))
        with self.assertRaisesRegex(ValueError, "name its generator"):
            read_corpus(root)


if __name__ == "__main__":
    unittest.main()
