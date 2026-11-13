import json
from pathlib import Path
import shutil
import tempfile
import unittest

from model.corpus.pipeline import (
    MANIFEST,
    audit_splits,
    months,
    read_corpus,
    sha256_file,
    split_candidates,
    timestamp,
    utc,
    write_corpus,
)

SOURCE = "fixture.pgn"


def game(number, family, minute):
    key = f"position{number} w - -"
    return {"game_id": str(number), "source": SOURCE, "family": family, "ply": 24,
            "utc_date": "2026.09.01", "utc_time": f"{minute // 60:02}:{minute % 60:02}:00",
            "position_key": key, "target": "e2e4",
            "packet": {"schema_version": 1, "ruleset": "standard-untimed-v1",
                       "history_complete": True, "fen": f"{key} 0 1", "legal_moves": ["e2e4"]}}


def provenance():
    return {"source": {"file": SOURCE, "bytes": 1, "sha256": "0" * 64}}


def fixture_splits():
    rows = [game(i, ("A0", "B1", "C2")[i % 3], i) for i in range(120)]
    return split_candidates(rows, 25, 5)


def write_fixture(root, shard_games=10):
    train, holdout = fixture_splits()
    return write_corpus(root, train, holdout, shard_games, provenance())


def plant_leak(root, kind):
    """Rewrite holdout shard 0 with one planted leak, keeping every checksum and range valid."""
    manifest = json.loads((root / MANIFEST).read_text())
    train, holdout = manifest["shards"]["train"], manifest["shards"]["holdout"]
    train_row = json.loads((root / train["files"][0]["file"]).read_text().splitlines()[0])
    shard = holdout["files"][0]
    path = root / shard["file"]
    rows = [json.loads(line) for line in path.read_text().splitlines()]
    if kind == "dup_game_id":
        rows[1]["game_id"] = rows[0]["game_id"]
    elif kind in ("game_id", "family"):
        rows[0][kind] = train_row[kind]
    elif kind == "position_key":
        rows[0]["position_key"] = train_row["position_key"]
        rows[0]["packet"]["fen"] = f"{train_row['position_key']} 0 1"
    elif kind == "time":
        rows[0]["utc_date"], rows[0]["utc_time"] = train_row["utc_date"], train_row["utc_time"]
    else:
        raise ValueError(kind)
    path.write_text("".join(json.dumps(row, separators=(",", ":")) + "\n" for row in rows))
    shard["sha256"] = sha256_file(path)
    every = rows[:]
    for other in holdout["files"][1:]:
        every += [json.loads(line) for line in (root / other["file"]).read_text().splitlines()]
    stamps = sorted(map(timestamp, every))
    holdout["first_utc"], holdout["last_utc"], holdout["months"] = utc(stamps[0]), utc(stamps[-1]), months(every)
    (root / MANIFEST).write_text(json.dumps(manifest, indent=2) + "\n")


class CorpusSplitTests(unittest.TestCase):
    def test_family_reservation_and_temporal_order(self):
        rows = [game(i, "A0" if i % 2 else "B1", i) for i in range(20)]
        train, holdout = split_candidates(list(reversed(rows)), 5, 2)
        self.assertEqual((len(train), len(holdout)), (5, 2))
        self.assertEqual((train, holdout), split_candidates(rows, 5, 2))
        audit_splits(train, holdout)

    def test_planted_leaks(self):
        train = game(1, "A0", 0)
        holdout = game(2, "B1", 1)
        for key in ("game_id", "family", "position_key"):
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, f"cross-split {key}"):
                audit_splits([train], [dict(holdout, **{key: train[key]})])
        with self.assertRaisesRegex(ValueError, "strictly later"):
            audit_splits([train], [dict(holdout, utc_time=train["utc_time"])])

    def test_too_few_disjoint_games_refuses(self):
        rows = [game(i, "A0", i) for i in range(10)]
        with self.assertRaisesRegex(ValueError, "could only split"):
            split_candidates(rows, 1, 1)

    def test_empty_split_refuses(self):
        with self.assertRaisesRegex(ValueError, "non-empty"):
            audit_splits([], [game(1, "A0", 0)])


class ShardedCorpusTests(unittest.TestCase):
    def setUp(self):
        self.scratch = Path(tempfile.mkdtemp(prefix="knightlm-corpus-test-"))
        self.addCleanup(shutil.rmtree, self.scratch)
        self.root = self.scratch / "corpus"
        self.manifest = write_fixture(self.root)

    def test_clean_corpus_is_accepted_and_accounted_for(self):
        train, holdout = read_corpus(self.root)
        self.assertEqual((len(train), len(holdout)), (25, 5))
        self.assertEqual([s["games"] for s in self.manifest["shards"]["train"]["files"]], [10, 10, 5])
        self.assertEqual([s["games"] for s in self.manifest["shards"]["holdout"]["files"]], [5])
        self.assertEqual(self.manifest["shards"]["train"]["months"], ["2026-09"])

    def test_rebuild_reproduces_manifest_and_shards(self):
        again = self.scratch / "again"
        self.assertEqual(self.manifest, write_fixture(again))
        names = sorted(path.name for path in self.root.iterdir())
        self.assertEqual(names, sorted(path.name for path in again.iterdir()))
        for name in names:
            self.assertEqual((self.root / name).read_bytes(), (again / name).read_bytes())

    def test_each_planted_leak_is_refused_by_the_audit_not_the_checksum(self):
        expected = {
            "game_id": "cross-split game_id",
            "family": "cross-split family",
            "position_key": "cross-split position_key",
            "dup_game_id": "duplicate game_id within split",
            "time": "strictly later",
        }
        for kind, message in expected.items():
            with self.subTest(kind=kind):
                planted = self.scratch / f"planted-{kind}"
                shutil.copytree(self.root, planted)
                plant_leak(planted, kind)
                with self.assertRaisesRegex(ValueError, message):
                    read_corpus(planted)

    def test_tampering_and_unaccounted_shards_are_refused(self):
        flipped = self.scratch / "flipped"
        shutil.copytree(self.root, flipped)
        shard = flipped / "train-00000.jsonl"
        shard.write_bytes(shard.read_bytes().replace(b"e2e4", b"e2e3", 1))
        with self.assertRaisesRegex(ValueError, "checksum mismatch"):
            read_corpus(flipped)
        missing = self.scratch / "missing"
        shutil.copytree(self.root, missing)
        (missing / "train-00001.jsonl").unlink()
        with self.assertRaisesRegex(ValueError, "not accounted for"):
            read_corpus(missing)
        extra = self.scratch / "extra"
        shutil.copytree(self.root, extra)
        shutil.copy(extra / "train-00000.jsonl", extra / "train-00009.jsonl")
        with self.assertRaisesRegex(ValueError, "not accounted for"):
            read_corpus(extra)
