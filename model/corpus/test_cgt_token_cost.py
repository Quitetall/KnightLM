"""CGT token-cost helpers; the renderer itself is tested in Rust."""

import unittest

from model.corpus import cgt_token_cost as cost

START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
PACKET = {"schema_version": 1, "fen": START, "facts": {"repetition_count": 1}}


class Stats(unittest.TestCase):
    def test_percentile_is_nearest_rank(self):
        self.assertEqual(cost.percentile(list(range(1, 101)), 95), 95)
        self.assertEqual(cost.percentile([7], 95), 7)

    def test_summary(self):
        s = cost.summarize([1, 2, 3, 4])
        self.assertEqual((s["mean"], s["max"], s["n"]), (2.5, 4, 4))


@unittest.skipIf(cost.BINARY is None, "knightlm binary not built")
class Rendering(unittest.TestCase):
    def test_forms_from_the_compiler(self):
        forms = cost.render_forms(cost.BINARY, PACKET)
        self.assertTrue(forms["cgt_lite"].startswith("cgt/1 lite\n"))
        self.assertTrue(forms["cgt_full"].startswith("cgt/1 full\n"))
        self.assertIn("rep 1", forms["cgt_lite"])
        self.assertIn("Nb1: a3 c3", forms["cgt_full"])
        self.assertLess(len(forms["cgt_lite"]), len(forms["cgt_full"]))


if __name__ == "__main__":
    unittest.main()
