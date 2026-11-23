"""Token cost of ctok/1 sequences against the stock SmolVLM tokenizer, per position.

For the first N holdout positions it counts: the stock tokenizer over the FEN
packet JSON, bare FEN, cgt-lite and cgt-full (as `model.corpus.cgt_token_cost`),
and the ctok T1 sequence (with and without the legal-move list) and T3 lite and
full. Measurement only. Reproduce with

    python -m model.tokenizer.token_cost --corpus DIR --tokenizer DIR --out DIR
"""

import argparse
import json
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from model.corpus import cgt_token_cost as base
from model.tokenizer import cgt, position, vocab

FORMS = ("stock_fen", "stock_fen_packet", "stock_cgt_lite", "stock_cgt_full",
         "t1_board", "t1_board_moves", "t3_cgt_lite", "t3_cgt_full")


def measure_one(binary, packet, count_stock):
    forms = base.render_forms(binary, packet)
    rec = position.packet_to_record(packet)
    no_moves = {k: v for k, v in rec.items() if k != "legal_moves"}
    lite, full = forms["cgt_lite"], forms["cgt_full"]
    out = {
        "stock_fen": count_stock(packet["fen"]),
        "stock_fen_packet": count_stock(forms["fen_packet"]),
        "stock_cgt_lite": count_stock(lite),
        "stock_cgt_full": count_stock(full),
        "t1_board": len(position.encode(no_moves)),
        "t1_board_moves": len(position.encode(rec)),
        "t3_cgt_lite": len(cgt.encode(lite)),
        "t3_cgt_full": len(cgt.encode(full)),
    }
    # the round trip is part of the measurement: a count is only reported for text that decodes back
    assert position.decode(position.encode(rec)) == {**rec, "repetition_count": rec["repetition_count"]}
    assert cgt.decode(cgt.encode(lite)) == lite and cgt.decode(cgt.encode(full)) == full
    return out


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus", type=Path, required=True)
    parser.add_argument("--tokenizer", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--limit", type=int, default=2000)
    parser.add_argument("--binary", type=Path, default=base.BINARY)
    args = parser.parse_args(argv)
    from tokenizers import Tokenizer

    tok = Tokenizer.from_file(str(args.tokenizer / "tokenizer.json"))

    def count_stock(text):
        return len(tok.encode(text, add_special_tokens=False).ids)

    packets = base.read_packets(args.corpus / "holdout-00000.jsonl", args.limit)
    with ThreadPoolExecutor(8) as pool:
        rows = list(pool.map(lambda p: measure_one(args.binary, p, count_stock), packets))
    counts = {form: [r[form] for r in rows] for form in FORMS}
    summary = {form: base.summarize(v) for form, v in counts.items()}
    reference = "stock_fen_packet"
    result = {
        "format": "knightlm-ctok-token-cost-v1",
        "spec_version": vocab.SPEC_VERSION,
        "vocab_sha256": vocab.vocab_sha256(),
        "corpus": args.corpus.name,
        "tokenizer": args.tokenizer.name,
        "positions": len(packets),
        "note": "stock_* use the SmolVLM tokenizer on the packet JSON (without the fixed instruction line), the bare FEN, "
                "and the compiler's cgt-lite/full text; t1_* and t3_* count ctok ids. t1_board_moves lists every legal move "
                "as one token, t1_board omits the move list; every counted sequence round-trips.",
        "tokens": summary,
        "ratio_to_stock_fen_packet": {f: round(summary[f]["mean"] / summary[reference]["mean"], 2) for f in FORMS},
    }
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "summary.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    (args.out / "manifest.json").write_text(vocab.manifest_json())
    with open(args.out / "per_position.jsonl", "w") as handle:
        for i, packet in enumerate(packets):
            handle.write(json.dumps({"i": i, "fen": packet["fen"], **rows[i]}, sort_keys=True) + "\n")
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
