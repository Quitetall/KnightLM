"""Engine-free rules-QA pretraining of the board encoder, and its per-head eval axis.

Positions come from a corpus (the Run 1 packets); labels come from the compiler
(`labels.compile_labels`). Training sums the per-head losses, which is only an
optimisation objective: evaluation reports every head on its own and never a composite.

CPU smoke (tiny config, 200 positions): `python3 -m unittest model.encoder.test_encoder`.
Real run (prepared, not run): see docs/model/board-encoder.md.
"""

import argparse
import json
import random
import time
from pathlib import Path

import torch

from model.encoder import labels as L
from model.encoder.encoder import (BoardEncoder, EncoderConfig, RulesHeads, head_losses, save_encoder,
                                   to_targets)
from model.encoder.manifest import write_encoder_manifest
from model.encoder.planes import build_planes

AXIS = "board-encoder-rules-qa"
HEADS = ("piece_on_square", "attacked_squares", "legal_moves", "check", "material")


def make_batch(rows, device="cpu"):
    planes = torch.tensor([build_planes(r["fen"], r.get("repetition")) for r in rows], dtype=torch.float32)
    planes = planes.reshape(len(rows), -1, 8, 8).to(device)
    tgt = {k: v.to(device) for k, v in to_targets([L.targets(r) for r in rows]).items()}
    return planes, tgt


def train(rows, cfg, epochs=1, batch=32, lr=1e-3, seed=0, device="cpu", log=None):
    """Train encoder + heads on label rows. Returns (encoder, heads, history of per-head mean losses)."""
    torch.manual_seed(seed)
    rng = random.Random(seed)
    encoder, heads = BoardEncoder(cfg).to(device), RulesHeads(cfg.d_model).to(device)
    opt = torch.optim.AdamW([*encoder.parameters(), *heads.parameters()], lr=lr, weight_decay=0.0)
    history = []
    for epoch in range(epochs):
        order = list(range(len(rows)))
        rng.shuffle(order)
        sums, n = dict.fromkeys(("piece", "attacked", "legal", "check", "counts"), 0.0), 0
        for i in range(0, len(order), batch):
            planes, tgt = make_batch([rows[j] for j in order[i:i + batch]], device)
            losses = head_losses(heads(encoder(planes)), tgt)
            opt.zero_grad()
            sum(losses.values()).backward()  # optimisation objective only
            opt.step()
            for k, v in losses.items():
                sums[k] += v.item()
            n += 1
        history.append({k: v / n for k, v in sums.items()})
        if log:
            log(epoch, history[-1])
    return encoder, heads, history


@torch.no_grad()
def evaluate_heads(encoder, heads, rows, batch=64, device="cpu"):
    """Per-head accuracy on held-out label rows. One record per head; no composite is computed."""
    encoder.eval(), heads.eval()
    acc = {h: dict.fromkeys(("n", "hit", "exact", "tp", "fp", "fn"), 0) for h in HEADS}
    for i in range(0, len(rows), batch):
        chunk = rows[i:i + batch]
        planes, tgt = make_batch(chunk, device)
        out = heads(encoder(planes))
        b = len(chunk)
        hit = out["piece"].argmax(-1) == tgt["piece"]
        a = acc["piece_on_square"]
        a["n"] += hit.numel(); a["hit"] += int(hit.sum()); a["exact"] += int(hit.all(dim=1).sum())
        ok = (out["attacked"] > 0).float() == tgt["attacked"]
        a = acc["attacked_squares"]
        a["n"] += ok.numel(); a["hit"] += int(ok.sum()); a["exact"] += int(ok.reshape(b, -1).all(dim=1).sum())
        pred, true = out["legal"] > 0, tgt["legal"] > 0.5
        a = acc["legal_moves"]
        a["n"] += b; a["exact"] += int((pred == true).reshape(b, -1).all(dim=1).sum())
        a["tp"] += int((pred & true).sum()); a["fp"] += int((pred & ~true).sum()); a["fn"] += int((~pred & true).sum())
        ck = (out["check"] > 0).float() == tgt["check"]
        a = acc["check"]
        a["n"] += b; a["hit"] += int(ck.sum())
        a = acc["material"]
        a["n"] += b; a["exact"] += int((out["counts"].round() == tgt["counts"]).all(dim=1).sum())
    n_rows = len(rows)

    def rate(num, den):
        return round(num / den, 4) if den else None

    p, lg = acc["piece_on_square"], acc["legal_moves"]
    return {
        "axis": AXIS, "n_positions": n_rows,
        "heads": {
            "piece_on_square": {"square_accuracy": rate(p["hit"], p["n"]), "board_exact": rate(p["exact"], n_rows)},
            "attacked_squares": {"square_accuracy": rate(acc["attacked_squares"]["hit"], acc["attacked_squares"]["n"]),
                                 "map_exact": rate(acc["attacked_squares"]["exact"], n_rows)},
            "legal_moves": {"set_exact": rate(lg["exact"], n_rows),
                            "precision": rate(lg["tp"], lg["tp"] + lg["fp"]),
                            "recall": rate(lg["tp"], lg["tp"] + lg["fn"])},
            "check": {"accuracy": rate(acc["check"]["hit"], n_rows)},
            "material": {"counts_exact": rate(acc["material"]["exact"], n_rows)},
        },
        "note": "per head; no composite score",
    }


def corpus_fens(corpus, split, limit):
    """Unique FEN + repetition rows from a corpus split's packets (train-*.jsonl or holdout-*.jsonl)."""
    seen, rows = set(), []
    for shard in sorted(Path(corpus).glob(f"{split}-*.jsonl")):
        with open(shard, encoding="utf-8") as fh:
            for line in fh:
                packet = json.loads(line)["packet"]
                if packet["fen"] in seen:
                    continue
                seen.add(packet["fen"])
                rows.append({"fen": packet["fen"],
                             "repetition": packet["facts"]["repetition_count"] if packet.get("history_complete") else None})
                if len(rows) >= limit:
                    return rows
    return rows


def walked(rows, share, max_walk=40, seed=0):
    """Add `share` x len(rows) positions reached from corpus positions by 1..max_walk random legal moves.

    The corpus holds one decision per game at ply 24, so the encoder would otherwise only see middlegames
    at one depth. The moves are drawn by the compiler (`board-labels` walk); repetition is unknown there."""
    rng = random.Random(seed)
    extra = [{"fen": r["fen"], "walk": rng.randint(1, max_walk), "seed": rng.getrandbits(32), "repetition": None}
             for r in rng.sample(rows, int(len(rows) * share))]
    return rows + extra


def label_rows(rows, binary=None, chunk=20000):
    out = []
    for i in range(0, len(rows), chunk):
        part = rows[i:i + chunk]
        items = [{k: r[k] for k in ("fen", "walk", "seed") if k in r} for r in part]
        for src, lab in zip(part, L.compile_labels(items, binary)):
            out.append({**lab, "repetition": src["repetition"]})
    return out


def _sha_or_none(path):
    from model.corpus.pipeline import sha256_file
    return sha256_file(path) if path and Path(path).is_file() else None


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--corpus", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--train-rows", type=int, default=200_000)
    ap.add_argument("--holdout-rows", type=int, default=10_000)
    ap.add_argument("--epochs", type=int, default=20)
    ap.add_argument("--batch", type=int, default=256)
    ap.add_argument("--lr", type=float, default=1e-3)
    ap.add_argument("--walk-share", type=float, default=1.0, help="extra random-walk positions per corpus position (train only)")
    ap.add_argument("--n-global", type=int, default=0)
    ap.add_argument("--tiny", action="store_true", help="the CPU smoke config")
    ap.add_argument("--device", default="cpu")
    ap.add_argument("--knightlm", type=Path, default=None)
    args = ap.parse_args(argv)
    cfg = EncoderConfig.tiny() if args.tiny else EncoderConfig()
    cfg.n_global = args.n_global
    args.out.mkdir(parents=True, exist_ok=True)
    train_rows = label_rows(walked(corpus_fens(args.corpus, "train", args.train_rows), args.walk_share), args.knightlm)
    hold_rows = label_rows(corpus_fens(args.corpus, "holdout", args.holdout_rows), args.knightlm)
    started = time.time()
    encoder, heads, history = train(train_rows, cfg, args.epochs, args.batch, args.lr, device=args.device,
                                    log=lambda e, h: print(json.dumps({"epoch": e, **h}), flush=True))
    axis = evaluate_heads(encoder, heads, hold_rows, device=args.device)
    axis["train_seconds"] = round(time.time() - started, 1)
    save_encoder(args.out / "encoder", encoder, heads)
    (args.out / "axis.json").write_text(json.dumps(axis, indent=2, sort_keys=True))
    write_encoder_manifest(args.out / "encoder", corpus_manifest_sha256=_sha_or_none(args.corpus / "manifest.json"),
                           compiler_sha256=_sha_or_none(L.find_binary(args.knightlm)),
                           axis_sha256=_sha_or_none(args.out / "axis.json"),
                           train={"epochs": args.epochs, "batch": args.batch, "lr": args.lr, "train_rows": args.train_rows,
                                  "walk_share": args.walk_share, "device": args.device, "tiny": args.tiny})
    print(json.dumps(axis, sort_keys=True))


if __name__ == "__main__":
    main()
