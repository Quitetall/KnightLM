"""Corpus v3 typed intermediate record shared by every source adapter."""

import hashlib
import json

RECORD_FORMAT = "knightlm-record-v3"
KINDS = ("commentary", "text", "move", "reasoning")


def content_hash(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def make_record(source_id, licence, kind, text, positions, provenance):
    """One validated-source record; positions are FENs the notation crate replayed."""
    if kind not in KINDS:
        raise ValueError(f"unknown record kind {kind!r}")
    if not source_id or not licence or not text.strip():
        raise ValueError("source_id, licence and text are required")
    return {
        "format": RECORD_FORMAT,
        "source_id": source_id,
        "licence": licence,
        "content_hash": content_hash(text),
        "kind": kind,
        "text": text,
        "positions": positions,
        "provenance": provenance,
    }


def write_records(path, records):
    with open(path, "x", encoding="utf-8") as stream:
        for record in records:
            stream.write(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n")


def position_key(fen):
    return " ".join(fen.split()[:4])


def audit_records(train, holdout):
    """Refuse a v3 split that leaks. Holdout sources are whole-source: no source_id
    appears in both; no holdout position appears in any training record of any kind;
    no content hash is shared. Raises ValueError naming the first leak found."""
    if not train or not holdout:
        raise ValueError("both splits must be non-empty")
    for record in (*train, *holdout):
        if record.get("format") != RECORD_FORMAT:
            raise ValueError(f"not a {RECORD_FORMAT} record")
    shared = {r["source_id"] for r in train} & {r["source_id"] for r in holdout}
    if shared:
        raise ValueError(f"cross-split source_id leakage: {min(shared)}")
    shared = {r["content_hash"] for r in train} & {r["content_hash"] for r in holdout}
    if shared:
        raise ValueError(f"cross-split content_hash leakage: {min(shared)}")
    held = {position_key(p["fen"]) for r in holdout for p in r["positions"]}
    leaked = sorted(position_key(p["fen"]) for r in train for p in r["positions"] if position_key(p["fen"]) in held)
    if leaked:
        raise ValueError(f"cross-split position leakage: {leaked[0]}")
