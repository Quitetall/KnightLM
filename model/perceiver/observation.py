"""Predictions -> BoardObservation JSON, and the ledger check via the Rust CLI.

The compiler is the only judge of agreement: this module never compares
placements itself for the verdict and never repairs a disagreeing prediction.
"""
import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

from .fen import labels_to_placement

SCHEMA_VERSION = 1  # keep equal to crates/core SCHEMA_VERSION (checked in tests)
REPO = Path(__file__).resolve().parents[2]


def to_observation(labels, image_id: str) -> dict:
    """Placement only. No side to move, castling, clocks or history: pixels do not carry them."""
    if not image_id:
        raise ValueError("image_id required")
    return {"schema_version": SCHEMA_VERSION, "image_id": image_id, "placement": labels_to_placement(labels)}


def cli_binary() -> str | None:
    explicit = os.environ.get("KNIGHTLM_BIN")
    if explicit and Path(explicit).exists():
        return explicit
    for rel in ("target/debug/knightlm", "target/release/knightlm"):
        if (REPO / rel).exists():
            return str(REPO / rel)
    return shutil.which("knightlm")


def game_record(fen: str, game_id: str = "perceiver") -> dict:
    return {"schema_version": SCHEMA_VERSION, "game_id": game_id, "source": "perceiver", "initial_fen": fen,
            "history_complete": False, "moves": []}


def verify(fen: str, observation: dict, binary: str | None = None) -> dict:
    """Run `knightlm verify-observation`. Returns {"ok": bool, "detail": str}; a mismatch is ok=False."""
    binary = binary or cli_binary()
    if not binary:
        raise FileNotFoundError("knightlm binary not found; set KNIGHTLM_BIN")
    with tempfile.TemporaryDirectory(prefix="perceiver-") as tmp:
        g, o = Path(tmp, "game.json"), Path(tmp, "obs.json")
        g.write_text(json.dumps(game_record(fen)))
        o.write_text(json.dumps(observation))
        p = subprocess.run([binary, "verify-observation", str(g), str(o)], capture_output=True, text=True, timeout=60)
    detail = (p.stdout if p.returncode == 0 else p.stderr).strip()
    return {"ok": p.returncode == 0, "detail": detail}
