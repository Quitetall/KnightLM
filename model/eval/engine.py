"""Stockfish scoring through the Rust tool (`knightlm score-moves` / `rank-positions`).

Python never talks UCI. The Rust subcommands use crates/engine with one thread,
64 MB hash and a `ucinewgame` before every search, so a fixed-depth score does
not depend on query order. Inputs are deduplicated and sharded across worker
processes; results are matched back by id and a missing result is an error.
"""

import hashlib
import json
import os
import shutil
import subprocess
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
STOCKFISH_DEFAULT = Path.home() / ".local/bin/stockfish"


def find_binary(explicit=None):
    candidates = [explicit, os.environ.get("KNIGHTLM_BIN"), REPO / "target/release/knightlm",
                  shutil.which("knightlm")]
    for candidate in candidates:
        if candidate and Path(candidate).is_file():
            return Path(candidate).resolve()
    raise FileNotFoundError("knightlm binary not found; build with `cargo build --release -p knightlm-cli`"
                            " or set KNIGHTLM_BIN")


def find_stockfish(explicit=None):
    candidates = [explicit, os.environ.get("STOCKFISH_PATH"), STOCKFISH_DEFAULT, shutil.which("stockfish")]
    for candidate in candidates:
        if candidate and Path(candidate).is_file():
            return Path(candidate).resolve()
    raise FileNotFoundError("stockfish binary not found; set STOCKFISH_PATH")


def sha256_path(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


class StockfishScorer:
    def __init__(self, depth=12, workers=8, binary=None, stockfish=None, timeout_s=3600):
        self.depth = depth
        self.workers = workers
        self.binary = find_binary(binary)
        self.stockfish = find_stockfish(stockfish)
        self.timeout_s = timeout_s
        self.engine_name = None

    def _run(self, command, rows):
        payload = "".join(json.dumps(row, separators=(",", ":")) + "\n" for row in rows)
        env = dict(os.environ, STOCKFISH_PATH=str(self.stockfish))
        done = subprocess.run([str(self.binary), *command], input=payload, capture_output=True,
                              text=True, timeout=self.timeout_s, env=env)
        if done.returncode != 0:
            raise RuntimeError(f"{command[0]} failed: {done.stderr.strip()[:500]}")
        out = [json.loads(line) for line in done.stdout.splitlines() if line.strip()]
        if len(out) != len(rows):
            raise RuntimeError(f"{command[0]} returned {len(out)} results for {len(rows)} inputs")
        return out

    def _sharded(self, command, rows):
        if not rows:
            return []
        size = max(1, -(-len(rows) // self.workers))
        shards = [rows[i:i + size] for i in range(0, len(rows), size)]
        with ThreadPoolExecutor(max_workers=len(shards)) as pool:
            parts = list(pool.map(lambda shard: self._run(command, shard), shards))
        merged = [record for part in parts for record in part]
        for record, row in zip(merged, rows, strict=True):
            if record["id"] != row["id"]:
                raise RuntimeError("scorer results out of order")
        if merged:
            self.engine_name = merged[0].get("engine")
        return merged

    def score_moves(self, pairs):
        """pairs: iterable of (fen, move). Returns {(fen, move): record}."""
        unique = sorted(set(pairs))
        rows = [{"id": i, "fen": fen, "move": move} for i, (fen, move) in enumerate(unique)]
        out = self._sharded(["score-moves", "--depth", str(self.depth)], rows)
        return {pair: record for pair, record in zip(unique, out, strict=True)}

    def rank(self, fens, lines=2, depth=None):
        unique = sorted(set(fens))
        rows = [{"id": i, "fen": fen} for i, fen in enumerate(unique)]
        out = self._sharded(["rank-positions", "--depth", str(depth or self.depth), "--lines", str(lines)], rows)
        return {fen: record for fen, record in zip(unique, out, strict=True)}

    def info(self):
        return {
            "engine": self.engine_name, "depth": self.depth, "threads": 1, "hash_mb": 64,
            "fresh_hash_per_search": True, "cpl_clip_cp": 1000,
            "stockfish_path": str(self.stockfish), "stockfish_sha256": sha256_path(self.stockfish),
            "knightlm_bin_sha256": sha256_path(self.binary),
        }
