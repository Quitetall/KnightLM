"""Build and verify the bounded, text-only Lichess pilot corpus."""

import argparse
from collections import Counter
from datetime import datetime
import hashlib
import json
from pathlib import Path
import signal
import subprocess

CORPUS_FORMAT = "knightlm-lichess-pilot-v1"


def sha256_file(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def timestamp(row):
    return datetime.strptime(f"{row['utc_date']} {row['utc_time']}", "%Y.%m.%d %H:%M:%S")


def audit_splits(train, holdout):
    for key in ("game_id", "position_key"):
        values = [{row[key] for row in split} for split in (train, holdout)]
        if len(values[0]) != len(train) or len(values[1]) != len(holdout):
            raise ValueError(f"duplicate {key} within split")
    for key in ("game_id", "family", "position_key"):
        overlap = {row[key] for row in train} & {row[key] for row in holdout}
        if overlap:
            raise ValueError(f"cross-split {key} leakage: {min(overlap)}")
    if max(map(timestamp, train)) >= min(map(timestamp, holdout)):
        raise ValueError("holdout must be strictly later than training")


def select_unique(rows, count, excluded=()):
    seen_games = {row["game_id"] for row in excluded}
    seen_positions = {row["position_key"] for row in excluded}
    selected = []
    for row in rows:
        if row["game_id"] in seen_games or row["position_key"] in seen_positions:
            continue
        selected.append(row)
        if len(selected) == count:
            break
        seen_games.add(row["game_id"])
        seen_positions.add(row["position_key"])
    return selected


def split_candidates(rows, train_count, holdout_count):
    ordered = sorted(rows, key=lambda row: (timestamp(row), row["game_id"]))
    cutoff = timestamp(ordered[len(ordered) * 4 // 5])
    earlier = [row for row in ordered if timestamp(row) < cutoff]
    later = [row for row in reversed(ordered) if timestamp(row) >= cutoff]
    families = Counter(row["family"] for row in later)
    held_out = set()
    holdout = []
    for family, _ in families.most_common():
        held_out.add(family)
        holdout = select_unique((row for row in later if row["family"] in held_out), holdout_count)
        if len(holdout) == holdout_count:
            break
    train = select_unique(
        (row for row in earlier if row["family"] not in held_out), train_count, holdout
    )
    if len(train) != train_count or len(holdout) != holdout_count:
        raise ValueError(f"could only split {len(train)} train/{len(holdout)} holdout; increase --candidates")
    audit_splits(train, holdout)
    return train, holdout


def validate_rows(rows, source):
    for row in rows:
        packet = row["packet"]
        if (
            row["source"] != source
            or packet["schema_version"] != 1
            or packet["ruleset"] != "standard-untimed-v1"
            or packet["history_complete"] is not True
            or row["position_key"] != " ".join(packet["fen"].split()[:4])
            or row["target"] not in packet["legal_moves"]
        ):
            raise ValueError(f"invalid corpus row for game {row['game_id']}")


def read_corpus(root):
    manifest = json.loads((root / "manifest.json").read_text())
    if manifest["format"] != CORPUS_FORMAT:
        raise ValueError("unsupported corpus format")
    splits = []
    for name in ("train", "holdout"):
        path = root / f"{name}.jsonl"
        expected = manifest["shards"][name]
        contents = path.read_bytes()
        if hashlib.sha256(contents).hexdigest() != expected["sha256"]:
            raise ValueError(f"{name} shard checksum mismatch")
        rows = [json.loads(line) for line in contents.splitlines()]
        if not rows or len(rows) != expected["games"]:
            raise ValueError(f"{name} shard game count mismatch")
        validate_rows(rows, manifest["source"]["file"])
        splits.append(rows)
    audit_splits(*splits)
    return splits


def compile_candidates(binary, source, limit):
    command = [str(binary.resolve()), "corpus-candidates", "--source", source.name, "--limit", str(limit)]
    if source.suffix == ".zst":
        decoder = subprocess.Popen(["zstd", "-dc", "--", str(source)], stdout=subprocess.PIPE)
        try:
            with decoder.stdout as stream:
                result = subprocess.run(command, stdin=stream, capture_output=True, timeout=180)
            # The compiler intentionally closes the pipe once it has enough rows.
            if decoder.wait(timeout=10) not in (0, -signal.SIGPIPE):
                raise ValueError("zstd could not read the source PGN")
        finally:
            if decoder.poll() is None:
                decoder.kill()
                decoder.wait(timeout=10)
    else:
        with source.open("rb") as stream:
            result = subprocess.run(command, stdin=stream, capture_output=True, timeout=180)
    if result.returncode:
        raise ValueError(result.stderr.decode().strip())
    rows = [json.loads(line) for line in result.stdout.splitlines()]
    if len(rows) != limit:
        raise ValueError(f"compiler returned {len(rows)} candidates, expected {limit}")
    validate_rows(rows, source.name)
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pgn", type=Path, required=True, help="Lichess .pgn or .pgn.zst")
    parser.add_argument("--output", type=Path, required=True, help="new corpus directory")
    parser.add_argument("--knightlm", type=Path, default=Path("target/release/knightlm"))
    parser.add_argument("--train-games", type=int, default=1000)
    parser.add_argument("--holdout-games", type=int, default=100)
    parser.add_argument("--candidates", type=int, default=4000)
    args = parser.parse_args()
    if min(args.train_games, args.holdout_games) < 1 or args.candidates < args.train_games + args.holdout_games:
        parser.error("positive split sizes and enough candidates are required")
    if args.output.exists():
        parser.error("output must be a new directory")
    try:
        rows = compile_candidates(args.knightlm, args.pgn, args.candidates)
        train, holdout = split_candidates(rows, args.train_games, args.holdout_games)
    except (OSError, ValueError, subprocess.TimeoutExpired) as error:
        parser.exit(1, f"corpus build failed: {error}\n")
    args.output.mkdir()
    shards = {}
    for name, split in (("train", train), ("holdout", holdout)):
        path = args.output / f"{name}.jsonl"
        with path.open("x", encoding="utf-8") as stream:
            for row in split:
                stream.write(json.dumps(row, separators=(",", ":")) + "\n")
        shards[name] = {
            "games": len(split),
            "sha256": sha256_file(path),
            "first_utc": min(map(timestamp, split)).isoformat() + "Z",
            "last_utc": max(map(timestamp, split)).isoformat() + "Z",
        }
    manifest = {
        "format": CORPUS_FORMAT,
        "source": {"file": args.pgn.name, "bytes": args.pgn.stat().st_size},
        "compiler_sha256": sha256_file(args.knightlm),
        "candidates": args.candidates,
        "selection": "first eligible stream prefix; one decision at ply 24; rated standard blitz/rapid; both Elo >= 1600",
        "split": "strictly later holdout; disjoint Lichess IDs, ECO two-character groups and scored positions",
        "target": "played_uci_move",
        "shards": shards,
    }
    (args.output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Verified {len(train)} train and {len(holdout)} holdout games in {args.output}", flush=True)


if __name__ == "__main__":
    main()
