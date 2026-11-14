"""Corpus fixtures for the BLUT training-loop tests (CPU, no model weights)."""

import json
from pathlib import Path

from model.corpus.pipeline import MANIFEST, read_corpus, sha256_file, write_corpus

PROVENANCE_KEYS = ("source", "compiler_sha256", "candidates", "candidate_window", "selection", "split", "target")


def slice_corpus(source, output, train_games, holdout_games, shard_games=64):
    """Write the first `train_games` train and `holdout_games` holdout rows of an
    audited corpus as a new, itself audited corpus (a subset of a leak-free split
    is leak-free; `write_corpus` re-audits it anyway). The provenance of the
    source corpus is carried over and the slice says what it was cut from."""
    source, output = Path(source), Path(output)
    train, holdout = read_corpus(source)
    manifest = json.loads((source / MANIFEST).read_text())
    provenance = {key: manifest[key] for key in PROVENANCE_KEYS if key in manifest}
    provenance["slice_of"] = {"manifest_sha256": sha256_file(source / MANIFEST),
                              "train_games": train_games, "holdout_games": holdout_games}
    return write_corpus(output, train[:train_games], holdout[:holdout_games], shard_games, provenance)
