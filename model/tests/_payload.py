"""Put the blut-knightlm Python payload (apps/blut-knightlm/python) on sys.path."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
PAYLOAD = ROOT / "apps" / "blut-knightlm" / "python"
if str(PAYLOAD) not in sys.path:
    sys.path.insert(0, str(PAYLOAD))
