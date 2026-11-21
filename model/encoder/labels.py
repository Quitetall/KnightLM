"""Rules-QA labels for encoder pretraining, from the compiler only.

Labels come from `knightlm board-labels` (crates/cli/src/board_labels.rs: shakmaty on the
notation crate's validated position). Nothing here computes a chess fact and no engine
is involved. Square index is rank * 8 + file (a1 = 0), as in `planes.py`.
"""

import json
import os
import subprocess
from pathlib import Path

from model.encoder.planes import PIECES, square_index

ROOT = Path(__file__).resolve().parents[2]
FIXTURE = Path(__file__).parent / "fixtures" / "labels_smoke.jsonl"


def find_binary(explicit=None):
    for candidate in (explicit, os.environ.get("KNIGHTLM_BIN"), ROOT / "target/release/knightlm",
                      ROOT / "target/debug/knightlm"):
        if candidate and Path(candidate).is_file():
            return Path(candidate)
    return None


def compile_labels(fens, binary=None, timeout_s=600):
    """Run the compiler's `board-labels`; one label row per item, refusing any `ok: false`.

    An item is a FEN, or `{"fen", "walk": n, "seed": s}` to label the position reached by n
    pseudo-random legal moves (chosen by the compiler's own generator); that row's `fen` is the
    position reached and `source_fen` where it started."""
    exe = find_binary(binary)
    if exe is None:
        raise FileNotFoundError("knightlm binary not found (cargo build --release -p knightlm-cli, or set KNIGHTLM_BIN)")
    stdin = "".join(json.dumps(f if isinstance(f, dict) else {"fen": f}) + "\n" for f in fens)
    run = subprocess.run([str(exe), "board-labels"], input=stdin, capture_output=True, text=True,
                         timeout=timeout_s, check=True)
    rows = [json.loads(line) for line in run.stdout.splitlines() if line.strip()]
    bad = [r for r in rows if not r.get("ok")]
    if bad or len(rows) != len(fens):
        raise ValueError(f"compiler refused {len(bad)} of {len(fens)} positions: {bad[:1]}")
    return rows


def read_labels(path):
    with open(path, encoding="utf-8") as fh:
        return [json.loads(line) for line in fh if line.strip()]


def write_labels(path, rows):
    with open(path, "w", encoding="utf-8") as fh:
        for row in rows:
            fh.write(json.dumps(row, sort_keys=True, separators=(",", ":")) + "\n")


def legal_pairs(row):
    """(from, to) square indexes of the legal moves; promotion pieces collapse onto one pair."""
    pairs = set()
    for uci in row["legal_moves"]:
        pairs.add((square_index(uci[:2]), square_index(uci[2:4])))
    return pairs


def targets(row):
    """Plain-Python targets for one label row (the model module turns them into tensors)."""
    piece = [PIECES.index(c) + 1 if c != "." else 0 for c in row["squares"]]
    attacked = [[int(c) for c in row["attacked_white"]], [int(c) for c in row["attacked_black"]]]
    legal = [[0] * 64 for _ in range(64)]
    for a, b in legal_pairs(row):
        legal[a][b] = 1
    counts = row["counts"]["white"] + row["counts"]["black"]
    return {"piece": piece, "attacked": attacked, "legal": legal, "check": int(row["in_check"]), "counts": counts}
