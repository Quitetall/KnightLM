"""Read the identified holdout and hash everything that identifies it.

Handles the sharded Run 1 format (knightlm-lichess-v2) and the single-file pilot
format (knightlm-lichess-pilot-v1). Every shard checksum in the manifest is
verified before a row is used; a mismatch is an error, never a partial read.
"""

import hashlib
import json
from pathlib import Path


def sha256_file(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def holdout_files(manifest):
    holdout = manifest["shards"]["holdout"]
    if "files" in holdout:
        return [(item["file"], item["sha256"], item["games"]) for item in holdout["files"]]
    return [("holdout.jsonl", holdout["sha256"], holdout["games"])]


def load_holdout(corpus_dir, rows=None):
    """Return (rows, info). rows=None reads all; otherwise the first `rows` in shard order."""
    root = Path(corpus_dir)
    manifest_path = root / "manifest.json"
    manifest = json.loads(manifest_path.read_text())
    selected, files = [], []
    for name, expected, games in holdout_files(manifest):
        path = root / name
        contents = path.read_bytes()
        if hashlib.sha256(contents).hexdigest() != expected:
            raise ValueError(f"holdout shard {name} does not match the manifest checksum")
        files.append({"file": name, "sha256": expected, "games": games})
        for line in contents.splitlines():
            if rows is not None and len(selected) >= rows:
                break
            row = json.loads(line)
            packet = row["packet"]
            if row["target"] not in packet["legal_moves"]:
                raise ValueError(f"holdout row {row['game_id']} target is not a legal move")
            selected.append(row)
    if rows is not None and len(selected) < rows:
        raise ValueError(f"holdout has only {len(selected)} rows, {rows} requested")
    info = {
        "corpus_dir": str(root.resolve()),
        "corpus_format": manifest["format"],
        "corpus_manifest_sha256": sha256_file(manifest_path),
        "holdout_files": files,
        "holdout_total_rows": sum(f["games"] for f in files),
        "rows_used": len(selected),
        "selection": "first N rows in manifest shard order",
    }
    return selected, info
