"""Build and audit the sharded, text-only Lichess corpus."""

import argparse
from collections import Counter
from datetime import datetime
import hashlib
import json
from pathlib import Path
import signal
import subprocess

CORPUS_FORMAT = "knightlm-lichess-v2"
SPLITS = ("train", "holdout")
MANIFEST = "manifest.json"


def sha256_file(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def timestamp(row):
    return datetime.strptime(f"{row['utc_date']} {row['utc_time']}", "%Y.%m.%d %H:%M:%S")


def months(rows):
    return sorted({row["utc_date"][:7].replace(".", "-") for row in rows})


def audit_splits(train, holdout):
    if not train or not holdout:
        raise ValueError("both splits must be non-empty")
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


def utc(moment):
    return moment.isoformat() + "Z"


def write_shards(output, name, rows, shard_games):
    files = []
    for index in range(0, len(rows), shard_games):
        shard = rows[index : index + shard_games]
        path = output / f"{name}-{index // shard_games:05}.jsonl"
        with path.open("x", encoding="utf-8") as stream:
            for row in shard:
                stream.write(json.dumps(row, separators=(",", ":")) + "\n")
        files.append({"file": path.name, "games": len(shard), "sha256": sha256_file(path)})
    return {
        "games": len(rows),
        "months": months(rows),
        "first_utc": utc(min(map(timestamp, rows))),
        "last_utc": utc(max(map(timestamp, rows))),
        "files": files,
    }


def write_corpus(output, train, holdout, shard_games, provenance):
    """Write audited splits as fixed-size shards plus a manifest; return the manifest."""
    audit_splits(train, holdout)
    output.mkdir()
    manifest = {
        "format": CORPUS_FORMAT,
        **provenance,
        "shard_games": shard_games,
        "reserved_families": sorted({row["family"] for row in holdout}),
        "shards": {
            name: write_shards(output, name, split, shard_games)
            for name, split in zip(SPLITS, (train, holdout), strict=True)
        },
    }
    (output / MANIFEST).write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest


def read_split(root, name, entry, source):
    rows = []
    for shard in entry["files"]:
        contents = (root / shard["file"]).read_bytes()
        if hashlib.sha256(contents).hexdigest() != shard["sha256"]:
            raise ValueError(f"{shard['file']} checksum mismatch")
        shard_rows = [json.loads(line) for line in contents.splitlines()]
        if not shard_rows or len(shard_rows) != shard["games"]:
            raise ValueError(f"{shard['file']} game count mismatch")
        rows.extend(shard_rows)
    if len(rows) != entry["games"]:
        raise ValueError(f"{name} game count mismatch")
    validate_rows(rows, source)
    stamps = sorted(map(timestamp, rows))
    if (utc(stamps[0]), utc(stamps[-1]), months(rows)) != (
        entry["first_utc"],
        entry["last_utc"],
        entry["months"],
    ):
        raise ValueError(f"{name} time range does not match the manifest")
    return rows


def read_corpus(root):
    """Return (train, holdout) rows after checking every manifest claim."""
    manifest = json.loads((root / MANIFEST).read_text())
    if manifest["format"] != CORPUS_FORMAT:
        raise ValueError("unsupported corpus format")
    listed = {shard["file"] for name in SPLITS for shard in manifest["shards"][name]["files"]}
    present = {path.name for path in root.iterdir()} - {MANIFEST}
    if listed != present:
        raise ValueError(f"shards not accounted for by the manifest: {sorted(listed ^ present)[:3]}")
    splits = [
        read_split(root, name, manifest["shards"][name], manifest["source"]["file"]) for name in SPLITS
    ]
    audit_splits(*splits)
    if manifest["reserved_families"] != sorted({row["family"] for row in splits[1]}):
        raise ValueError("reserved families do not match the holdout")
    return splits


def compile_candidates(binary, source, limit, timeout):
    command = [str(binary.resolve()), "corpus-candidates", "--source", source.name, "--limit", str(limit)]
    if source.suffix == ".zst":
        decoder = subprocess.Popen(["zstd", "-dc", "--", str(source)], stdout=subprocess.PIPE)
        try:
            with decoder.stdout as stream:
                result = subprocess.run(command, stdin=stream, capture_output=True, timeout=timeout)
            # The compiler intentionally closes the pipe once it has enough rows.
            if decoder.wait(timeout=10) not in (0, -signal.SIGPIPE):
                raise ValueError("zstd could not read the source PGN")
        finally:
            if decoder.poll() is None:
                decoder.kill()
                decoder.wait(timeout=10)
    else:
        with source.open("rb") as stream:
            result = subprocess.run(command, stdin=stream, capture_output=True, timeout=timeout)
    if result.returncode:
        raise ValueError(result.stderr.decode().strip())
    rows = [json.loads(line) for line in result.stdout.splitlines()]
    if len(rows) != limit:
        raise ValueError(f"compiler returned {len(rows)} candidates, expected {limit}")
    validate_rows(rows, source.name)
    return rows


def build(args, parser):
    if min(args.train_games, args.holdout_games, args.shard_games) < 1 or (
        args.candidates < args.train_games + args.holdout_games
    ):
        parser.error("positive split sizes and enough candidates are required")
    if args.output.exists():
        parser.error("output must be a new directory")
    try:
        rows = compile_candidates(args.knightlm, args.pgn, args.candidates, args.compile_timeout)
        train, holdout = split_candidates(rows, args.train_games, args.holdout_games)
        stamps = sorted(map(timestamp, rows))
        manifest = write_corpus(
            args.output,
            train,
            holdout,
            args.shard_games,
            {
                "source": {
                    "file": args.pgn.name,
                    "bytes": args.pgn.stat().st_size,
                    "sha256": sha256_file(args.pgn),
                },
                "compiler_sha256": sha256_file(args.knightlm),
                "candidates": args.candidates,
                "candidate_window": {"first_utc": utc(stamps[0]), "last_utc": utc(stamps[-1])},
                "selection": "first eligible stream prefix; one decision at ply 24; rated standard blitz/rapid; both Elo >= 1600",
                "split": "strictly later holdout; disjoint Lichess IDs, ECO two-character groups and scored positions",
                "target": "played_uci_move",
            },
        )
        read_corpus(args.output)
    except (OSError, ValueError, subprocess.TimeoutExpired) as error:
        parser.exit(1, f"corpus build failed: {error}\n")
    shards = manifest["shards"]
    print(
        f"Verified {shards['train']['games']} train and {shards['holdout']['games']} holdout games in {args.output}",
        flush=True,
    )


def audit(args, parser):
    try:
        train, holdout = read_corpus(args.corpus)
    except (OSError, ValueError, KeyError) as error:
        parser.exit(1, f"corpus audit failed: {error}\n")
    print(
        f"Accepted {len(train)} train and {len(holdout)} holdout games in {args.corpus}: "
        "checksums, counts, game IDs, ECO families, scored positions and time order",
        flush=True,
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    make = commands.add_parser("build", help="compile, split, audit and shard a corpus")
    make.add_argument("--pgn", type=Path, required=True, help="Lichess .pgn or .pgn.zst")
    make.add_argument("--output", type=Path, required=True, help="new corpus directory")
    make.add_argument("--knightlm", type=Path, default=Path("target/release/knightlm"))
    make.add_argument("--train-games", type=int, default=1000)
    make.add_argument("--holdout-games", type=int, default=100)
    make.add_argument("--candidates", type=int, default=4000)
    make.add_argument("--shard-games", type=int, default=10000)
    make.add_argument("--compile-timeout", type=int, default=3600, help="seconds")
    make.set_defaults(run=build)
    check = commands.add_parser("audit", help="re-check a corpus directory against its manifest")
    check.add_argument("corpus", type=Path)
    check.set_defaults(run=audit)
    args = parser.parse_args()
    args.run(args, parser)


if __name__ == "__main__":
    main()
