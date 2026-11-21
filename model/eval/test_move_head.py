import unittest

from model.eval import move_head
from model.eval.fakes import fixture_rows
from model.policy_head import space


class Logits:
    """Fake predictor: build raw logits per row from a rule. Indexed by FEN."""

    def __init__(self, rule):
        self.rule, self.rows = rule, {r["packet"]["fen"]: r for r in fixture_rows()}

    def logits(self, packet):
        row = self.rows[packet["fen"]]
        out = [0.0] * space.SIZE
        self.rule(out, row)
        return out


def perfect(out, row):
    out[space.INDEX[row["target"]]] = 9.0


def illegal_favoured(out, row):  # huge raw logit on an illegal move, human move second among legal
    legal = set(row["packet"]["legal_moves"])
    out[next(i for i, m in enumerate(space.UCI) if m not in legal)] = 50.0
    out[space.INDEX[row["target"]]] = 3.0


def broken(out, row):
    raise RuntimeError("boom")


class FakeScorer:
    def score_moves(self, pairs):
        return {p: {"cpl": 0 if p[1] == "g4h5" else 40, "best_move": "g4h5", "status": "scored"} for p in pairs}

    def info(self):
        return {"engine": "fake"}


class MoveHeadAxis(unittest.TestCase):
    rows = fixture_rows()

    def test_perfect_predictor(self):
        _, card = move_head.evaluate(self.rows, Logits(perfect))
        axis = card["axes"]["move_head"]
        self.assertIsNone(card["composite_score"])
        self.assertEqual(axis["masked"]["top1_match"]["rate"], 1.0)
        self.assertEqual(axis["masked"]["top3_match"]["rate"], 1.0)
        self.assertEqual(axis["raw"]["argmax_legal"]["rate"], 1.0)
        self.assertEqual(axis["label"], "human-only move head")

    def test_raw_illegal_but_masked_product_is_legal(self):
        records, card = move_head.evaluate(self.rows, Logits(illegal_favoured))
        axis = card["axes"]["move_head"]
        self.assertEqual(axis["raw"]["argmax_legal"]["count"], 0)
        self.assertEqual(axis["raw"]["top1_match"]["count"], 0)
        self.assertGreater(axis["raw"]["illegal_mass_mean"], 0.99)
        self.assertEqual(axis["masked"]["top1_match"]["rate"], 1.0)  # masking hides it from the product only
        self.assertTrue(all(r["raw"]["status"] == "illegal" and r["masked"]["status"] == "ok" for r in records))

    def test_failures_stay_in_denominator(self):
        _, card = move_head.evaluate(self.rows, Logits(broken))
        axis = card["axes"]["move_head"]
        self.assertEqual(axis["rows_with_logits"], 0)
        self.assertEqual(axis["masked"]["top1_match"], {"count": 0, "n": len(self.rows), "rate": 0.0})

    def test_wrong_length_is_a_failure(self):
        class Short:
            def logits(self, packet):
                return [0.0] * 10
        records, _ = move_head.evaluate(self.rows, Short())
        self.assertTrue(all("1968" in r["error"] for r in records))

    def test_cpl_through_scoring_path(self):
        _, card = move_head.evaluate(self.rows, Logits(illegal_favoured), FakeScorer())
        cpl = card["axes"]["move_head"]["centipawn_loss"]
        self.assertEqual(cpl["masked"]["rows_scored"], len(self.rows))
        self.assertEqual(cpl["raw"]["rows_scored"], 0)
        self.assertEqual(cpl["raw"]["rows_unscored"], len(self.rows))
        self.assertEqual(cpl["engine"], {"engine": "fake"})

    def test_no_scorer_means_no_cpl(self):
        _, card = move_head.evaluate(self.rows, Logits(perfect))
        self.assertIsNone(card["axes"]["move_head"]["centipawn_loss"])


if __name__ == "__main__":
    unittest.main()
