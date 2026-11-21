"""Prepared training command for the human-only move head. CPU preflight only: it launches nothing.

    python3 -m model.policy_head.train --corpus CORPUS [--rows N] [--epochs E]

Prints the plan (usable rows, steps, estimate). The GPU launch (host forward with the LoRA
adapter, head on the last prompt token) is owed work and needs the owner's go; see
docs/model/move-head.md. Training rows are the corpus `target` (the human move) with the packet's
legal_moves as the mask. Rows whose target is not in the legal list, or has no legal move, are refused.
"""

import argparse
import json
from pathlib import Path
import sys

from model.policy_head import space

DEFAULT_CORPUS = Path("/mnt/4tb/artifacts/KnightLM/expire/corpus/run1-2026-09-a")
BATCH_ROWS = 32
# Unmeasured: Run 1 cost ~75 ms/row (625 steps x 32 rows in 25 min) including ~100+ generated tokens;
# prompt-only forward/backward is assumed 3-5x cheaper. Re-measure on the first 200 steps.
MS_PER_ROW = (15, 25)


def preflight(corpus, rows, epochs):
    shards = sorted(Path(corpus).glob("train-*.jsonl"))
    if not shards:
        raise SystemExit(f"no train shards in {corpus}")
    seen = refused = 0
    for shard in shards:
        with shard.open() as stream:
            for line in stream:
                if seen + refused >= rows:
                    break
                row = json.loads(line)
                try:
                    space.target_index(row["target"], row["packet"]["legal_moves"])
                    space.indices(row["packet"]["legal_moves"])
                    seen += 1
                except space.MoveSpaceError:
                    refused += 1
    total = seen * epochs
    return {"arm": "human-only move head", "corpus": str(corpus), "rows_usable": seen, "rows_refused": refused,
            "epochs": epochs, "steps": total // BATCH_ROWS,
            "estimate_minutes": [round(total * ms / 60000, 1) for ms in MS_PER_ROW],
            "estimate_note": "unmeasured; see docs/model/move-head.md", "launch": "owed (needs the owner's go)"}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--corpus", type=Path, default=DEFAULT_CORPUS)
    parser.add_argument("--rows", type=int, default=20000)
    parser.add_argument("--epochs", type=int, default=1)
    parser.add_argument("--gpu", action="store_true", help="refused: launch wiring is not built")
    args = parser.parse_args(argv)
    if args.gpu:
        print("refused: the GPU launch is not built and training needs the owner's go", file=sys.stderr)
        return 3
    print(json.dumps(preflight(args.corpus, args.rows, args.epochs), indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
