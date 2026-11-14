"""materialize_corpus conversion: deterministic, prompt-identical, refuses leaks."""

import contextlib
import io
import json
from pathlib import Path
import shutil
import tempfile
import unittest

from model.corpus.pipeline import read_corpus
from model.corpus.test_pipeline import plant_leak, write_fixture
from model.prompt import user_chat
from model.tests import _payload  # noqa: F401  (sys.path)
from knightlm_blut import materialize as mat

ROOT = _payload.ROOT


class MaterializeTests(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.corpus = self.tmp / "corpus"
        write_fixture(self.corpus, shard_games=10)  # 25 train rows in 3 shards

    def run_to(self, name):
        out = self.tmp / name
        return out, mat.materialize(self.corpus, out, ROOT)

    def test_prompt_bytes_are_model_prompt_and_completion_is_the_target(self):
        train, _ = read_corpus(self.corpus)
        out, receipt = self.run_to("a")
        lines = (out / "dataset.jsonl").read_bytes().splitlines()
        self.assertEqual(len(lines), len(train))
        for line, row in zip(lines, train, strict=True):
            record = json.loads(line)
            served = user_chat(row["packet"])[0]["content"][0]["text"]
            self.assertEqual(record["prompt"].encode(), served.encode())
            self.assertEqual(record["completion"], row["target"])
            self.assertEqual(sorted(record), ["completion", "prompt"])
        self.assertEqual(receipt["rows"], len(train))

    def test_two_runs_are_byte_identical_with_per_shard_hashes(self):
        a, ra = self.run_to("a")
        b, rb = self.run_to("b")
        self.assertEqual((a / "dataset.jsonl").read_bytes(), (b / "dataset.jsonl").read_bytes())
        self.assertEqual(ra["dataset_sha256"], rb["dataset_sha256"])
        self.assertEqual(ra["shards"], rb["shards"])
        self.assertEqual([s["rows"] for s in ra["shards"]], [10, 10, 5])
        for shard in ra["shards"]:
            self.assertEqual(len(shard["converted_sha256"]), 64)

    def test_valid_result_is_reused_and_a_tampered_one_is_rebuilt(self):
        out, first = self.run_to("a")
        self.assertFalse(first["reused"])
        original = (out / "dataset.jsonl").read_bytes()
        self.assertTrue(mat.materialize(self.corpus, out, ROOT)["reused"])
        (out / "dataset.jsonl").write_bytes(original[:-5] + b"XXXXX")
        again = mat.materialize(self.corpus, out, ROOT)
        self.assertFalse(again["reused"])
        self.assertEqual((out / "dataset.jsonl").read_bytes(), original)

    def test_changed_prompt_pin_invalidates_reuse(self):
        out, _ = self.run_to("a")
        receipt = json.loads((out / "receipt.json").read_text())
        receipt["prompt_py_sha256"] = "0" * 64
        (out / "receipt.json").write_text(json.dumps(receipt))
        self.assertFalse(mat.materialize(self.corpus, out, ROOT)["reused"])

    def test_planted_leak_is_refused_before_any_output(self):
        for kind in ("game_id", "family", "position_key", "time"):
            with self.subTest(kind=kind):
                corpus = self.tmp / f"leaky-{kind}"
                write_fixture(corpus, shard_games=10)
                plant_leak(corpus, kind)
                out = self.tmp / f"out-{kind}"
                with self.assertRaises(mat.MaterializeError) as caught:
                    mat.materialize(corpus, out, ROOT)
                self.assertIn("corpus audit failed", str(caught.exception))
                self.assertFalse((out / "dataset.jsonl").exists())

    def test_cli_exit_codes(self):
        out = self.tmp / "cli"
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            ok = mat.main(["--corpus", str(self.corpus), "--out", str(out)])
            refused = mat.main(["--corpus", str(self.corpus), "--out", str(self.tmp / "x"),
                                "--expect-manifest-sha256", "0" * 64])
        self.assertEqual((ok, refused), (0, 1))

    def test_shard_edited_after_the_audit_is_refused(self):
        train, _ = read_corpus(self.corpus)
        manifest = json.loads((self.corpus / "manifest.json").read_text())
        shard = self.corpus / manifest["shards"]["train"]["files"][0]["file"]
        original = shard.read_bytes()
        calls = []
        real_audit = mat.run_audit
        def audit_then_edit(corpus, root):
            note = real_audit(corpus, root)
            shard.write_bytes(original.replace(b"e2e4", b"d2d4", 1))  # edit after the audit passed
            calls.append(1)
            return note
        mat.run_audit = audit_then_edit
        try:
            with self.assertRaises(mat.MaterializeError) as caught:
                mat.materialize(self.corpus, self.tmp / "late", ROOT)
        finally:
            mat.run_audit = real_audit
        self.assertIn("changed since the audit", str(caught.exception))


if __name__ == "__main__":
    unittest.main()
