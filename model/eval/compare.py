"""python -m model.eval.compare [--changed-only] [--allow-prompt-mismatch] A B: per-axis deltas side by side. No composite.

A and B are scorecard directories (or scorecard.json files). Deltas are B - A.
Whether higher is better differs per axis, so the direction column says which
way is improvement; nothing is summed or weighted across axes.

A scorecard has a prompt kind: "move" (the move-only prompt), "play" (the play-mode prompt
and tool-call completion with the FEN packet, arm T0) or "play-cgt" / "play-cgt-full" (the same
with the position as Chess Graph Text, arm T2). Cards of different kinds measure different
tasks: compare prints NOT COMPARABLE and exits 3 without a table, unless
--allow-prompt-mismatch is given (the notes still print, marked ALLOWED). A card with no
prompt_kind is a move card. One exception, by design: two S0 rules-QA cards whose corpora carry the same
`matched_sha256` (same ids, questions and answers, different prompt rendering) are the arms of
one experiment and compare without the flag; the note says so. An unknown kind is refused.
"""

import json
import sys
from pathlib import Path

from model.eval.render import fmt
from model.prompt import PLAY_KINDS

KNOWN_KINDS = ("move", *PLAY_KINDS)

# (axis path suffix) -> "up" or "down" is better. Unlisted numeric leaves get no arrow.
LOWER_IS_BETTER = ("centipawn_loss", "latency_ms", "token_cost", "failures", "rows_unscored")
LOWER_IS_BETTER_LEAVES = ("failed", "failed_total", "explanations_missing", "explanations_with_a_failed_claim",
                          "claim_failure_rate", "parse_failures")
SKIP = ("gate", "set", "engine", "protocol", "tokenizer_recount", "rating", "status", "source")


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
    if leaf in LOWER_IS_BETTER_LEAVES or (leaf in ("mean", "median", "p90") and path.startswith("lc0_strength.")):
        return "down"
    if any(part in path.split(".") for part in LOWER_IS_BETTER) or leaf == "losses":
        return "down"
    if leaf in ("rate", "accuracy", "score", "matches", "correct", "legal", "wins", "valid"):
        return "up"
    return ""


PROMPT_MISMATCH = "NOT COMPARABLE: prompt kind differs"


ARMS_NOTE = "ARMS of one matched S0 corpus"


def prompt_kind(card):
    kind = card.get("prompt_kind", "move")
    if kind not in KNOWN_KINDS:
        raise ValueError(f"unknown prompt kind {kind!r}; this compare knows {KNOWN_KINDS}")
    return kind


def matched_key(card):
    """The S0 matched-corpus key of a card, or None for any other card."""
    return (card.get("inputs", {}).get("corpus", {}) or {}).get("matched_sha256")


def comparability(a, b):
    notes = []
    ia, ib = a.get("inputs", {}), b.get("inputs", {})
    matched = matched_key(a) is not None and matched_key(a) == matched_key(b)
    if prompt_kind(a) != prompt_kind(b):
        if matched:
            notes.append(f"{ARMS_NOTE} ({prompt_kind(a)} vs {prompt_kind(b)}): same ids, questions and answers; only the prompt rendering differs")
        else:
            notes.append(f"{PROMPT_MISMATCH} ({prompt_kind(a)} vs {prompt_kind(b)}): the two scorecards measure different tasks")
    checks = {
        "play prompt": lambda i: i.get("settings", {}).get("play_prompt_sha256"),
        "play max new tokens": lambda i: i.get("settings", {}).get("play_max_new_tokens"),
        "corpus manifest": lambda i: i.get("corpus", {}).get("matched_sha256") or i.get("corpus", {}).get("corpus_manifest_sha256"),
        "rows": lambda i: i.get("corpus", {}).get("rows_used"),
        "stockfish depth": lambda i: (i.get("stockfish") or {}).get("depth"),
        "stockfish binary": lambda i: (i.get("stockfish") or {}).get("stockfish_sha256"),
        "tactical set": lambda i: (i.get("tactical_set") or {}).get("sha256"),
        "modes": lambda i: i.get("settings", {}).get("modes"),
        "lc0 nodes": lambda i: i.get("settings", {}).get("lc0_nodes"),
        "lc0 weights": lambda i: ((i.get("oracles") or {}).get("lc0") or {}).get("weights_sha256"),
        "maia rating": lambda i: i.get("settings", {}).get("maia_rating"),
        "maia weights": lambda i: ((i.get("oracles") or {}).get("maia") or {}).get("weights_sha256"),
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
    allow_prompt = "--allow-prompt-mismatch" in argv
    argv = [a for a in argv if a not in ("--changed-only", "--allow-prompt-mismatch")]
    if len(argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    a, b = load(argv[0]), load(argv[1])
    print(f"A = {argv[0]}\nB = {argv[1]}\nDeltas are B - A. No composite score is computed.")
    refused = False
    for note in comparability(a, b):
        if note.startswith(PROMPT_MISMATCH):
            refused = not allow_prompt
            note = note if refused else "ALLOWED (--allow-prompt-mismatch) " + note
        print(note)
    if refused:
        print("No table: pass --allow-prompt-mismatch to print one anyway.", file=sys.stderr)
        return 3
    table = [r for r in compare(a, b) if not changed_only or r[3] != 0]
    width = max((len(r[0]) for r in table), default=10)
    print(f"{'axis':<{width}}  {'A':>10}  {'B':>10}  {'delta':>10}  better")
    for path, x, y, delta, better in table:
        print(f"{path:<{width}}  {fmt(x):>10}  {fmt(y):>10}  {fmt(delta):>10}  {better}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
