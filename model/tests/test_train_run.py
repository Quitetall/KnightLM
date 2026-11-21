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
from unittest import mock

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

    def test_ctok1_refuses_a_stock_prompt_corpus_and_is_never_launched(self):
        code, _, err = call([*self.base, "--run", "r1", "--tokenizer", "ctok1"])
        self.assertEqual(code, 2)
        self.assertIn("need --tokenizer ctok1", err)

    def test_tokenizer_kind_guard(self):
        rows = {"reasoning": [{"reasoning": "x", "game_id": "g"}], "s0": [{"qa": {}, "prompt_kind": "play-t3", "game_id": "g"}]}
        run.check_tokenizer_kinds(rows["reasoning"], "play-t1", "ctok1")
        run.check_tokenizer_kinds(rows["reasoning"], "play-cgt", "stock")
        run.check_tokenizer_kinds(rows["s0"], "play", "ctok1")
        for rows_, kind, tokenizer in ((rows["reasoning"], "play-t1", "stock"), (rows["reasoning"], "play", "ctok1"),
                                       (rows["s0"], "play", "stock")):
            with self.assertRaises(run.Refused):
                run.check_tokenizer_kinds(rows_, kind, tokenizer)

    def test_ctok1_without_an_extended_base_plans_the_recipe_and_refuses_gpu(self):
        with mock.patch.object(run, "CTOK_BASE_DIR", self.tmp / "no-such-base"), \
                mock.patch.object(run, "check_tokenizer_kinds"), mock.patch.object(run, "check_prompt_parity", return_value={"status": "ok"}):
            code, out, _ = call([*self.base, "--run", "r1", "--tokenizer", "ctok1", "--json"])
            self.assertEqual(code, 0, out)
            plan = json.loads(out)
            args = json.loads(plan["command"][-1])
            self.assertEqual((args["base"], args["ctok_rows"], args["base_dir"]),
                             ("smolvlm-500m-ctok1", "embed+head", str(self.tmp / "no-such-base")))
            self.assertIn("ctok build-base", plan["launch_blocked"])
            self.assertFalse(plan["tokenizer"]["extended_base_built"])
            code, _, err = call([*self.base, "--run", "r1", "--tokenizer", "ctok1", "--gpu", "--blut-bin", str(self.stub("exit 0\n"))])
            self.assertEqual(code, 2)
            self.assertIn("no extended base", err)
            self.assertFalse((self.runs / "r1").exists())

    def test_ctok1_with_an_extended_base_is_accepted_for_gpu(self):
        ext = self.tmp / "ext-base"
        ext.mkdir()
        (ext / "ctok-extension.json").write_text("{}")
        with mock.patch.object(run, "check_tokenizer_kinds"), mock.patch.object(run, "check_prompt_parity", return_value={"status": "ok"}), \
                mock.patch.object(run, "resolve_venv", return_value=(self.tmp, "test")):
            code, out, _ = call([*self.base, "--run", "r1", "--tokenizer", "ctok1", "--base-dir", str(ext), "--ctok-rows", "embed",
                                 "--gpu", "--blut-bin", str(self.stub("mkdir -p x\nexit 0\n"))])
            self.assertIn("Launching (--gpu).", out)
            entry = json.loads((self.runs / "r1" / "entry.json").read_text())
            args = json.loads(entry["command"][-1])
            self.assertEqual((args["base"], args["ctok_rows"], args["base_dir"]), ("smolvlm-500m-ctok1", "embed", str(ext)))

    def test_extended_base_check_refuses_other_weights_a_tampered_file_and_a_stock_file(self):
        from model.tokenizer import vocab
        ext = self.tmp / "ext-base2"
        ext.mkdir()
        weights = ext / "model.safetensors"
        weights.write_bytes(b"extended")
        profile = {**bases.BASES["smolvlm-500m"], "weights_sha256": "a" * 64}
        record = {"format": run.EXTENSION_FORMAT, "spec_version": vocab.SPEC_VERSION, "vocab_sha256": vocab.vocab_sha256(),
                  "vocab_size": vocab.SIZE, "base": 49280, "seed": 0, "head_untied": True, "stock_weights_sha256": "a" * 64,
                  "extended_weights_sha256": run.sha256_file(weights)}
        (ext / "ctok-extension.json").write_text(json.dumps(record))
        self.assertEqual(run.check_extended_base(profile, ext)[1]["status"], "ok")
        for edit in ({"stock_weights_sha256": "b" * 64}, {"head_untied": False}, {"extended_weights_sha256": "0" * 64},
                     {"vocab_sha256": "0" * 64}, {"spec_version": "ctok/2"}):
            (ext / "ctok-extension.json").write_text(json.dumps({**record, **edit}))
            with self.assertRaises(run.Refused, msg=str(edit)):
                run.check_extended_base(profile, ext)
        (ext / "ctok-extension.json").write_text(json.dumps(record))
        weights.write_bytes(b"tampered")
        with self.assertRaises(run.Refused):
            run.check_extended_base(profile, ext)

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


class LossSegmentTests(unittest.TestCase):
    """The recipe puts loss on every completion token, so rows with tool-result spans are refused there."""

    ROW = {"id": "r2b-train-000000", "segments": [{"text": "call", "loss": True}, {"text": "result", "loss": False},
                                                   {"text": "final", "loss": True}]}

    def test_a_row_with_a_result_span_is_refused_unless_the_flag_accepts_it(self):
        with self.assertRaisesRegex(run.Refused, "r2b-train-000000.*without loss.*--allow-unmasked-results"):
            run.check_loss_segments([{"id": "plain"}, self.ROW])
        self.assertEqual(run.check_loss_segments([{"id": "plain"}, self.ROW], allow=True)["status"], "warning")

    def test_rows_without_segments_or_with_only_loss_spans_pass(self):
        self.assertIsNone(run.check_loss_segments([{"id": "plain"}, {"id": "a", "segments": [{"text": "x", "loss": True}]}]))


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
