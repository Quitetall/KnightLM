"""Token cost of Chess Graph Text (cgt/1) against the FEN packet, per position.

Renders holdout positions three ways with the Rust compiler and counts tokens
with the SmolVLM tokenizer: the packet JSON the Run 1 prompt carries (without
the fixed instruction line), cgt-lite and cgt-full. Measurement only: it writes
no prompt and touches no training or eval path.

    python -m model.corpus.cgt_token_cost --corpus DIR --tokenizer DIR --out DIR
"""

import argparse
import json
import math
import subprocess
import sys
from pathlib import Path

from model import prompt

ROOT = Path(__file__).resolve().parents[2]
BINARY = next((p for p in (ROOT / "target/debug/knightlm", ROOT / "target/release/knightlm") if p.exists()), None)
FORMS = ("fen_packet", "cgt_lite", "cgt_full")


def percentile(values, q):
    """Nearest-rank percentile (q in 0..100) of a non-empty list."""
    ordered = sorted(values)
    return ordered[max(0, math.ceil(q / 100 * len(ordered)) - 1)]


def summarize(counts):
    return {
        "n": len(counts),
        "mean": round(sum(counts) / len(counts), 1),
        "p50": percentile(counts, 50),
        "p95": percentile(counts, 95),
        "max": max(counts),
    }


def render_cgt(binary, packet, variant):
    rep = packet["facts"].get("repetition_count")
    cmd = [str(binary), "cgt", "--fen", packet["fen"], "--variant", variant]
    if rep is not None:
        cmd += ["--rep", str(rep)]
    out = subprocess.run(cmd, check=True, capture_output=True, text=True, timeout=30)
    return out.stdout.rstrip("\n")


def render_forms(binary, packet):
    return {
        "fen_packet": prompt.packet_json(packet),
        "cgt_lite": render_cgt(binary, packet, "lite"),
        "cgt_full": render_cgt(binary, packet, "full"),
    }


def read_packets(path, limit):
    packets = []
    with open(path) as handle:
        for line in handle:
            if len(packets) == limit:
                break
            packets.append(json.loads(line)["packet"])
    return packets


def measure(packets, count_tokens, binary):
    counts = {form: [] for form in FORMS}
    for packet in packets:
        for form, text in render_forms(binary, packet).items():
            counts[form].append(count_tokens(text))
    return {form: summarize(values) for form, values in counts.items()}, counts


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus", type=Path, required=True)
    parser.add_argument("--tokenizer", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--limit", type=int, default=2000)
    parser.add_argument("--binary", type=Path, default=BINARY)
    args = parser.parse_args(argv)
    if args.binary is None:
        sys.exit("build the knightlm binary first (cargo build -p knightlm-cli)")
    from tokenizers import Tokenizer

    tok = Tokenizer.from_file(str(args.tokenizer / "tokenizer.json"))

    def count_tokens(text):
        return len(tok.encode(text, add_special_tokens=False).ids)

    packets = read_packets(args.corpus / "holdout-00000.jsonl", args.limit)
    summary, counts = measure(packets, count_tokens, args.binary)
    result = {
        "format": "knightlm-cgt-token-cost-v1",
        "corpus": args.corpus.name,
        "tokenizer": args.tokenizer.name,
        "positions": len(packets),
        "note": "fen_packet is the packet JSON of the Run 1 prompt without the fixed instruction line",
        "tokens": summary,
        "ratio_to_fen_packet": {
            form: round(summary[form]["mean"] / summary["fen_packet"]["mean"], 2) for form in FORMS
        },
    }
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "summary.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    with open(args.out / "per_position.jsonl", "w") as handle:
        for i, packet in enumerate(packets):
            row = {"i": i, "fen": packet["fen"]}
            row.update({form: counts[form][i] for form in FORMS})
            handle.write(json.dumps(row, sort_keys=True) + "\n")
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
