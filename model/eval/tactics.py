"""Tactical set derived from the holdout by Stockfish MultiPV margin (t-e135).

No curated puzzle set exists in the repository, so the set is derived: a holdout
position is tactical when the best move beats the second-best by at least
`margin` centipawns at the derivation depth and the side to move is not already
lost. The set is a subset of the holdout, so it inherits the holdout's
independence from training (strictly later time window, disjoint Lichess IDs,
ECO group and scored positions). The "right" answer is the engine's best move,
not the human move.
"""

import argparse
import hashlib
import json
from pathlib import Path

from model.eval.corpus import load_holdout

FORMAT = "knightlm-tactical-set/v1"


def derive(rows, scorer, depth=16, margin=150, min_best=-300):
    ranked = scorer.rank([r["packet"]["fen"] for r in rows], lines=2, depth=depth)
    items = []
    for idx, row in enumerate(rows):
        rank = ranked[row["packet"]["fen"]]
        if rank["status"] != "ranked" or rank["margin_cp"] is None:
            continue
        best, second = rank["lines"][0], rank["lines"][1]
        if rank["margin_cp"] >= margin and best["clipped_cp"] >= min_best:
            items.append({"idx": idx, "game_id": row["game_id"], "fen": row["packet"]["fen"],
                          "best_move": rank["best_move"], "best_cp": best["clipped_cp"],
                          "second_cp": second["clipped_cp"], "margin_cp": rank["margin_cp"]})
    return {"format": FORMAT, "derivation": {
        "source": "holdout rows in manifest shard order", "rows_considered": len(rows), "depth": depth,
        "multipv": 2, "min_margin_cp": margin, "min_best_cp": min_best, "cp_clip": 1000,
        "engine": rank["engine"] if rows else None}, "rows_in_set": len(items), "items": items}


def write(path, tactic_set):
    Path(path).write_text(json.dumps(tactic_set, indent=1, sort_keys=True) + "\n")


def load_tactics(path, rows=None):
    """Load a set; with `rows`, check each item still points at the same holdout row."""
    raw = Path(path).read_bytes()
    data = json.loads(raw)
    if data["format"] != FORMAT:
        raise ValueError("unsupported tactical set format")
    limit = len(rows) if rows is not None else None
    items = [i for i in data["items"] if limit is None or i["idx"] < limit]
    if rows is not None:
        for item in items:
            row = rows[item["idx"]]
            if row["game_id"] != item["game_id"] or row["packet"]["fen"] != item["fen"]:
                raise ValueError(f"tactical set does not match the corpus at row {item['idx']}")
    return {"by_idx": {i["idx"]: i for i in items}, "path_sha256": hashlib.sha256(raw).hexdigest(),
            "rows_in_set": len(items), "derivation": data["derivation"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--corpus", type=Path, required=True)
    parser.add_argument("--rows", type=int, default=None)
    parser.add_argument("--depth", type=int, default=16)
    parser.add_argument("--margin", type=int, default=150)
    parser.add_argument("--workers", type=int, default=16)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    from model.eval.engine import StockfishScorer
    rows, _ = load_holdout(args.corpus, args.rows)
    scorer = StockfishScorer(depth=args.depth, workers=args.workers)
    tactic_set = derive(rows, scorer, args.depth, args.margin)
    write(args.out, tactic_set)
    print(f"{tactic_set['rows_in_set']} tactical positions of {len(rows)} -> {args.out}")


if __name__ == "__main__":
    main()
