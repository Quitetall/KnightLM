"""Planted-violation tests for check_layers.py: it must refuse what it should and pass what it should."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("check_layers", Path(__file__).with_name("check_layers.py"))
check_layers = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(check_layers)

ROOT_TOML = """[workspace]
members = ["crates/*"]
exclude = ["apps/*"]
[workspace.dependencies]
knightlm-core = { path = "crates/core" }
knightlm-model = { path = "crates/model" }
serde = "1"
"""


def manifest(name, deps=""):
    return f'[package]\nname = "{name}"\nversion = "0.1.0"\n[dependencies]\nserde.workspace = true\n{deps}'


def tree(core="", notation='knightlm-core = { path = "../core" }\n', extra=None):
    temp = tempfile.TemporaryDirectory()
    root = Path(temp.name)
    (root / "Cargo.toml").write_text(ROOT_TOML)
    files = {"crates/core": manifest("knightlm-core", core), "crates/notation": manifest("knightlm-notation", notation),
             "crates/model": manifest("knightlm-model"), "crates/ai": manifest("rbc-ai"),
             "apps/rbc": manifest("rbc-app")}
    files.update(extra or {})
    for rel, text in files.items():
        (root / rel).mkdir(parents=True, exist_ok=True)
        (root / rel / "Cargo.toml").write_text(text)
    return temp, root


class CheckLayers(unittest.TestCase):
    def problems(self, **kw):
        temp, root = tree(**kw)
        self.addCleanup(temp.cleanup)
        return check_layers.check(root)

    def test_clean_layering_passes(self):
        self.assertEqual(self.problems(), [])

    def test_notation_may_depend_on_core_through_the_workspace_too(self):
        self.assertEqual(self.problems(notation="knightlm-core.workspace = true\n"), [])

    def test_core_depending_on_the_model_crate_is_refused(self):
        problems = self.problems(core='knightlm-model = { path = "../model" }\n')
        self.assertTrue(any("knightlm-model" in p and "above the rules layer" in p for p in problems), problems)

    def test_workspace_dependency_on_a_forbidden_crate_is_refused(self):
        problems = self.problems(notation="knightlm-model.workspace = true\n")
        self.assertTrue(any("knightlm-model" in p for p in problems), problems)

    def test_path_dependency_into_apps_is_refused(self):
        problems = self.problems(core='rbc-app = { path = "../../apps/rbc" }\n')
        self.assertTrue(any("rbc-app" in p for p in problems), problems)

    def test_unknown_path_outside_the_rules_layer_is_refused(self):
        problems = self.problems(notation='helper = { path = "../brick-chess" }\n')
        self.assertTrue(any("outside the rules layer" in p for p in problems), problems)

    def test_dev_dependency_and_renamed_package_are_checked(self):
        problems = self.problems(core='[dev-dependencies]\nx = { path = "../ai", package = "rbc-ai" }\n')
        self.assertTrue(any("rbc-ai" in p for p in problems), problems)

    def test_the_real_repository_passes(self):
        self.assertEqual(check_layers.check(Path(__file__).resolve().parents[1]), [])


if __name__ == "__main__":
    unittest.main()
