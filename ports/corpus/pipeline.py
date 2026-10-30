"""Corpus pipeline (KLM-WAR-0007): ingest -> join -> split -> audit.

Seams consumed: corpus-shards (manifest schema), packet-row (per-row packets).
No engine access; labels are joined, never treated as ground truth.
"""

import json, hashlib, os, sys
from dataclasses import dataclass


@dataclass
class SplitAudit:
    train_ids: set
    eval_ids: set
    leaked: list

    def check_family_disjoint(self, family_of: dict) -> bool:
        train_families = {family_of[g] for g in self.train_ids}
        eval_families = {family_of[g] for g in self.eval_ids}
        self.leaked = sorted(train_families & eval_families)
        return not self.leaked

    def check_position_disjoint(self, positions_of: dict) -> bool:
        tp = {p for g in self.train_ids for p in positions_of[g]}
        ep = {p for g in self.eval_ids for p in positions_of[g]}
        pos_leak = tp & ep
        if pos_leak:
            self.leaked.extend(sorted(pos_leak))
        return not pos_leak


def join_label(position_key: str, evals_index: dict, depth_floor: int = 20) -> dict | None:
    """Join by normalized position key; highest depth wins; first PV is policy target.
    Returns a LABEL dict (never treated as ground truth)."""
    rows = evals_index.get(position_key)
    if not rows:
        return None
    rows = [r for r in rows if r.get("depth", 0) >= depth_floor]
    if not rows:
        return None
    best = max(rows, key=lambda r: r.get("depth", 0))
    pv = best.get("pv", "").split()
    return {
        "label_source": "lichess_db_eval",
        "depth": best.get("depth"),
        "pv_uci": pv[0] if pv else None,
        "cp": best.get("cp"),
        "mate": best.get("mate"),
        "is_label": True,  # never ground truth
    }


def sha256_file(path: str) -> str:
    import hashlib
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for c in iter(lambda: f.read(65536), b""):
            h.update(c)
    return h.hexdigest()


if __name__ == "__main__":
    print("corpus pipeline scaffold; see KLM-WAR-0007")
EOF
echo written