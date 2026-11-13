"""Deterministically convert a knightlm-lichess-v2 corpus into BLUT SFT rows.

    python3 -m knightlm_blut.materialize --corpus CORPUS --out DIR

Writes DIR/dataset.jsonl (every train shard converted, in manifest order, one
{"prompt","completion"} object per line) and DIR/receipt.json (per-shard source
and converted hashes plus the hashes that pin the prompt and this converter).

The prompt is `model.prompt.user_text(packet)` verbatim, the same function the
model is served with; the completion is the played UCI move. Nothing here
re-implements prompt text. The corpus is audited first (`python3 -m
model.corpus.pipeline audit CORPUS`) and a failed audit refuses the conversion.

Re-running on an existing DIR re-hashes dataset.jsonl against the receipt and
the inputs it was built from; a match is reused untouched, anything else is
rebuilt from the audited corpus and replaced atomically.
"""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

from knightlm_blut.common import repo_root, sha256_bytes, sha256_file

RECEIPT_FORMAT = "knightlm-blut-materialized-v1"
CORPUS_FORMAT = "knightlm-lichess-v2"
DATASET = "dataset.jsonl"
RECEIPT = "receipt.json"


class MaterializeError(ValueError):
    """The corpus failed the audit or does not match its manifest."""


def _prompt_module(root):
    sys.path.insert(0, str(root))
    try:
        from model import prompt
    finally:
        sys.path.remove(str(root))
    return prompt


def convert_row(row, prompt_module):
    """One corpus row -> one SFT row. The only place the prompt is built."""
    return {"prompt": prompt_module.user_text(row["packet"]), "completion": row["target"]}


def encode_row(record):
    return (json.dumps(record, ensure_ascii=False, separators=(",", ":")) + "\n").encode("utf-8")


def run_audit(corpus, root):
    result = subprocess.run(
        [sys.executable, "-m", "model.corpus.pipeline", "audit", str(corpus)],
        cwd=root, capture_output=True, text=True,
    )
    if result.returncode != 0:
        raise MaterializeError(
            f"corpus audit failed for {corpus}: {(result.stderr or result.stdout).strip()}")
    return result.stdout.strip()


def pins(corpus, root):
    """Hashes that decide whether an existing materialization is still valid."""
    return {
        "corpus_manifest_sha256": sha256_file(Path(corpus) / "manifest.json"),
        "prompt_py_sha256": sha256_file(root / "model" / "prompt.py"),
        "materializer_sha256": sha256_file(Path(__file__)),
    }


def existing_receipt(out, expected):
    """The receipt in `out` if dataset.jsonl still matches it and the pins, else None."""
    receipt_path, data_path = out / RECEIPT, out / DATASET
    if not (receipt_path.is_file() and data_path.is_file()):
        return None
    try:
        receipt = json.loads(receipt_path.read_text())
    except ValueError:
        return None
    if receipt.get("format") != RECEIPT_FORMAT:
        return None
    if any(receipt.get(key) != value for key, value in expected.items()):
        return None
    if receipt.get("dataset_sha256") != sha256_file(data_path):
        return None
    return receipt


def build(corpus, out, root, prompt_module):
    manifest = json.loads((corpus / "manifest.json").read_text())
    if manifest.get("format") != CORPUS_FORMAT:
        raise MaterializeError(f"unsupported corpus format {manifest.get('format')!r}")
    out.mkdir(parents=True, exist_ok=True)
    tmp = out / (DATASET + ".tmp")
    shards, total = [], 0
    with tmp.open("wb") as sink:
        for entry in manifest["shards"]["train"]["files"]:
            source = (corpus / entry["file"]).read_bytes()
            if sha256_bytes(source) != entry["sha256"]:
                raise MaterializeError(f"{entry['file']} changed since the audit")
            converted, rows = [], 0
            for line in source.splitlines():
                converted.append(encode_row(convert_row(json.loads(line), prompt_module)))
                rows += 1
            if rows != entry["games"]:
                raise MaterializeError(f"{entry['file']} row count differs from the manifest")
            blob = b"".join(converted)
            sink.write(blob)
            total += rows
            shards.append({"source_file": entry["file"], "source_sha256": entry["sha256"],
                           "rows": rows, "converted_sha256": sha256_bytes(blob)})
        sink.flush()
        os.fsync(sink.fileno())
    if total != manifest["shards"]["train"]["games"]:
        raise MaterializeError("train row total differs from the manifest")
    os.replace(tmp, out / DATASET)
    return shards, total


def materialize(corpus, out, root=None):
    """Audit, convert and write the receipt; returns the receipt. Reuses a valid result."""
    root = Path(root) if root else repo_root()
    corpus, out = Path(corpus).resolve(), Path(out)
    audit_note = run_audit(corpus, root)
    expected = pins(corpus, root)
    reused = existing_receipt(out, expected)
    if reused is not None:
        return {**reused, "reused": True}
    prompt_module = _prompt_module(root)
    shards, total = build(corpus, out, root, prompt_module)
    receipt = {
        "format": RECEIPT_FORMAT,
        "corpus_dir": str(corpus),
        "corpus_audit": audit_note,
        **expected,
        "instruction": prompt_module.INSTRUCTION,
        "dataset_sha256": sha256_file(out / DATASET),
        "rows": total,
        "shards": shards,
    }
    (out / RECEIPT).write_text(json.dumps(receipt, indent=2) + "\n")
    return {**receipt, "reused": False}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--corpus", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--expect-manifest-sha256", help="refuse unless the corpus manifest has this sha256")
    args = parser.parse_args(argv)
    try:
        if args.expect_manifest_sha256:
            actual = sha256_file(args.corpus / "manifest.json")
            if actual != args.expect_manifest_sha256:
                raise MaterializeError(
                    f"corpus manifest sha256 {actual} != expected {args.expect_manifest_sha256}")
        receipt = materialize(args.corpus, args.out)
    except (MaterializeError, OSError, KeyError, ValueError) as error:
        print(f"materialize refused: {error}", file=sys.stderr)
        return 1
    print(json.dumps({"rows": receipt["rows"], "dataset_sha256": receipt["dataset_sha256"],
                      "reused": receipt["reused"], "receipt": str(args.out / RECEIPT)}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
