"""Planted-mismatch tests for check_pins.py: it must refuse what it should and pass what it should."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("check_pins", Path(__file__).with_name("check_pins.py"))
check_pins = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(check_pins)

ROOT_TOML = """[workspace]
members = []
exclude = ["apps/*"]
[workspace.dependencies]
shakmaty = { version = "=0.30.2", features = ["variant"] }
serde = "1"
"""
APP_TOML = """[package]
name = "app"
version = "0.1.0"
[dependencies]
shakmaty = "=0.30.2"
serde = "1"
"""


def lock(version):
    return f'version = 4\n[[package]]\nname = "shakmaty"\nversion = "{version}"\n'


def tree(app_toml=APP_TOML, root_lock="0.30.2", app_lock="0.30.2"):
    temp = tempfile.TemporaryDirectory()
    root = Path(temp.name)
    (root / "apps/a").mkdir(parents=True)
    (root / "Cargo.toml").write_text(ROOT_TOML)
    (root / "Cargo.lock").write_text(lock(root_lock))
    (root / "apps/a/Cargo.toml").write_text(app_toml)
    (root / "apps/a/Cargo.lock").write_text(lock(app_lock))
    return temp, root


class CheckPins(unittest.TestCase):
    def test_agreeing_tree_passes(self):
        temp, root = tree()
        self.addCleanup(temp.cleanup)
        self.assertEqual(check_pins.check(root), [])

    def test_manifest_requirement_mismatch_is_refused(self):
        temp, root = tree(APP_TOML.replace('shakmaty = "=0.30.2"', 'shakmaty = "=0.30.1"'))
        self.addCleanup(temp.cleanup)
        problems = check_pins.check(root)
        self.assertTrue(any("shakmaty" in p and "apps/a/Cargo.toml" in p for p in problems), problems)

    def test_loosened_pin_is_refused(self):
        temp, root = tree(APP_TOML.replace('shakmaty = "=0.30.2"', 'shakmaty = "0.30"'))
        self.addCleanup(temp.cleanup)
        self.assertTrue(check_pins.check(root))

    def test_lockfile_drift_is_refused(self):
        temp, root = tree(app_lock="0.30.3")
        self.addCleanup(temp.cleanup)
        problems = check_pins.check(root)
        self.assertTrue(any("apps/a/Cargo.lock" in p and "0.30.3" in p for p in problems), problems)

    def test_unshared_dependency_may_differ(self):
        temp, root = tree(APP_TOML + 'other = "2"\n')
        self.addCleanup(temp.cleanup)
        self.assertEqual(check_pins.check(root), [])

    def test_real_repository_passes(self):
        self.assertEqual(check_pins.check(Path(__file__).resolve().parents[1]), [])


if __name__ == "__main__":
    unittest.main()
