"""Play-mode evaluation: scoring the completion, the reasoning, and never mixing prompt kinds."""

import contextlib
import io
import json
import tempfile
import unittest
from pathlib import Path

from model.eval import compare, fakes, run
from model.eval.predictors import load_predictor
from model.eval.scorecard import build_scorecard, predict_rows
from model.eval.test_eval import make_corpus

MODES = ("raw", "constrained")


def play_card():
    rows = fakes.fixture_rows()
    records = predict_rows(rows, fakes.play_scripted(), MODES, 1.0, "play")
    return rows, records, build_scorecard(records, MODES, None, None, len(rows), "play")


class ScorecardTests(unittest.TestCase):
    def test_raw_status_per_scripted_failure(self):
        _, records, card = play_card()
        self.assertEqual([r["raw"]["status"] for r in records],
                         ["ok", "unparseable", "illegal", "unparseable", "inconsistent", "unparseable"])
        self.assertEqual([r["raw"]["play_code"] for r in records],
                         ["ok", "extra_text", "illegal_move", "not_one_call", "stated_move_mismatch", "key_order"])
        self.assertEqual(card["prompt_kind"], "play")
        self.assertEqual(card["axes"]["play_raw_legality"]["legal"], 1)
        self.assertEqual(card["axes"]["play_raw_legality"]["n"], 6)
        self.assertEqual(card["axes"]["failures"]["raw"]["by_kind"]["inconsistent"], 1)

    def test_play_axes_have_their_own_names(self):
        _, _, card = play_card()
        self.assertIn("play_raw_legality", card["axes"])
        self.assertIn("play_constrained_legality", card["axes"])
        self.assertNotIn("raw_legality", card["axes"])
        self.assertNotIn("constrained_legality", card["axes"])

    def test_constrained_repairs_only_where_reasoning_is_recoverable(self):
        _, records, card = play_card()
        self.assertEqual([r["constrained"]["play_path"] for r in records],
                         ["model", "none", "constrained_move", "none", "constrained_move", "none"])
        self.assertEqual([r["constrained"]["status"] for r in records],
                         ["ok", "unparseable", "ok", "unparseable", "ok", "unparseable"])
        block = card["axes"]["play_completion"]["constrained"]
        self.assertEqual(block["by_path"], {"model": 1, "constrained_move": 2, "constrained_forced_reasoning": 0, "none": 3})
        self.assertEqual(block["by_code"]["constrained_no_reasoning"], 3)
        self.assertEqual(block["model_completion_valid"]["valid"], 1)
        self.assertEqual(block["completion_valid"]["valid"], 3)
        # every constrained answer that returns a move is legal; the rest return none
        for r, row in zip(records, fakes.fixture_rows(), strict=True):
            move = r["constrained"]["move"]
            self.assertTrue(move is None or move in row["packet"]["legal_moves"])

    def test_parsed_move_legal_is_reported_beside_the_completion_gate(self):
        _, _, card = play_card()
        raw = card["axes"]["play_completion"]["raw"]
        self.assertEqual(raw["completion_valid"]["valid"], 1)
        self.assertEqual(raw["parsed_move_legal"]["legal"], 2)  # row 4 is legal but disagrees with its reasoning
        self.assertEqual(raw["by_code"]["illegal_move"], 1)

    def test_a_crashing_play_predictor_is_an_exception_row(self):
        rows = fakes.fixture_rows()

        class Crash:
            def predict(self, packet, mode):
                raise RuntimeError("500")

        records = predict_rows(rows, Crash(), MODES, 1.0, "play")
        card = build_scorecard(records, MODES, None, None, len(rows), "play")
        self.assertEqual(card["axes"]["failures"]["raw"]["by_kind"]["exception"], 6)
        self.assertEqual(card["axes"]["play_completion"]["raw"]["by_code"]["no_reply"], 6)


class RunTests(unittest.TestCase):
    def run_cli(self, tmp, label, spec, *extra):
        argv = ["--predictor", spec, "--label", label, "--rows", "6", "--corpus", str(make_corpus(Path(tmp) / "c")),
                "--out-root", str(Path(tmp) / "out"), "--no-stockfish", "--no-tactics", "--timeout-s", "1", *extra]
        with contextlib.redirect_stdout(io.StringIO()):
            return run.main(argv)

    def test_play_run_scores_reasoning_and_labels_the_kind(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = self.run_cli(tmp, "p", "model.eval.fakes:play_scripted", "--prompt-kind", "play")
            card = json.loads((out / "scorecard.json").read_text())
            self.assertEqual(card["prompt_kind"], "play")
            self.assertEqual(card["inputs"]["settings"]["prompt_kind"], "play")
            self.assertEqual(len(card["inputs"]["settings"]["play_prompt_sha256"]), 64)
            expl = card["axes"]["play_explanation"]["raw"]
            self.assertEqual((expl["n"], expl["explanations_missing"]), (6, 3))  # rows 1, 3, 5 never parsed
            self.assertIn("Prompt kind: **play**", (out / "scorecard.md").read_text())
            first = json.loads((out / "predictions.jsonl").read_text().splitlines()[0])
            self.assertIn("I choose", first["raw"]["reasoning"])

    def test_http_play_spec_uses_the_play_endpoint(self):
        predictor, desc = load_predictor("http://127.0.0.1:1", "play")
        self.assertEqual(predictor.play_url, "http://127.0.0.1:1/v1/play")
        self.assertEqual(desc["kind"], "http")

    def test_compare_refuses_to_mix_prompt_kinds(self):
        with tempfile.TemporaryDirectory() as tmp:
            move = self.run_cli(tmp, "m", "model.eval.fakes:perfect")
            play = self.run_cli(tmp, "p", "model.eval.fakes:play_scripted", "--prompt-kind", "play")
            out, err = io.StringIO(), io.StringIO()
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                self.assertEqual(compare.main([str(move), str(play)]), 3)
            self.assertIn("NOT COMPARABLE: prompt kind differs (move vs play)", out.getvalue())
            self.assertNotIn("axis ", out.getvalue())
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                self.assertEqual(compare.main(["--allow-prompt-mismatch", str(move), str(play)]), 0)
            self.assertIn("ALLOWED", out.getvalue())
            self.assertIn("NOT COMPARABLE: prompt kind differs", out.getvalue())
            # play against play is a normal comparison
            other = self.run_cli(tmp, "q", "model.eval.fakes:play_scripted", "--prompt-kind", "play")
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                self.assertEqual(compare.main([str(play), str(other)]), 0)
            self.assertNotIn("NOT COMPARABLE", out.getvalue())

    def test_a_card_without_prompt_kind_is_a_move_card(self):
        a = {"axes": {}, "inputs": {}}
        b = {"axes": {}, "inputs": {}, "prompt_kind": "move"}
        self.assertEqual(compare.comparability(a, b), [])
        self.assertTrue(compare.comparability(a, dict(b, prompt_kind="play")))

    def test_move_only_run_is_still_a_move_card_with_the_old_axes(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = self.run_cli(tmp, "m", "model.eval.fakes:perfect")
            card = json.loads((out / "scorecard.json").read_text())
            self.assertEqual(card["prompt_kind"], "move")
            self.assertIn("raw_legality", card["axes"])
            self.assertNotIn("play_completion", card["axes"])
            self.assertNotIn("play_explanation", card["axes"])
            first = json.loads((out / "predictions.jsonl").read_text().splitlines()[0])
            self.assertNotIn("play_code", first["raw"])


if __name__ == "__main__":
    unittest.main()
