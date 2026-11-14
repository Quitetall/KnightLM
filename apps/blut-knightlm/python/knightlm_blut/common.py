"""Shared helpers: hashing and locating the KnightLM repository root."""

import hashlib
import os
from pathlib import Path


def sha256_bytes(data):
    return hashlib.sha256(data).hexdigest()


def sha256_file(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def repo_root():
    """The KnightLM checkout: $KNIGHTLM_REPO, else the repository two levels above apps/blut-knightlm/."""
    env = os.environ.get("KNIGHTLM_REPO")
    if env:
        return Path(env).resolve()
    return Path(__file__).resolve().parents[4]
