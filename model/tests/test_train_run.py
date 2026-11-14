"""model.train.run on CPU with the corpus fixture: preflight, refusals, launch bookkeeping."""

import contextlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import stat
import tempfile
import unittest

from model.corpus.pipeline import MANIFEST
from model.corpus.test_pipeline import plant_leak, write_fixture
from model.tests import _payload  # noqa: F401
from model.train import bases, run

ROOT = _payload.ROOT


def call(argv):
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        code = run.main(argv)
    return code, out.getvalue(), err.getvalue()


class EntryPointTests(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.corpus = self.tmp / "corpus"
        write_fixture(self.corpus, shard_games=10)
        self.runs = self.tmp / "runs"
        self.base = ["--corpus", str(self.corpus), "--run-root", str(self.runs), "--skip-model"]

    def test_valid_fixture_prints_a_plan_and_launches_nothing(self):
        code, out, _ = call([*self.base, "--run", "r1"])
        self.assertEqual(code, 0, out)
        self.assertIn("[ok     ] corpus_audit", out)
        self.assertIn("[ok     ] prompt_parity", out)
        self.assertIn("[skipped] base_weights - --skip-model", out)
        self.assertIn("Nothing launched", out)
        self.assertFalse(self.runs.exists())

    def test_json_plan_carries_digest_estimate_and_command(self):
        code, out, _ = call([*self.base, "--run", "r1", "--json", "--batch-size", "5", "--grad-accum", "1"])
        plan = json.loads(out)
        self.assertEqual(plan["corpus"]["manifest_sha256"], run.sha256_file(self.corpus / MANIFEST))
        self.assertEqual(plan["estimate"]["optimizer_steps"], 5)  # 25 rows / 5 per step, 1 epoch
        self.assertEqual(plan["command"][1:4], ["recipe", "run", "knightlm_lora"])
        args = json.loads(plan["command"][-1])
        self.assertEqual((args["base"], args["batch_size"], args["run"]), ("smolvlm-500m", 5, "r1"))
        self.assertEqual(set(plan["skipped_checks"]), {"base_weights", "tokenisation",
                                                      "trainer_vs_harness_tokens", "cpu_steps"})

    def test_tampered_shard_is_refused(self):
        shard = self.corpus / json.loads((self.corpus / MANIFEST).read_text())["shards"]["train"]["files"][0]["file"]
        shard.write_bytes(shard.read_bytes().replace(b"e2e4", b"d2d4", 1))
        code, _, err = call([*self.base, "--run", "r1"])
        self.assertEqual(code, 2)
        self.assertIn("REFUSED: corpus audit failed", err)

    def test_planted_leak_is_refused(self):
        plant_leak(self.corpus, "position_key")
        code, _, err = call([*self.base, "--run", "r1"])
        self.assertEqual((code, "leakage" in err), (2, True))

    def test_existing_run_directory_is_refused_not_extended(self):
        (self.runs / "r1").mkdir(parents=True)
        code, _, err = call([*self.base, "--run", "r1"])
        self.assertEqual(code, 2)
        self.assertIn("choose a new --run", err)

    def test_resume_needs_an_existing_run(self):
        code, _, err = call([*self.base, "--run", "r1", "--resume"])
        self.assertEqual(code, 2)
        self.assertIn("--resume needs an existing run", err)

    def test_prompt_divergence_from_model_prompt_is_caught(self):
        from knightlm_blut import materialize
        real = materialize._prompt_module
        class Drifted:
            INSTRUCTION = "x"
            @staticmethod
            def user_text(packet):
                return "Pick a move.\n" + json.dumps(packet)
        materialize._prompt_module = lambda root: Drifted
        try:
            code, _, err = call([*self.base, "--run", "r1"])
        finally:
            materialize._prompt_module = real
        self.assertEqual(code, 2)
        self.assertIn("training prompt differs from model.prompt", err)

    def test_config_sanity_reports_deviations_and_warnings(self):
        _, out, _ = call([*self.base, "--run", "r1", "--lr", "0.01", "--epochs", "3", "--json"])
        sanity = json.loads(out)["sanity"]
        self.assertTrue(any("lr=0.01" in line for line in sanity["deviations_from_recommended"]))
        self.assertTrue(any("outside the usual LoRA range" in w for w in sanity["warnings"]))
        self.assertTrue(any("more than one epoch" in w for w in sanity["warnings"]))

    def test_gpu_without_a_binary_is_refused_before_anything_is_created(self):
        code, _, err = call([*self.base, "--run", "r1", "--gpu", "--blut-bin", str(self.tmp / "missing")])
        self.assertEqual(code, 2)
        self.assertIn("binary not found", err)
        self.assertFalse((self.runs / "r1").exists())

    def stub(self, body):
        path = self.tmp / "blut-stub"
        path.write_text("#!/bin/sh\n" + body)
        path.chmod(path.stat().st_mode | stat.S_IXUSR)
        return path

    def launch(self, body, name):
        os.environ["BLUT_HF_VENV_IMAGE_TEXT"] = str(self.tmp)
        self.addCleanup(os.environ.pop, "BLUT_HF_VENV_IMAGE_TEXT", None)
        return call([*self.base, "--run", name, "--gpu", "--blut-bin", str(self.stub(body))])

    def test_successful_launch_records_the_entry_manifest(self):
        # The recipe writes run-manifest.json; the stub does it from the args JSON.
        body = (f'python3 - "$5" <<\'PY\'\nimport json,sys\na=json.loads(sys.argv[1])\n'
                f'from pathlib import Path\nd=Path(a["run_root"])/a["run"]\nd.mkdir(parents=True,exist_ok=True)\n'
                f'(d/"run-manifest.json").write_text(json.dumps({{"adapter_sha256":"ab","corpus_manifest_sha256":"cd"}}))\nPY\n')
        code, out, _ = self.launch(body, "ok1")
        self.assertEqual(code, 0, out)
        entry = json.loads((self.runs / "ok1" / "entry.json").read_text())
        self.assertEqual(entry["status"], "succeeded")
        self.assertEqual(entry["corpus"]["manifest_sha256"], run.sha256_file(self.corpus / MANIFEST))
        self.assertEqual(entry["model"]["revision"], bases.BASES["smolvlm-500m"]["revision"])
        self.assertEqual(entry["seed"], 42)
        self.assertIn("recipe", entry["command"])

    def test_failed_launch_is_recorded_and_earlier_runs_are_untouched(self):
        self.launch('mkdir -p "$(dirname "$0")/x"; exit 0\n', "done1")  # no manifest => failed too
        first = (self.runs / "done1" / "entry.json").read_text()
        code, _, _ = self.launch("echo boom >&2; exit 7\n", "bad1")
        self.assertEqual(code, 3)
        entry = json.loads((self.runs / "bad1" / "entry.json").read_text())
        self.assertEqual((entry["status"], entry["returncode"]), ("failed", 7))
        self.assertEqual((self.runs / "done1" / "entry.json").read_text(), first)
        self.assertIn("boom", (self.runs / "bad1" / "blut.log").read_text())


class BaseTableTests(unittest.TestCase):
    def test_python_bases_match_the_rust_profiles(self):
        source = (ROOT / "apps/blut-knightlm/src/profile.rs").read_text()
        for key, base in bases.BASES.items():
            block = source[source.index(f'key: "{key}"'):]
            block = block[:block.index("BaseProfile {") if "BaseProfile {" in block else len(block)]
            for field, name in (("revision", "revision"), ("weights_sha256", "weights_sha256"),
                                ("weights_file", "weights_file"), ("dir", "dir"), ("repository", "repository")):
                self.assertIn(f'{name}: "{base[field]}"', block, f"{key}.{field}")

    def test_recommended_defaults_match_the_recipe(self):
        source = (ROOT / "apps/blut-knightlm/src/recipes/knightlm_lora.rs").read_text()
        for name, key in (("rank", "rank"), ("alpha", "alpha"), ("batch", "batch_size"), ("accum", "grad_accum"),
                          ("save_steps", "save_steps"), ("keep", "keep_checkpoints"), ("seq_len", "seq_len")):
            match = re.search(rf"fn default_{name}\(\) -> \w+ \{{\s*(\d+)\s*\}}", source)
            self.assertEqual(int(match.group(1)), bases.RECOMMENDED[key], name)


if __name__ == "__main__":
    unittest.main()
