"""Token counts of the S0 rows of two matched arms (SmolVLM tokenizer), per split and question type.

Counts the row's prompt text (the play prompt plus question; the fixed prefix with the tool schema
is included) and its completion text, as the trainer sees them before chat-template markers.
Measurement only.

    python -m model.corpus.s0_token_cost --arm t0=DIR --arm t2=DIR --tokenizer DIR --out DIR
"""

import argparse
import json
import math
from pathlib import Path

from model.corpus import pipeline


def summarize(counts):
    ordered = sorted(counts)
    pick = lambda q: ordered[max(0, math.ceil(q / 100 * len(ordered)) - 1)]  # noqa: E731
    return {"n": len(ordered), "mean": round(sum(ordered) / len(ordered), 1), "p50": pick(50), "p95": pick(95),
            "max": ordered[-1]}


def count_rows(rows, count):
    out = {"prompt": [], "completion": [], "total": [], "by_type": {}}
    for row in rows:
        p, c = count(row["prompt"]), count(row["completion"])
        out["prompt"].append(p)
        out["completion"].append(c)
        out["total"].append(p + c)
        out["by_type"].setdefault(row["qa"]["type"], []).append(p + c)
    return out


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--arm", action="append", required=True, metavar="NAME=DIR")
    parser.add_argument("--tokenizer", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    from tokenizers import Tokenizer

    tok = Tokenizer.from_file(str(args.tokenizer / "tokenizer.json"))
    count = lambda text: len(tok.encode(text, add_special_tokens=False).ids)  # noqa: E731
    result = {"format": "knightlm-s0-token-cost-v1", "tokenizer": args.tokenizer.name, "arms": {}}
    for item in args.arm:
        name, _, path = item.partition("=")
        train, holdout = pipeline.read_corpus(Path(path), allow_rules_drift=True)
        arm = {}
        for split, rows in (("train", train), ("holdout", holdout)):
            counted = count_rows(rows, count)
            arm[split] = {"prompt": summarize(counted["prompt"]), "completion": summarize(counted["completion"]),
                          "total": summarize(counted["total"]),
                          "total_by_question_type": {t: summarize(v) for t, v in sorted(counted["by_type"].items())}}
        result["arms"][name] = arm
    names = list(result["arms"])
    if len(names) == 2:
        a, b = (result["arms"][n]["train"]["total"]["mean"] for n in names)
        result["train_total_mean_ratio"] = {f"{names[1]}/{names[0]}": round(b / a, 3)}
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "s0-token-cost.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps({n: {s: a[s]["total"] for s in a} for n, a in result["arms"].items()}, indent=1))


if __name__ == "__main__":
    main()
