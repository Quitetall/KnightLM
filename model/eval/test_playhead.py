"""The move_head axis in model.eval.run, the play-head agreement axis, and the board-arm factories (fakes only)."""

import contextlib
import io
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from model.eval import compare, fakes, playmode, run
from model.eval.predictors import load_predictor
from model.eval.test_eval import make_corpus
from model.policy_head import space
from model.serve.head import pick_from_logits, resolve_head
from model.serve.inference import PlayPrediction


class HeadScripted(fakes.Fake):
    """A play-head predictor: the head picks the human move except on row 5 (another legal move); the completion text
    is scripted per row and run through the real agreement logic (model.serve.head.resolve_head).

    row 0 agrees; 1 writes another move field with no reasoning conflict; 2 states another move; 3 writes no call;
    4 agrees; 5 agrees with its (wrong) head pick."""

    def head_move(self, i):
        return self.other_legal(5) if i == 5 else self.target(i)

    def logits(self, packet):
        i = self.index[packet["fen"]]
        out = [0.0] * space.SIZE
        out[space.INDEX[self.head_move(i)]] = 8.0
        return out

    def predict(self, packet, mode):
        from model.prompt import format_play_completion as fmt
        i = self.index[packet["fen"]]
        head = pick_from_logits(self.logits(packet), packet["legal_moves"]).move
        other = next(m for m in packet["legal_moves"] if m != head)
        text = [fmt(f"Develops. I choose {head}.", head), fmt("A quiet move.", other), fmt(f"I choose {other}.", other),
                "I would play it.", fmt("Fine.", head), fmt("Fine.", head)][i]
        fields, extra = resolve_head(text, packet["legal_moves"], head, mode, lambda: ("Develops a piece.", 6))
        return PlayPrediction(raw_reply=text, input_tokens=300, output_tokens=40 + extra, latency_ms=15.0,
                              head_move=head, **fields)


def head_scripted():
    return HeadScripted()


class Plain(fakes.Perfect):
    pass


class RunTests(unittest.TestCase):
    def run_cli(self, tmp, label, spec, *extra):
        argv = ["--predictor", spec, "--label", label, "--rows", "6", "--corpus", str(make_corpus(Path(tmp) / "c")),
                "--out-root", str(Path(tmp) / "out"), "--no-stockfish", "--no-tactics", "--timeout-s", "1", *extra]
        with contextlib.redirect_stdout(io.StringIO()):
            return run.main(argv)

    def card(self, out):
        return json.loads((out / "scorecard.json").read_text())

    def test_the_axis_is_off_by_default(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = self.run_cli(tmp, "h", "model.eval.test_playhead:head_scripted", "--prompt-kind", "play-head")
            card = self.card(out)
            self.assertNotIn("move_head", card["axes"])
            self.assertNotIn("move_head", card["inputs"]["settings"])
            first = json.loads((out / "predictions.jsonl").read_text().splitlines()[0])
            self.assertNotIn("move_head", first)
            self.assertEqual(card["prompt_kind"], "play-head")

    def test_move_head_axis_and_agreement_are_scored_beside_the_completion(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = self.run_cli(tmp, "h", "model.eval.test_playhead:head_scripted", "--prompt-kind", "play-head", "--move-head")
            card = self.card(out)
            axis = card["axes"]["move_head"]
            self.assertEqual(axis["label"], "human-only move head")
            self.assertEqual(axis["rows"], 6)
            self.assertEqual(axis["masked"]["top1_match"]["count"], 5)  # row 5 picks another legal move
            self.assertEqual(axis["raw"]["argmax_legal"]["rate"], 1.0)
            self.assertTrue(card["inputs"]["settings"]["move_head"])
            agree = card["axes"]["head_agreement"]
            self.assertEqual(agree["label"], "human-only move head")
            self.assertEqual(agree["raw"]["agreed"]["count"], 3)  # rows 0, 4, 5
            self.assertEqual(agree["raw"]["by_code"], {"not_one_call": 1, "ok": 3, "stated_move_mismatch": 2})
            self.assertEqual(agree["raw"]["head_move_matches_target"]["count"], 5)
            # constrained: row 1 repaired to the head's move; row 2 states another move so it stays a mismatch;
            # row 3 gets reasoning written after the call head
            self.assertEqual(agree["constrained"]["agreed"]["count"], 5)
            self.assertEqual(agree["constrained"]["by_code"], {"ok": 5, "stated_move_mismatch": 1})
            first = json.loads((out / "predictions.jsonl").read_text().splitlines()[0])
            self.assertEqual(first["move_head"]["masked"]["status"], "ok")
            self.assertEqual(first["raw"]["head_move"], first["target"])
            text = (out / "scorecard.md").read_text()
            self.assertIn("Move head (human-only move head)", text)
            self.assertIn("Agreement of the written completion", text)

    def test_move_head_needs_a_predictor_with_logits(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(SystemExit):
                self.run_cli(tmp, "x", "model.eval.fakes:perfect", "--move-head")

    def test_move_head_with_a_move_prompt_predictor_and_compare(self):
        with tempfile.TemporaryDirectory() as tmp:
            a = self.run_cli(tmp, "a", "model.eval.test_playhead:head_scripted", "--prompt-kind", "play-head", "--move-head")
            b = self.run_cli(tmp, "b", "model.eval.test_playhead:head_scripted", "--prompt-kind", "play-head", "--move-head")
            plain = self.run_cli(tmp, "c", "model.eval.test_playhead:head_scripted", "--prompt-kind", "play-head")
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                self.assertEqual(compare.main([str(a), str(b)]), 0)
            self.assertIn("move_head.masked.top1_match.rate", out.getvalue())
            self.assertNotIn("NOT COMPARABLE", out.getvalue())
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                self.assertEqual(compare.main([str(plain), str(a)]), 0)  # one side lacks the axis: a gap, not a zero
            self.assertIn("move_head.masked.top1_match.rate", out.getvalue())

    def test_compare_directions_for_the_head_leaves(self):
        self.assertEqual(compare.direction("move_head.masked.calibration.ece"), "down")
        self.assertEqual(compare.direction("move_head.raw.illegal_mass_mean"), "down")
        self.assertEqual(compare.direction("move_head.masked.top1_match.rate"), "up")
        self.assertEqual(compare.direction("move_head.centipawn_loss.masked.mean"), "down")

    def test_head_kind_does_not_compare_with_the_plain_play_kind(self):
        with tempfile.TemporaryDirectory() as tmp:
            head = self.run_cli(tmp, "h", "model.eval.test_playhead:head_scripted", "--prompt-kind", "play-head")
            play = self.run_cli(tmp, "p", "model.eval.fakes:play_scripted", "--prompt-kind", "play")
            out = io.StringIO()
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(compare.main([str(play), str(head)]), 3)
            self.assertIn("NOT COMPARABLE: prompt kind differs (play vs play-head)", out.getvalue())


class FactoryTests(unittest.TestCase):
    def test_play_adapter_exposes_logits_only_when_the_inner_predictor_has_them(self):
        self.assertTrue(callable(playmode.PlayAdapter(HeadScripted()).logits))
        self.assertFalse(hasattr(playmode.PlayAdapter(fakes.Perfect()), "logits"))

    def test_from_env_passes_the_board_directories(self):
        calls = []

        class Recorder:
            prompt_kind = "play"

            def __init__(self, *args, **kwargs):
                calls.append((args, kwargs))

        env = {"KNIGHTLM_BASE_DIR": "/b", "KNIGHTLM_ADAPTER_DIR": "/a", "KNIGHTLM_DEVICE": "cpu",
               "KNIGHTLM_ENCODER_DIR": "/e", "KNIGHTLM_PROJECTOR_DIR": "/p"}
        with mock.patch.dict(os.environ, env), mock.patch("model.serve.inference.Predictor", Recorder):
            adapter = playmode.from_env()
        self.assertEqual(calls[0], (("/b", "/a", "cpu"), {"encoder_dir": "/e", "projector_dir": "/p"}))
        self.assertIsInstance(adapter, playmode.PlayAdapter)

    def test_from_env_without_a_board_is_the_plain_predictor(self):
        calls = []

        class Recorder:
            def __init__(self, *args, **kwargs):
                calls.append((args, kwargs))

        env = {"KNIGHTLM_BASE_DIR": "/b", "KNIGHTLM_ADAPTER_DIR": "", "KNIGHTLM_DEVICE": "cpu"}
        with mock.patch.dict(os.environ, env), mock.patch("model.serve.inference.Predictor", Recorder):
            playmode.from_env()
        self.assertEqual(calls[0], (("/b", None, "cpu"), {"encoder_dir": None, "projector_dir": None}))

    def test_head_from_env_builds_a_head_predictor(self):
        calls = []

        class Recorder:
            def __init__(self, *args, **kwargs):
                calls.append((args, kwargs))

        env = {"KNIGHTLM_BASE_DIR": "/b", "KNIGHTLM_DEVICE": "cpu", "KNIGHTLM_HEAD_DIR": "/h"}
        with mock.patch.dict(os.environ, env), mock.patch("model.serve.head.HeadPredictor", Recorder):
            playmode.head_from_env()
        self.assertEqual(calls[0], (("/b", None, "cpu"), {"head_dir": "/h"}))

    def test_a_python_predictor_is_told_its_board_or_head_kind(self):
        for kind in ("play-head", "play-enc", "play-enc-cgt"):
            predictor, _ = load_predictor("model.eval.test_playhead:enc_fake", kind)
            self.assertEqual(predictor.inner.prompt_kind, kind)


class EncFake(fakes.Fake):
    prompt_kind = "play"

    def predict_play(self, packet, mode):
        raise NotImplementedError


def enc_fake():
    return EncFake()


class SettingsTests(unittest.TestCase):
    def test_the_enc_cgt_arm_pins_the_compiler_and_the_plain_enc_arm_does_not(self):
        with mock.patch("model.prompt.knightlm_binary", lambda: Path(__file__)):
            cgt = run.cgt_settings("play-enc-cgt")
            self.assertEqual(cgt["cgt_variant"], "lite")
            self.assertEqual(len(cgt["compiler_sha256"]), 64)
        self.assertEqual(run.cgt_settings("play-enc"), {})
        self.assertEqual(run.cgt_settings("play-head"), {})


if __name__ == "__main__":
    unittest.main()
