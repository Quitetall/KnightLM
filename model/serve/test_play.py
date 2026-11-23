"""Play-mode serving checks, CPU only: refusal codes, constrained path, and the move-only guarantees."""

import hashlib
import json
import os
import tempfile
import unittest
from pathlib import Path

from model.prompt import format_play_completion, play_text
from model.serve import play
from model.serve.inference import MODES, PlayPrediction, Prediction
from model.serve.test_serve import PACKET, FakePredictor

LEGAL = PACKET["legal_moves"]
SAY = "White to move. I choose e2e4."


def good(move="e2e4", reasoning=SAY):
    return format_play_completion(reasoning, move)


class InterpretTests(unittest.TestCase):
    def test_valid_completion(self):
        r = play.interpret(good(), LEGAL)
        self.assertEqual((r.code, r.move, r.reasoning, r.stated_move), ("ok", "e2e4", SAY, "e2e4"))

    def test_each_malformed_completion_has_its_stable_code(self):
        call = good()
        cases = {
            "missing tool_call": ("I choose e2e4.", "not_one_call"),
            "empty": ("", "not_one_call"),
            "two calls": (call + call, "not_one_call"),
            "text before the call": ("Sure! " + call, "extra_text"),
            "text after the call": (call + " Good luck.", "extra_text"),
            "reasoning after move": ('<tool_call>{"name":"submit_move","arguments":{"move":"e2e4","reasoning":"%s"}}'
                                     "</tool_call>" % SAY, "key_order"),
            "not json": ("<tool_call>submit_move e2e4</tool_call>", "malformed"),
            "wrong tool": (call.replace("submit_move", "resign"), "wrong_tool"),
            "no reasoning": ('<tool_call>{"name":"submit_move","arguments":{"move":"e2e4"}}</tool_call>',
                             "reasoning_missing"),
            "blank reasoning": (good(reasoning="  "), "reasoning_missing"),
            "reasoning too long": (good(reasoning="x" * 401), "reasoning_too_long"),
            "move not uci": (good(move="Nf3"), "bad_move"),
            "illegal move": (good(move="e2e5", reasoning="I choose e2e5."), "illegal_move"),
            "move differs from the stated move": (good(move="g1f3"), "stated_move_mismatch"),
        }
        for name, (text, code) in cases.items():
            with self.subTest(name):
                r = play.interpret(text, LEGAL)
                self.assertEqual(r.code, code)
                self.assertIn(r.code, play.PLAY_CODES)

    def test_a_refused_move_is_still_reported_for_the_record(self):
        r = play.interpret(good(move="e2e5", reasoning="Advance."), LEGAL)
        self.assertEqual((r.code, r.move), ("illegal_move", "e2e5"))

    def test_reasoning_that_states_no_move_is_not_a_mismatch(self):
        self.assertEqual(play.interpret(good(reasoning="Develop the knight."), LEGAL).code, "ok")


class ResolveTests(unittest.TestCase):
    def constrain(self, calls):
        def choose(prefix, suffix):
            calls.append((prefix, suffix))
            return "g1f3", 3
        return choose

    def test_raw_never_repairs(self):
        calls = []
        for text in (good(move="e2e5", reasoning="x"), "nonsense", good(move="g1f3")):
            fields, extra = play.resolve(text, LEGAL, "raw", self.constrain(calls))
            self.assertFalse(fields["legal"])
            self.assertEqual((fields["path"], extra), ("none", 0))
        self.assertEqual(calls, [])

    def test_constrained_keeps_a_valid_model_completion(self):
        calls = []
        fields, extra = play.resolve(good(), LEGAL, "constrained", self.constrain(calls))
        self.assertEqual((fields["move"], fields["path"], fields["code"], extra), ("e2e4", "model", "ok", 0))
        self.assertEqual(calls, [])

    def test_constrained_chooses_the_move_after_the_reasoning(self):
        calls = []
        text = good(move="e2e5", reasoning="Advance the king pawn. It is wide open.")
        fields, extra = play.resolve(text, LEGAL, "constrained", self.constrain(calls))
        self.assertEqual((fields["move"], fields["path"], fields["code"], fields["raw_code"], fields["legal"]),
                         ("g1f3", "constrained_move", "ok", "illegal_move", True))
        self.assertEqual(fields["reasoning"], "Advance the king pawn. It is wide open.")
        (prefix, suffix), = calls
        self.assertEqual(prefix + "e2e5" + suffix, text)  # the model's own reasoning, byte for byte
        self.assertTrue(prefix.endswith('"move":"'))

    def test_constrained_repairs_a_mismatch_but_keeps_the_stated_move_visible(self):
        fields, _ = play.resolve(good(move="g1f3"), LEGAL, "constrained", self.constrain([]))
        self.assertEqual((fields["raw_code"], fields["path"], fields["stated_move"]),
                         ("stated_move_mismatch", "constrained_move", "e2e4"))

    def test_constrained_without_recoverable_reasoning_returns_no_move(self):
        calls = []
        unusable = ["nonsense", "Sure! " + good(), good(reasoning=""),
                    '<tool_call>{"name":"submit_move","arguments":{"move":"e2e4","reasoning":"r"}}</tool_call>']
        for text in unusable:
            fields, _ = play.resolve(text, LEGAL, "constrained", self.constrain(calls))
            self.assertEqual((fields["move"], fields["code"], fields["path"], fields["legal"]),
                             (None, "constrained_no_reasoning", "none", False))
        self.assertEqual(calls, [])

    def test_forced_reasoning_when_the_model_wrote_no_usable_call(self):
        calls = []
        fields, extra = play.resolve("garbage", LEGAL, "constrained", self.constrain(calls),
                                     lambda: ("White to move.", 7))
        self.assertEqual((fields["move"], fields["path"], fields["code"], fields["raw_code"], extra),
                         ("g1f3", "constrained_forced_reasoning", "ok", "not_one_call", 10))
        (prefix, suffix), = calls
        self.assertEqual(prefix + "e2e4" + suffix, good(reasoning="White to move."))
        for bad in (None, "", "  ", "x" * 401):  # an unusable forced reasoning is still no move
            fields, _ = play.resolve("garbage", LEGAL, "constrained", self.constrain([]), lambda b=bad: (b, 3))
            self.assertEqual((fields["move"], fields["code"]), (None, "constrained_no_reasoning"))

    def test_forced_text_is_cut_at_the_first_unescaped_quote(self):
        self.assertEqual(play.reasoning_from_forced('A \\"q\\" b","move":"e'), 'A "q" b')
        self.assertIsNone(play.reasoning_from_forced("never closes"))
        self.assertIsNone(play.reasoning_from_forced('bad \\x escape"'))
        self.assertEqual(play.call_head() + 'r"', good(reasoning="r")[:len(play.call_head()) + 2])

    def test_constrained_never_returns_an_illegal_move(self):
        with self.assertRaises(ValueError):
            play.resolve(good(move="e2e5", reasoning="x"), LEGAL, "constrained", lambda p, s: ("a1a1", 1))

    def test_prefix_survives_escapes(self):
        reasoning = 'A "quote", a backslash \\ and an é.\nNew line.'
        found = play.move_prefix(good(reasoning=reasoning))
        self.assertEqual(found[0], reasoning)
        self.assertEqual(found[1] + "e2e4" + found[2], good(reasoning=reasoning))


class MoveOnlyUnchangedTests(unittest.TestCase):
    def test_move_only_records_are_byte_identical_to_before_play_mode(self):
        from model.eval import fakes
        from model.eval.scorecard import predict_rows
        records = predict_rows(fakes.fixture_rows(), fakes.perfect(), ("raw", "constrained"), 1.0)
        digest = hashlib.sha256(json.dumps(records, sort_keys=True).encode()).hexdigest()
        # Computed on main at 428623c, before any play-mode change.
        self.assertEqual(digest, "d7da35757feaa31f686b5d8a0a7c2e907a7a5695c266926efade37304b7e4ca4")

    def test_move_prompt_and_modes_are_untouched(self):
        self.assertEqual(MODES, ("raw", "constrained"))
        self.assertNotEqual(play_text(PACKET), "")


class ServerTests(unittest.TestCase):
    def client(self, text, tmp, prompt="move", mode="raw"):
        try:
            from fastapi.testclient import TestClient
        except ImportError:
            self.skipTest("fastapi/httpx not installed")
        import model.serve.server as server

        (Path(tmp) / "tokenizer.json").write_text("{}")
        (Path(tmp) / "chat_template.json").write_text("{}")

        class Fake(FakePredictor):
            def predict_play(self, packet, mode):
                fields, extra = play.resolve(self.reply, packet["legal_moves"], mode, lambda p, s: ("g1f3", 2))
                return PlayPrediction(raw_reply=self.reply, input_tokens=9, output_tokens=5 + extra,
                                      latency_ms=1.0, **fields)

        predictor = Fake(text)
        predictor.base_dir = Path(tmp)
        return TestClient(server.create_app(predictor, mode, prompt)), server

    BODY = {"packet": PACKET, "packet_format_version": 1}

    def test_move_only_manifest_and_move_are_unchanged(self):
        with tempfile.TemporaryDirectory() as tmp:
            client, server = self.client("e2e4", tmp)
            manifest = client.get("/v1/manifest").json()
            self.assertNotIn("default_prompt", manifest)
            legacy = {"packet_format_version": 1, "default_mode": "raw", "engine_access": False,
                      **FakePredictor("e2e4").manifest()}
            canonical = json.dumps(legacy, sort_keys=True, separators=(",", ":"))
            self.assertEqual(manifest["manifest_hash"], hashlib.sha256(canonical.encode()).hexdigest())
            self.assertEqual(client.post("/v1/move", json=self.BODY).json(),
                             {"move": "e2e4", "manifest_hash": manifest["manifest_hash"], "legal": True,
                              "raw_reply": "e2e4"})

    def test_play_endpoint_reports_move_reasoning_code_and_path(self):
        with tempfile.TemporaryDirectory() as tmp:
            client, _ = self.client(good(), tmp)
            r = client.post("/v1/play", json=self.BODY)
            self.assertEqual(r.status_code, 200)
            body = r.json()
            self.assertEqual((body["move"], body["reasoning"], body["code"], body["path"], body["legal"]),
                             ("e2e4", SAY, "ok", "model", True))

    def test_play_refusals_are_502_by_default_and_a_200_record_when_raw_is_asked_for(self):
        with tempfile.TemporaryDirectory() as tmp:
            for text, code in (("nonsense", "not_one_call"), (good(move="e2e5", reasoning="x"), "illegal_move"),
                               (good(move="g1f3"), "stated_move_mismatch")):
                client, _ = self.client(text, tmp, mode="constrained")
                self.assertEqual(client.post("/v1/play?mode=raw", json=self.BODY).json()["code"], code)
                self.assertFalse(client.post("/v1/play?mode=raw", json=self.BODY).json()["legal"])
            client, _ = self.client("nonsense", tmp, mode="raw")
            self.assertEqual(client.post("/v1/play", json=self.BODY).status_code, 502)
            self.assertEqual(client.post("/v1/play?mode=constrained", json=self.BODY).status_code, 502)

    def test_constrained_play_repairs_only_the_move_and_says_so(self):
        with tempfile.TemporaryDirectory() as tmp:
            client, _ = self.client(good(move="e2e5", reasoning="Advance."), tmp, mode="constrained")
            body = client.post("/v1/play", json=self.BODY).json()
            self.assertEqual((body["move"], body["path"], body["code"], body["raw_code"]),
                             ("g1f3", "constrained_move", "ok", "illegal_move"))

    def test_prompt_play_routes_move_through_the_play_prompt(self):
        with tempfile.TemporaryDirectory() as tmp:
            client, _ = self.client(good(), tmp, prompt="play")
            self.assertEqual(client.get("/v1/manifest").json()["default_prompt"], "play")
            body = client.post("/v1/move", json=self.BODY).json()
            self.assertEqual((body["move"], body["legal"]), ("e2e4", True))
            client, _ = self.client("nonsense", tmp, prompt="play", mode="constrained")
            self.assertEqual(client.post("/v1/move", json=self.BODY).status_code, 502)

    def test_play_needs_a_play_capable_predictor(self):
        with tempfile.TemporaryDirectory() as tmp:
            client, server = self.client("e2e4", tmp)
            plain = FakePredictor("e2e4")
            plain.base_dir = Path(tmp)
            from fastapi.testclient import TestClient
            self.assertEqual(TestClient(server.create_app(plain, "raw")).post("/v1/play", json=self.BODY).status_code,
                             501)


@unittest.skipUnless(os.environ.get("KNIGHTLM_BASE_DIR"), "GPU test: set KNIGHTLM_BASE_DIR to the pinned snapshot")
class GpuPlayTests(unittest.TestCase):
    """GPU TEST. The untrained base does not write a valid call, so constrained must supply the head."""

    def test_raw_is_checked_and_constrained_is_always_legal(self):
        import torch
        if not torch.cuda.is_available():
            self.skipTest("no CUDA")
        from model.serve.inference import Predictor
        predictor = Predictor(os.environ["KNIGHTLM_BASE_DIR"])
        raw = predictor.predict_play(PACKET, "raw")
        self.assertIn(raw.code, play.PLAY_CODES)
        self.assertEqual(raw.legal, raw.code == "ok")
        constrained = predictor.predict_play(PACKET, "constrained")
        self.assertIn(constrained.move, LEGAL)
        self.assertEqual(constrained.code, "ok")
        self.assertIn(constrained.path, play.PATHS)
        self.assertEqual(constrained.raw_code, raw.code)
        with self.assertRaises(ValueError):
            predictor.predict_play(PACKET, "bogus")


if __name__ == "__main__":
    unittest.main()
