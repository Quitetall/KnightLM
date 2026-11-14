"""python -m model.eval.compare [--changed-only] A B: per-axis deltas side by side. No composite.

A and B are scorecard directories (or scorecard.json files). Deltas are B - A.
Whether higher is better differs per axis, so the direction column says which
way is improvement; nothing is summed or weighted across axes.
"""

import json
import sys
from pathlib import Path

from model.eval.render import fmt

# (axis path suffix) -> "up" or "down" is better. Unlisted numeric leaves get no arrow.
LOWER_IS_BETTER = ("centipawn_loss", "latency_ms", "token_cost", "failures", "rows_unscored")
SKIP = ("gate", "set", "engine", "protocol", "tokenizer_recount")


def load(path):
    path = Path(path)
    return json.loads((path / "scorecard.json" if path.is_dir() else path).read_text())


def flatten(node, prefix=()):
    if isinstance(node, dict):
        for key, value in node.items():
            if key in SKIP:
                continue
            yield from flatten(value, prefix + (key,))
    elif isinstance(node, bool):
        return
    elif isinstance(node, (int, float)):
        yield ".".join(prefix), node


def direction(path):
    leaf = path.rsplit(".", 1)[-1]
    if leaf in ("n", "rows"):
        return ""
    if leaf.startswith("engine_best") or leaf == "rows_scored":
        return "up"
    if leaf.startswith("engine_best"):
        return "up"
    if any(part in path.split(".") for part in LOWER_IS_BETTER) or leaf == "losses":
        return "down"
    if leaf in ("rate", "accuracy", "score", "matches", "correct", "legal", "wins"):
        return "up"
    return ""


def comparability(a, b):
    notes = []
    ia, ib = a.get("inputs", {}), b.get("inputs", {})
    checks = {
        "corpus manifest": lambda i: i.get("corpus", {}).get("corpus_manifest_sha256"),
        "rows": lambda i: i.get("corpus", {}).get("rows_used"),
        "stockfish depth": lambda i: (i.get("stockfish") or {}).get("depth"),
        "stockfish binary": lambda i: (i.get("stockfish") or {}).get("stockfish_sha256"),
        "tactical set": lambda i: (i.get("tactical_set") or {}).get("sha256"),
        "modes": lambda i: i.get("settings", {}).get("modes"),
    }
    for name, get in checks.items():
        if get(ia) != get(ib):
            notes.append(f"NOT COMPARABLE: {name} differs ({get(ia)} vs {get(ib)})")
    return notes


def compare(a, b):
    left, right = dict(flatten(a["axes"])), dict(flatten(b["axes"]))
    rows = []
    for path in sorted(set(left) | set(right)):
        x, y = left.get(path), right.get(path)
        delta = None if x is None or y is None else round(y - x, 4)
        rows.append((path, x, y, delta, direction(path)))
    return rows


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    changed_only = "--changed-only" in argv
    argv = [a for a in argv if a != "--changed-only"]
    if len(argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    a, b = load(argv[0]), load(argv[1])
    print(f"A = {argv[0]}\nB = {argv[1]}\nDeltas are B - A. No composite score is computed.")
    for note in comparability(a, b):
        print(note)
    table = [r for r in compare(a, b) if not changed_only or r[3] != 0]
    width = max((len(r[0]) for r in table), default=10)
    print(f"{'axis':<{width}}  {'A':>10}  {'B':>10}  {'delta':>10}  better")
    for path, x, y, delta, better in table:
        print(f"{path:<{width}}  {fmt(x):>10}  {fmt(y):>10}  {fmt(delta):>10}  {better}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
