"""model.run_pipeline on CPU: first input, unchanged rerun, added input, bad data."""

import contextlib
import io
import json
from pathlib import Path
import shutil
import tempfile
import unittest

from model.corpus.pipeline import MANIFEST, sha256_file
from model.corpus.test_pipeline import plant_leak, write_fixture
from model import run_pipeline as pipeline


def tree_digest(root):
    return {str(p.relative_to(root)): sha256_file(p) for p in sorted(Path(root).rglob("*")) if p.is_file()}


class PipelineTests(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.source = self.tmp / "source"
        write_fixture(self.source, shard_games=10)  # 25 train, 5 holdout
        self.work = self.tmp / "work"

    def go(self, *extra, source=None):
        calls = []
        def launcher(argv):
            calls.append(argv)
            return 0
        err = io.StringIO()
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(err):
            code = pipeline.main(["--source", str(source or self.source), "--work", str(self.work),
                                  "--train-games", "15", "--holdout-games", "3", "--skip-model", *extra],
                                 launcher=launcher)
        return code, calls, err.getvalue()

    def test_first_input_makes_a_verified_snapshot_and_plans_without_training(self):
        code, calls, _ = self.go()
        self.assertEqual(code, 0)
        (snapshot,) = (self.work / "snapshots").iterdir()
        self.assertEqual(snapshot.name, sha256_file(snapshot / MANIFEST)[:12])
        self.assertEqual(len(calls), 1)
        self.assertNotIn("--gpu", calls[0])
        self.assertEqual(calls[0][calls[0].index("--corpus") + 1], str(snapshot))

    def test_gpu_flag_trains_on_the_snapshot_it_just_verified(self):
        code, calls, _ = self.go("--gpu")
        (snapshot,) = (self.work / "snapshots").iterdir()
        self.assertEqual([("--gpu" in c) for c in calls], [False, True])
        self.assertTrue(all(c[c.index("--corpus") + 1] == str(snapshot) for c in calls))
        self.assertEqual(code, 4)  # the fake launcher wrote no manifests: the link check fails, loudly
        report = json.loads((self.work / "pipeline-report.json").read_text())
        self.assertFalse(report["same"])

    def test_unchanged_rerun_reuses_the_snapshot_byte_for_byte(self):
        self.go()
        before = tree_digest(self.work / "snapshots")
        self.go()
        self.assertEqual(tree_digest(self.work / "snapshots"), before)

    def test_added_input_makes_a_new_snapshot_and_leaves_the_old_one(self):
        self.go()
        before = tree_digest(self.work / "snapshots")
        calls = []
        with contextlib.redirect_stdout(io.StringIO()):
            pipeline.main(["--source", str(self.source), "--work", str(self.work), "--train-games", "25",
                           "--holdout-games", "5", "--skip-model"], launcher=lambda a: calls.append(a) or 0)
        after = tree_digest(self.work / "snapshots")
        self.assertEqual(len({k.split("/")[0] for k in after}), 2)
        self.assertTrue(set(before.items()) <= set(after.items()))

    def test_bad_input_stops_before_training_and_keeps_the_prior_snapshot(self):
        self.go()
        before = tree_digest(self.work / "snapshots")
        bad = self.tmp / "bad"
        write_fixture(bad, shard_games=10)
        plant_leak(bad, "game_id")
        code, calls, err = self.go("--gpu", source=bad)
        self.assertEqual(code, 2)
        self.assertEqual(calls, [])
        self.assertIn("REJECTED", err)
        self.assertEqual(tree_digest(self.work / "snapshots"), before)
        self.assertFalse(list((self.work / "snapshots").glob(".candidate-*")))

    def test_tampered_source_shard_is_rejected(self):
        shard = self.source / json.loads((self.source / MANIFEST).read_text())["shards"]["train"]["files"][1]["file"]
        shard.write_bytes(shard.read_bytes()[:-3] + b"xxx")
        code, calls, err = self.go("--gpu")
        self.assertEqual((code, calls), (2, []))
        self.assertIn("checksum mismatch", err)
        self.assertEqual(list((self.work / "snapshots").iterdir()), [])


if __name__ == "__main__":
    unittest.main()
