#!/usr/bin/env python3
"""Validate evidence coverage for a selected RBC UI slice or the full cutover."""

import argparse
import json
import re
from collections import Counter
from pathlib import Path

PLAN = Path(__file__).resolve().parents[1] / "apps/rusty-brick-chess/UI-REDESIGN-PLAN.md"
CHECK = re.compile(r"^(?:\| |\- )\[[ x]\] (RBCUI-[A-Z]+-\d+)\b", re.MULTILINE)
FIELDS = ("build", "platform", "input", "fixture", "old_path", "new_path", "expected", "check", "evidence")


def validate(plan, ledger, build, requested):
    ids = CHECK.findall(plan)
    if not ids or len(ids) != len(set(ids)):
        raise ValueError("checklist is empty or has duplicate IDs")
    required = set(requested or ids)
    if not required <= set(ids):
        raise ValueError(f"unknown requested IDs: {', '.join(sorted(required - set(ids)))}")
    if not isinstance(ledger, list) or any(
        not isinstance(row, dict) or not isinstance(row.get("id"), str) for row in ledger
    ):
        raise ValueError("evidence must be a JSON array of records with string IDs")
    counts = Counter(row["id"] for row in ledger)
    errors = [f"{item}: duplicate evidence" for item, count in counts.items() if count != 1]
    errors.extend(f"{item}: unknown ID" for item in sorted(counts.keys() - set(ids)))
    errors.extend(f"{item}: missing evidence" for item in sorted(required - counts.keys()))
    for row in ledger:
        item = row["id"]
        if row.get("build") != build:
            errors.append(f"{item}: evidence is for a different build")
        if item not in required:
            continue
        if row.get("status") != "PASS":
            errors.append(f"{item}: {row.get('status', 'missing status')}")
        missing = [key for key in FIELDS if not isinstance(row.get(key), str) or not row[key].strip()]
        if missing:
            errors.append(f"{item}: missing {', '.join(missing)}")
    return required, errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("ledger", type=Path)
    parser.add_argument("--build", required=True, help="candidate build identifier recorded in evidence")
    parser.add_argument("--id", action="append", help="required ID for this slice, repeat as needed")
    args = parser.parse_args()
    try:
        required, errors = validate(PLAN.read_text(), json.loads(args.ledger.read_text()), args.build, args.id)
    except (OSError, ValueError) as error:
        parser.exit(1, f"FAIL: {error}\n")
    if errors:
        parser.exit(1, "\n".join(errors) + "\n")
    print(f"PASS: {len(required)} required checklist records for {args.build}")


if __name__ == "__main__":
    main()
