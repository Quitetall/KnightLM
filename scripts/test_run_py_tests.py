import io
import contextlib
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import run_py_tests  # noqa: E402

SRC = textwrap.dedent('''
    import unittest
    class T(unittest.TestCase):
        def test_ok(self): pass
        @unittest.skip("torch not installed")
        def test_skipped(self): pass
''')


class RunnerTests(unittest.TestCase):
    def run_runner(self, *extra):
        with tempfile.TemporaryDirectory() as d:
            Path(d, "sample_skip_mod.py").write_text(SRC)
            sys.path.insert(0, d)
            self.addCleanup(sys.path.remove, d)
            self.addCleanup(sys.modules.pop, "sample_skip_mod", None)
            out = io.StringIO()
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(io.StringIO()):
                code = run_py_tests.main([*extra, "sample_skip_mod"])
            return code, out.getvalue()

    def test_skip_is_listed_with_module_and_reason(self):
        code, out = self.run_runner()
        self.assertEqual(code, 0)
        self.assertIn("sample_skip_mod: 1 skipped, reason: torch not installed", out)

    def test_forbidden_skip_fails(self):
        code, out = self.run_runner("--forbid-skip", "torch")
        self.assertEqual(code, 1)
        self.assertIn("FAIL", out)

    def test_unmatched_forbid_pattern_passes(self):
        self.assertEqual(self.run_runner("--forbid-skip", "peft")[0], 0)


if __name__ == "__main__":
    unittest.main()
