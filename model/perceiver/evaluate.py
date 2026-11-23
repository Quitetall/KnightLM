"""Per-style scorecard: per-square accuracy and whole-board exact match, split by train/held-out style.

    python3 -m model.perceiver.evaluate --checkpoint DIR/perceiver.pt [--boards-per-style 200]
"""
import argparse
import json
from pathlib import Path

import numpy as np

from . import dataset
from .model import predict
from .styles import ALL_STYLES
from .fen import NUM_CLASSES


def scorecard(model, placements, styles, seed: int, boards_per_style: int) -> dict:
    cells, labels, sidx, _ = dataset.build(placements, styles, seed, boards_per_style)
    pred = predict(model, cells)
    ok = pred == labels
    rows = {}
    for si, style in enumerate(styles):
        m = sidx == si
        board_ok = ok[m].reshape(-1, 64).all(axis=1)
        occupied = labels[m] != 0
        rows[style.name] = {
            "split": style.split,
            "boards": int(board_ok.size),
            "square_accuracy": float(ok[m].mean()),
            "occupied_square_accuracy": float(ok[m][occupied].mean()) if occupied.any() else None,
            "board_exact_match": float(board_ok.mean()),
        }
    out = {"styles": rows}
    for split in ("train", "heldout"):
        m = np.isin(sidx, [i for i, s in enumerate(styles) if s.split == split])
        if m.any():
            out[split] = {"square_accuracy": float(ok[m].mean()),
                          "board_exact_match": float(ok[m].reshape(-1, 64).all(axis=1).mean())}
    confusion = np.zeros((NUM_CLASSES, NUM_CLASSES), dtype=np.int64)
    np.add.at(confusion, (labels, pred), 1)
    out["confusion"] = confusion.tolist()
    return out


def main(argv=None):
    from .train import load

    ap = argparse.ArgumentParser()
    ap.add_argument("--checkpoint", type=Path, required=True)
    ap.add_argument("--boards-per-style", type=int, default=200)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--out", type=Path)
    a = ap.parse_args(argv)
    model, meta = load(a.checkpoint)
    styles = [s for s in ALL_STYLES if dataset.available(s)]
    card = scorecard(model, meta["eval_positions"], styles, a.seed, a.boards_per_style)
    text = json.dumps({k: v for k, v in card.items() if k != "confusion"}, indent=2)
    print(text)
    if a.out:
        a.out.write_text(json.dumps(card))


if __name__ == "__main__":
    main()
