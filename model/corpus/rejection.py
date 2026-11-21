"""Rejection sampling for grounded reasoning: keep only samples the rules oracle accepts.

A predictor is any callable `predictor(row, n) -> list[str]` that returns up to `n` completions for the
row's prompt (`row["prompt"]`; the row also carries the packet). A completion is variant a (one
`submit_move` call) or variant b (read calls, each with its harness-rendered result, then one
`submit_move`), see model/corpus/reasoning2.py. A sample is kept only if every rule-based reward passes:

  format     the completion parses (play completion; for b also play-schema read calls with schema-valid
             arguments, results in place, at most eight reads)
  results    (b) every result in the transcript equals what the compiler returns for that call, and every
             `(tool ...)` quote in the reasoning equals a result of the transcript
  legality   the submitted move is in the packet's legal moves
  claims     `model.eval.explain` finds zero failed claims, at least one checkable claim, and (unless
             `allow_unchecked`) no claim it saw but could not check
  agreement  the move equals the chosen move (`row["target"]`, or `chosen(row)`: a move-policy head's move)

Rewards are rules only. No engine, no evaluation of move quality: an engine-scored reward is a separate
labelled arm (docs/model/reasoning-v2.md) and is not in this file. Duplicate completions for one row are
kept once. Kept rows are written with their claim counts; nothing is written for rejected samples except
the reason counts in the stats.

    python -m model.corpus.rejection run --corpus DIR --predictor pkg.module:callable [--n 4 --split train
        --limit N --out kept.jsonl --stats stats.json --knightlm BIN --allow-unchecked]
"""

import argparse
import importlib
import json
from pathlib import Path
import subprocess

from model import prompt as prompt_module
from model.corpus import pipeline, reasoning2
from model.eval import explain

REASONS = ("format", "results_differ", "misquote", "illegal_move", "failed_claim", "no_checkable_claim",
           "unchecked_claim", "move_disagrees")


class Rejected(Exception):
    def __init__(self, reason, detail=""):
        super().__init__(f"{reason}: {detail}")
        self.reason = reason


def segments_of(completion):
    """Loss segments of a transcript: calls are written by the model, results by the harness."""
    blocks = list(reasoning2.BLOCK.finditer(completion))
    segments = []
    for index, block in enumerate(blocks):
        end = blocks[index + 1].start() if index + 1 < len(blocks) else len(completion)
        segments.append({"text": completion[block.start():end], "loss": block.group(1) is not None})
    return segments


def check_sample(row, completion, variant, binary, chosen=None, allow_unchecked=False):
    """Return the claim counts of an accepted sample or raise Rejected(reason)."""
    packet = row["packet"]
    try:
        if variant == "a":
            reads, final = [], prompt_module.parse_play_completion(completion)
        else:
            reads, final = reasoning2.parse_transcript(completion)
    except ValueError as error:  # PlayCompletionError is a ValueError
        raise Rejected("format", getattr(error, "code", str(error))) from None
    if reads:
        try:
            again = reasoning2.compute_reads(binary, [{"fen": packet["fen"], "calls": [(n, a) for n, a, _ in reads]}])[0]
        except ValueError as error:
            raise Rejected("results_differ", str(error)) from None
        if again != [result for _, _, result in reads]:
            raise Rejected("results_differ", "a result is not what the compiler returns")
        try:
            reasoning2.check_quotes(final["reasoning"], reads, require=True)
        except (ValueError, KeyError) as error:
            raise Rejected("misquote", str(error)) from None
    elif variant == "b":
        raise Rejected("format", "variant b needs at least one read call")
    if final["move"] not in packet["legal_moves"]:
        raise Rejected("illegal_move", final["move"])
    result = explain.check_explanation(packet["fen"], packet["legal_moves"], final["reasoning"], final["move"])
    if result["failed_total"]:
        raise Rejected("failed_claim", json.dumps(result["failures"][:2]))
    if not result["checkable_total"]:
        raise Rejected("no_checkable_claim")
    if not allow_unchecked and sum(result["unchecked"].values()):
        raise Rejected("unchecked_claim", json.dumps(result["unchecked"]))
    if final["move"] != (chosen(row) if chosen else row["target"]):
        raise Rejected("move_disagrees", final["move"])
    return {"checkable": result["checkable_total"], "failed": 0,
            "by_type": {k: v["checkable"] for k, v in result["claim_types"].items() if v["checkable"]},
            "unchecked": sum(result["unchecked"].values())}, final, reads


def kept_row(row, k, variant, completion, claims, final, reads):
    out = {key: row[key] for key in ("family", "game_id", "packet", "ply", "position_key", "source", "target",
                                     "utc_date", "utc_time", "prompt_kind", "prompt")}
    out.update({"id": f"rs{variant}-{row['id'].split('-', 1)[1]}-{k}", "variant": variant, "reasoning": final["reasoning"],
                "move": final["move"], "reads": [{"name": n, "arguments": a} for n, a, _ in reads],
                "claims": claims, "completion": completion, "origin": "rejection_sampling",
                "rewards": "rules only: format, results, legality, claims, agreement"})
    if variant == "b":
        out["segments"] = segments_of(completion)
    return out


def run(rows, predictor, n, variant, binary, chosen=None, allow_unchecked=False):
    """Sample `n` completions per row; return (kept rows, stats). Rows are corpus rows (any reasoning-v2 split)."""
    kept, rejected, per_row = [], dict.fromkeys(REASONS, 0), {}
    samples = duplicates = 0
    claims = {"checkable": 0, "failed": 0}
    for row in rows:
        seen, count = set(), 0
        for k, completion in enumerate(predictor(row, n)[:n]):
            samples += 1
            if completion in seen:
                duplicates += 1
                continue
            seen.add(completion)
            try:
                found, final, reads = check_sample(row, completion, variant, binary, chosen, allow_unchecked)
            except Rejected as error:
                rejected[error.reason] += 1
                continue
            kept.append(kept_row(row, k, variant, completion, found, final, reads))
            claims["checkable"] += found["checkable"]
            count += 1
        per_row[count] = per_row.get(count, 0) + 1
    stats = {"rows": len(rows), "samples": samples, "duplicates": duplicates, "kept": len(kept),
             "rejected": rejected, "rows_with_a_kept_sample": sum(v for k, v in per_row.items() if k),
             "kept_per_row": dict(sorted(per_row.items())), "kept_claims": claims,
             "variant": variant, "n": n,
             "rewards": "rules only (format, results, legality, claims, agreement); no engine"}
    return kept, stats


def load_predictor(spec):
    module, _, name = spec.partition(":")
    return getattr(importlib.import_module(module), name)


def reference_predictor(row, n):
    """The corpus completion itself: every sample passes. For smoke tests of the loop, not a model."""
    return [row["completion"]]


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    commands = parser.add_subparsers(dest="command", required=True)
    go = commands.add_parser("run")
    go.add_argument("--corpus", type=Path, required=True, help="a reasoning-v2 corpus (its rows supply prompt and packet)")
    go.add_argument("--predictor", required=True, help="pkg.module:callable taking (row, n) and returning completions")
    go.add_argument("--n", type=int, default=4)
    go.add_argument("--split", choices=pipeline.SPLITS, default="train")
    go.add_argument("--limit", type=int, default=None)
    go.add_argument("--out", type=Path, default=None, help="kept rows (JSONL); refused for the holdout split")
    go.add_argument("--stats", type=Path, default=None)
    go.add_argument("--knightlm", type=Path, default=None)
    go.add_argument("--allow-unchecked", action="store_true")
    go.add_argument("--allow-rules-drift", action="store_true")
    args = parser.parse_args(argv)
    if args.split == "holdout" and args.out:
        parser.error("holdout samples measure the pass rate; they are never written as training rows")
    binary = Path(args.knightlm or prompt_module.knightlm_binary()).resolve()
    try:
        train, holdout = reasoning2.read_corpus(args.corpus, args.allow_rules_drift)
        variant = json.loads((args.corpus / pipeline.MANIFEST).read_text())["variant"]
        rows = (train if args.split == "train" else holdout)[: args.limit]
        kept, stats = run(rows, load_predictor(args.predictor), args.n, variant, binary, None, args.allow_unchecked)
    except (OSError, ValueError, KeyError, AttributeError, ImportError, subprocess.SubprocessError) as error:
        parser.exit(1, f"rejection sampling failed: {error}\n")
    if args.out:
        with args.out.open("x", encoding="utf-8") as stream:
            for row in kept:
                stream.write(json.dumps(row, separators=(",", ":")) + "\n")
    stats["corpus"] = args.corpus.name
    stats["split"] = args.split
    text = json.dumps(stats, indent=2)
    if args.stats:
        args.stats.write_text(text + "\n")
    print(text)


if __name__ == "__main__":
    main()
