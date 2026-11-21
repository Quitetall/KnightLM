"""The knightlm-uci game gate (scripts/knightlm_uci_game_gate.sh): a short Stockfish match through
the real serving app over a fake predictor. Skipped, with a reason, without the built binaries,
Stockfish, or fastapi; CI's `model-torch` job has all three and forbids the skip."""
import os
import shutil
import subprocess
import sys
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts" / "knightlm_uci_game_gate.sh"


def bin_dir():
    for name in (os.environ.get("KNIGHTLM_BIN_DIR"), REPO / "target" / "debug", REPO / "target" / "release"):
        if name and (Path(name) / "knightlm-uci").is_file() and (Path(name) / "knightlm-match").is_file():
            return Path(name)
    return None


def have_server_deps():
    try:
        import fastapi, httpx, uvicorn  # noqa: F401
        return True
    except ImportError:
        return False


STOCKFISH = os.environ.get("STOCKFISH_PATH") or shutil.which("stockfish")


@unittest.skipIf(bin_dir() is None, "knightlm-uci/knightlm-match binaries not built")
@unittest.skipIf(not STOCKFISH, "stockfish not installed")
@unittest.skipIf(not have_server_deps(), "fastapi/uvicorn/httpx not installed")
class UciGameGateTests(unittest.TestCase):
    def gate(self, **env):
        full = dict(os.environ, KNIGHTLM_BIN_DIR=str(bin_dir()), STOCKFISH_PATH=STOCKFISH, PYTHON=sys.executable, **env)
        return subprocess.run([str(SCRIPT), "2", "40"], env=full, capture_output=True, text=True, timeout=300)

    def test_short_match_has_zero_illegal_moves(self):
        done = self.gate()
        self.assertEqual(done.returncode, 0, done.stdout + done.stderr)
        self.assertIn("zero illegal moves", done.stdout)

    def test_gate_refuses_a_model_that_answers_illegal_moves(self):
        done = self.gate(GATE_FAKE_ARGS="--illegal")
        self.assertNotEqual(done.returncode, 0, done.stdout)


if __name__ == "__main__":
    unittest.main()
