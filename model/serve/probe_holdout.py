"""Record raw and constrained behaviour of a checkpoint on holdout positions.

  python -m model.serve.probe_holdout --corpus C --base-dir B [--adapter-dir A] --count 50 --output new.jsonl
"""

import argparse
import json
from pathlib import Path

from model.serve.holdout import read_holdout
from model.serve.inference import Predictor


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus", type=Path, required=True)
    parser.add_argument("--base-dir", type=Path, required=True)
    parser.add_argument("--adapter-dir", type=Path)
    parser.add_argument("--count", type=int, default=50)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    predictor = Predictor(args.base_dir, args.adapter_dir)
    rows = read_holdout(args.corpus, args.count)
    totals = {"raw": 0, "constrained": 0, "raw_match": 0, "constrained_match": 0, "empty_reply": 0}
    with args.output.open("x") as stream:
        for row in rows:
            record = {"game_id": row["game_id"], "target": row["target"]}
            for mode in ("raw", "constrained"):
                p = predictor.predict(row["packet"], mode)
                record[mode] = {"reply": p.raw_reply, "legal": p.legal, "input_tokens": p.input_tokens,
                                "output_tokens": p.output_tokens, "latency_ms": round(p.latency_ms, 1)}
                totals[mode] += p.legal
                totals[f"{mode}_match"] += p.move == row["target"]
            totals["empty_reply"] += not record["raw"]["reply"].strip()
            stream.write(json.dumps(record) + "\n")
    print(json.dumps({"positions": len(rows), **totals}, indent=2))


if __name__ == "__main__":
    main()
