"""Minimal verified holdout reader for the sharded Run 1 corpus (knightlm-lichess-v2).

`model.corpus.pipeline.read_corpus` on main reads only the v1 pilot layout; the
v2 reader is on the unmerged agent/run1-corpus branch. Until that lands this
reads just the holdout shards, rechecking each shard's sha256 from the manifest.
"""

import hashlib
import json
from pathlib import Path


def read_holdout(root, limit=None):
    root = Path(root)
    manifest = json.loads((root / "manifest.json").read_text())
    rows = []
    for shard in manifest["shards"]["holdout"]["files"]:
        data = (root / shard["file"]).read_bytes()
        if hashlib.sha256(data).hexdigest() != shard["sha256"]:
            raise ValueError(f"{shard['file']} checksum mismatch")
        rows.extend(json.loads(line) for line in data.splitlines())
        if limit is not None and len(rows) >= limit:
            break
    return rows[:limit]
