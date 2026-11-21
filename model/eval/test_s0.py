"""S0 rules-QA axis: per-type exact match, failures stay in the denominator, matched arms compare."""

import contextlib
import io
import json
import tempfile
import unittest
from pathlib import Path

from model import prompt
from model.corpus import s0
from model.corpus.test_s0 import BINARY, CURRENT, SOURCE, split_rows
from model.eval import compare, run

STATE = {"replies": {}, "wrong_types": set(), "garble_types": set()}


class Oracle:
    """complete(prompt) answers from a table; chosen types answer wrongly or not at all."""

    prompt_kind = "play"

    def complete(self, text):
        row = STATE["replies"][text]
        kind = row["qa"]["type"]
        if kind in STATE["garble_types"]:
            return "I do not know", 10, 4, 1.0
        answer = row["qa"]["answer"] + (" (wrong)" if kind in STATE["wrong_types"] else "")
        return prompt.format_play_completion(answer, row["target"]), 10, 20, 1.0

    def manifest(self):
        return {"fake": "oracle"}


def oracle():
    return Oracle()


def build(root, arm):
    train_src, holdout_src = split_rows()
    train = s0.build_rows(BINARY, train_src, "train", arm)
    holdout = s0.build_rows(BINARY, holdout_src, "holdout", arm)
    out = Path(root) / arm
    s0.write_corpus(out, train, holdout, arm, BINARY, CURRENT, SOURCE, {"corpus": "fixture"}, "fixture", 10)
    return out, holdout


def evaluate(root, arm, label):
    corpus, holdout = build(root, arm)
    STATE["replies"] = {row["prompt"]: row for row in holdout}
    out = run.main(["--predictor", "model.eval.test_s0:oracle", "--label", label, "--rows", str(len(holdout)),
                    "--corpus", str(corpus), "--out-root", str(Path(root) / "out"), "--no-stockfish", "--no-tactics"])
    return out, holdout


class S0Axis(unittest.TestCase):
    def setUp(self):
        STATE.update(wrong_types=set(), garble_types=set())

    def test_perfect_predictor_scores_every_type(self):
        with tempfile.TemporaryDirectory() as tmp:
            out, holdout = evaluate(tmp, "t0", "perfect")
            card = json.loads((out / "scorecard.json").read_text())
        axis = card["axes"]["s0_rules_qa"]
        self.assertEqual(card["prompt_kind"], "play")
        self.assertIsNone(card["composite_score"])
        self.assertEqual(axis["overall"], {"n": 3, "exact": 3, "rate": 1.0, "claim_failure_rate": 0.0, "parse_failures": 0})
        for name, block in axis["by_type"].items():
            self.assertEqual(block["claim_failure_rate"], 0.0, name)

    def test_wrong_and_garbled_answers_are_failures_in_the_denominator(self):
        with tempfile.TemporaryDirectory() as tmp:
            corpus, holdout = build(tmp, "t0")
            types = [r["qa"]["type"] for r in holdout]
        STATE["wrong_types"], STATE["garble_types"] = {types[0]}, {types[1]}
        with tempfile.TemporaryDirectory() as tmp:
            out, _ = evaluate(tmp, "t0", "broken")
            axis = json.loads((out / "scorecard.json").read_text())["axes"]["s0_rules_qa"]
            self.assertTrue((out / "scorecard.md").read_text().startswith("# S0 rules-QA scorecard"))
        self.assertEqual(axis["overall"]["exact"], 1)
        self.assertEqual(axis["overall"]["parse_failures"], 1)
        self.assertEqual(axis["by_type"][types[0]]["claim_failure_rate"], 1.0)
        self.assertEqual(axis["by_type"][types[1]]["parse_failures"], 1)

    def test_stored_prompt_that_drifts_from_model_prompt_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            corpus, holdout = build(tmp, "t2")
            from model.eval import s0 as eval_s0
            drifted = [dict(holdout[0], prompt=holdout[0]["prompt"] + " ")]
            with self.assertRaisesRegex(ValueError, "stored prompt differs"):
                eval_s0.predict_rows(drifted, Oracle(), "play-cgt")

    def test_matched_arms_compare_without_the_flag_and_report_failure_rate_down(self):
        with tempfile.TemporaryDirectory() as tmp:
            a, _ = evaluate(tmp, "t0", "t0")
            STATE["wrong_types"] = {"side_to_move", "in_check", "captures", "material", "piece_on_square",
                                    "attackers_of", "defenders_of", "legal_moves_of"}
            b, _ = evaluate(tmp, "t2", "t2")
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                self.assertEqual(compare.main([str(a), str(b)]), 0)
        text = out.getvalue()
        self.assertIn("ARMS of one matched S0 corpus (play vs play-cgt)", text)
        self.assertNotIn("NOT COMPARABLE", text)
        self.assertIn("s0_rules_qa.overall.claim_failure_rate", text)
        line = next(l for l in text.splitlines() if l.startswith("s0_rules_qa.overall.claim_failure_rate"))
        self.assertTrue(line.rstrip().endswith("down"))


class KindGuard(unittest.TestCase):
    def test_play_and_play_cgt_without_a_matched_corpus_are_not_comparable(self):
        a = {"axes": {}, "inputs": {}, "prompt_kind": "play"}
        b = {"axes": {}, "inputs": {}, "prompt_kind": "play-cgt"}
        self.assertTrue(compare.comparability(a, b)[0].startswith(compare.PROMPT_MISMATCH))
        self.assertTrue(compare.comparability(b, dict(b, prompt_kind="move"))[0].startswith(compare.PROMPT_MISMATCH))
        self.assertEqual(compare.comparability(b, dict(b)), [])

    def test_mismatched_matched_keys_do_not_unlock_the_arms_exception(self):
        a = {"axes": {}, "prompt_kind": "play", "inputs": {"corpus": {"matched_sha256": "x"}}}
        b = {"axes": {}, "prompt_kind": "play-cgt", "inputs": {"corpus": {"matched_sha256": "y"}}}
        self.assertTrue(compare.comparability(a, b)[0].startswith(compare.PROMPT_MISMATCH))

    def test_unknown_kind_is_refused(self):
        with self.assertRaisesRegex(ValueError, "unknown prompt kind"):
            compare.comparability({"axes": {}, "prompt_kind": "play-cgt-x"}, {"axes": {}})

    def test_run_accepts_the_new_kinds(self):
        from model.eval.scorecard import PROMPT_KINDS
        self.assertEqual(PROMPT_KINDS, ("move", "play", "play-cgt", "play-cgt-full"))


if __name__ == "__main__":
    unittest.main()
