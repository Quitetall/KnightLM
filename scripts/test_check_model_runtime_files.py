#!/usr/bin/env python3
"""Planted violations for scripts/check_model_runtime_files.py."""
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import check_model_runtime_files as check  # noqa: E402

REPO = Path(__file__).resolve().parents[1]
TAURI = "apps/rusty-brick-chess/src-tauri/tauri.conf.json"


class Planted(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="rbc-runtime-files-"))
        self.addCleanup(shutil.rmtree, self.tmp, True)
        listed = (REPO / "packaging/model-runtime.files").read_text().splitlines()
        for rel in [line for line in listed if line and not line.startswith("#")] + [
            "packaging/model-runtime.files", TAURI,
        ]:
            target = self.tmp / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(REPO / rel, target)

    def test_the_real_tree_is_clean(self):
        self.assertEqual(check.check(REPO), [])
        self.assertEqual(check.check(self.tmp), [])

    def test_a_missing_import_is_caught(self):
        listing = self.tmp / "packaging/model-runtime.files"
        listing.write_text("".join(
            line + "\n" for line in listing.read_text().splitlines() if not line.endswith("model/tokenizer/vocab.py")))
        errors = check.check(self.tmp)
        self.assertTrue(any("needed by the model server" in e and "vocab.py" in e for e in errors), errors)

    def test_a_missing_file_is_caught(self):
        (self.tmp / "model/prompt.py").unlink()
        errors = check.check(self.tmp)
        self.assertTrue(any("listed but missing" in e for e in errors), errors)

    def test_a_bundle_that_drifted_is_caught(self):
        conf_path = self.tmp / TAURI
        conf = json.loads(conf_path.read_text())
        conf["bundle"]["resources"].pop("../../../model/serve/play.py")
        conf_path.write_text(json.dumps(conf))
        errors = check.check(self.tmp)
        self.assertTrue(any("not in the Tauri bundle" in e and "play.py" in e for e in errors), errors)


if __name__ == "__main__":
    unittest.main()
