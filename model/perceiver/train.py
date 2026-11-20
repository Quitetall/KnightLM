"""CPU/GPU training of the square classifier. Never launched by tests beyond a tiny smoke fit.

    python3 -m model.perceiver.train --out DIR [--positions N --boards-per-style M --epochs E --device cuda]
"""
import argparse
import json
import time
from pathlib import Path

import numpy as np
import torch
from torch import nn

from . import dataset
from .model import SquareCNN, to_tensor, predict
from .styles import TRAIN_STYLES, HELDOUT_STYLES


def fit(cells, labels, epochs: int, seed: int = 0, lr: float = 3e-3, batch: int = 256, device: str = "cpu",
        log=None):
    torch.manual_seed(seed)
    model = SquareCNN().to(device)
    opt = torch.optim.AdamW(model.parameters(), lr=lr, weight_decay=1e-4)
    n = len(cells)
    steps = epochs * ((n + batch - 1) // batch)
    sched = torch.optim.lr_scheduler.OneCycleLR(opt, max_lr=lr, total_steps=steps)
    gen = np.random.default_rng(seed)
    for ep in range(epochs):
        model.train()
        order = gen.permutation(n)
        total = 0.0
        for i in range(0, n, batch):
            idx = order[i : i + batch]
            x = to_tensor(cells[idx]).to(device)
            y = torch.from_numpy(labels[idx]).to(device)
            loss = nn.functional.cross_entropy(model(x), y)
            opt.zero_grad()
            loss.backward()
            opt.step()
            sched.step()
            total += loss.item() * len(idx)
        if log:
            log(f"epoch {ep + 1}/{epochs} loss {total / n:.4f}")
    return model.cpu()


def save(model, path: Path, meta: dict):
    path.parent.mkdir(parents=True, exist_ok=True)
    torch.save({"state": model.state_dict(), "meta": meta}, path)


def load(path: Path):
    blob = torch.load(path, map_location="cpu", weights_only=False)
    model = SquareCNN()
    model.load_state_dict(blob["state"])
    return model.eval(), blob["meta"]


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--corpus", type=Path, default=dataset.CORPUS_DEFAULT)
    ap.add_argument("--positions", type=int, default=4000)
    ap.add_argument("--eval-positions", type=int, default=300)
    ap.add_argument("--boards-per-style", type=int, default=2000)
    ap.add_argument("--epochs", type=int, default=12)
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--device", default="cpu")
    a = ap.parse_args(argv)
    train_shards, hold_shards = dataset.corpus_split(a.corpus)
    train_pos = dataset.load_placements(train_shards, a.positions, a.seed)
    eval_pos = [p for p in dataset.load_placements(hold_shards, a.eval_positions * 3, a.seed) if p not in set(train_pos)][
        : a.eval_positions]
    styles = [s for s in TRAIN_STYLES if dataset.available(s)]
    t0 = time.time()
    cells, labels, _, _ = dataset.build(train_pos, styles, a.seed, a.boards_per_style)
    print(f"rendered {len(cells) // 64} boards in {time.time() - t0:.0f}s", flush=True)
    model = fit(cells, labels, a.epochs, a.seed, device=a.device, log=lambda m: print(m, flush=True))
    meta = {"seed": a.seed, "train_styles": [s.name for s in styles], "dataset": dataset.manifest(styles, a.seed),
            "train_positions": len(train_pos), "eval_positions": eval_pos}
    save(model, a.out / "perceiver.pt", meta)
    (a.out / "eval_positions.json").write_text(json.dumps(eval_pos))
    print(f"saved {a.out / 'perceiver.pt'}")


if __name__ == "__main__":
    main()
