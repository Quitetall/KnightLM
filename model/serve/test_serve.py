"""CPU-only tests: prompt identity, constrained trie, HTTP contract with a fake predictor.

The GPU test (real SmolVLM) runs only when KNIGHTLM_BASE_DIR points at the pinned
snapshot and CUDA is available; it is skipped otherwise.
"""

import json
import os
import unittest

from model.prompt import INSTRUCTION, user_chat, user_text
from model.serve.inference import MODES, Prediction, allowed_next, build_trie

PACKET = {"schema_version": 1, "ruleset": "standard", "fen": "startpos", "legal_moves": ["e2e4", "g1f3"],
          "facts": {"b": 1, "a": 2}}


def old_harness_chat(packet):
    """The expression model/train/harness.py used before the prompt was extracted."""
    packet = json.dumps(packet, sort_keys=True, separators=(",", ":"))
    return [{"role": "user", "content": [{"type": "text", "text": (
        "Choose a legal chess move. Reply with just the UCI move.\n" + packet
    )}]}]


class PromptTests(unittest.TestCase):
    def test_byte_identical_to_pre_extraction_harness(self):
        self.assertEqual(user_chat(PACKET), old_harness_chat(PACKET))
        self.assertEqual(json.dumps(user_chat(PACKET)), json.dumps(old_harness_chat(PACKET)))

    def test_golden_text(self):
        self.assertEqual(user_text(PACKET), (
            "Choose a legal chess move. Reply with just the UCI move.\n"
            '{"facts":{"a":2,"b":1},"fen":"startpos","legal_moves":["e2e4","g1f3"],'
            '"ruleset":"standard","schema_version":1}'))
        self.assertTrue(INSTRUCTION.endswith("\n"))

    def test_harness_uses_the_shared_prompt(self):
        import model.train.harness as harness
        self.assertIs(harness.user_chat, user_chat)


class TrieTests(unittest.TestCase):
    def test_only_continuations_of_legal_sequences(self):
        trie = build_trie([[1, 2, 9], [1, 3, 9], [4, 9]])
        self.assertEqual(sorted(allowed_next(trie, [])), [1, 4])
        self.assertEqual(sorted(allowed_next(trie, [1])), [2, 3])
        self.assertEqual(allowed_next(trie, [4]), [9])
        self.assertEqual(allowed_next(trie, [1, 2]), [9])

    def test_off_trie_prefix_allows_nothing(self):
        trie = build_trie([[1, 2, 9]])
        self.assertEqual(allowed_next(trie, [7]), [])
        self.assertEqual(allowed_next(trie, [1, 5]), [])


class FakePredictor:
    base_dir = None
    base_sha256 = "b" * 64
    adapter_sha256 = None

    def __init__(self, reply):
        self.reply = reply

    def predict(self, packet, mode):
        if mode not in MODES:
            raise ValueError("bad mode")
        return Prediction(self.reply.strip(), self.reply, self.reply.strip() in packet["legal_moves"], 10, 3, 1.0)


class ServerContractTests(unittest.TestCase):
    def client(self, reply, tmp):
        try:
            from fastapi.testclient import TestClient
        except ImportError:
            self.skipTest("fastapi/httpx not installed")
        from pathlib import Path
        import model.serve.server as server
        (Path(tmp) / "tokenizer.json").write_text("{}")
        (Path(tmp) / "chat_template.json").write_text("{}")
        predictor = FakePredictor(reply)
        predictor.base_dir = Path(tmp)
        return TestClient(server.create_app(predictor, "raw"))

    def test_move_health_manifest(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            client = self.client("e2e4", tmp)
            self.assertEqual(client.get("/v1/health").json()["engine_access"], False)
            manifest = client.get("/v1/manifest").json()
            self.assertEqual(manifest["adapter_sha256"], "none")
            for key in ("base_sha256", "prompt_sha256", "tokenizer_sha256", "chat_template_sha256"):
                self.assertTrue(manifest[key])
            body = {"packet": PACKET, "packet_format_version": 1}
            response = client.post("/v1/move", json=body)
            self.assertEqual(response.status_code, 200)
            self.assertEqual(response.json(), {"move": "e2e4", "manifest_hash": manifest["manifest_hash"]})

    def test_errors_are_http_errors_never_a_guess(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            client = self.client("zzzz", tmp)
            self.assertEqual(client.post("/v1/move", json={"packet": PACKET, "packet_format_version": 1}).status_code, 502)
            self.assertEqual(client.post("/v1/move", json={"packet": PACKET, "packet_format_version": 2}).status_code, 400)
            self.assertEqual(client.post("/v1/move", json={"packet": {"fen": "x"}, "packet_format_version": 1}).status_code, 422)
            self.assertEqual(client.post("/v1/move", json={"packet": "str", "packet_format_version": 1}).status_code, 422)


@unittest.skipUnless(os.environ.get("KNIGHTLM_BASE_DIR"), "GPU test: set KNIGHTLM_BASE_DIR to the pinned snapshot")
class GpuPredictorTests(unittest.TestCase):
    """GPU TEST. Loads the real base model."""

    def test_constrained_is_always_legal_and_counts_tokens(self):
        import torch
        if not torch.cuda.is_available():
            self.skipTest("no CUDA")
        from model.serve.inference import Predictor
        predictor = Predictor(os.environ["KNIGHTLM_BASE_DIR"])
        raw = predictor.predict(PACKET, "raw")
        constrained = predictor.predict(PACKET, "constrained")
        self.assertIn(constrained.move, PACKET["legal_moves"])
        self.assertTrue(constrained.legal)
        self.assertEqual(raw.legal, raw.move in PACKET["legal_moves"])
        self.assertGreater(raw.input_tokens, 20)
        self.assertGreaterEqual(raw.output_tokens, 1)
        with self.assertRaises(ValueError):
            predictor.predict(PACKET, "bogus")


if __name__ == "__main__":
    unittest.main()
