"""Leela Chess Zero (lc0) and Maia as extra oracles, used only by the evaluator.

Engine-zero (WAR-0005): these run after the model has committed to a move. Nothing
in model/serve, model/prompt.py or a crate reachable from inference imports this
module (model/eval/test_oracles.py scans for it).

Both go through the same Rust driver and UCI client as Stockfish
(`knightlm score-moves` / `rank-positions`, crates/engine), with a node limit
(`--nodes`) instead of a depth and the weights file passed as a UCI option.

  lc0_strength           a second strength oracle: lc0 at a fixed node count, with the
                         centipawn loss of each legal chosen move (Stockfish's axis stays
                         separate; the two engines' centipawns are not comparable).
  maia_human_likeness    Maia (lc0 with a Maia net at one node: the policy head's top
                         move) at a chosen rating; the share of rows where the model's
                         move equals Maia's top move, beside how often the human move
                         does.

Neither binary nor any weights ship here, and nothing is downloaded. When one is
missing the axis is `{"status": "skipped", "reason": ...}` with a named reason.
"""

import os
import shutil
import statistics
from pathlib import Path

from model.eval.engine import StockfishScorer, find_binary, sha256_path
from model.eval.scorecard import percentile, r4, rate

LC0_DEFAULT = Path.home() / ".local/bin/lc0"
MAIA_RATINGS = tuple(range(1100, 2000, 100))
LC0_NODES = 800  # a fixed-node lc0 search; repeatable on one backend, not bit-exact across GPUs
MAIA_NODES = 1  # one node = the policy head's own choice, which is what Maia models
DEFAULT_MAIA_RATING = 1500


class OracleUnavailable(Exception):
    """Raised with a named reason when an oracle cannot run. Never a silent zero."""


def find_lc0(explicit=None):
    candidates = [explicit, os.environ.get("LC0_PATH"), LC0_DEFAULT, shutil.which("lc0")]
    for candidate in candidates:
        if candidate and Path(candidate).is_file():
            return Path(candidate).resolve()
    raise OracleUnavailable("lc0 binary not found (set LC0_PATH or --lc0-path)")


def find_lc0_weights(explicit=None):
    candidate = explicit or os.environ.get("LC0_WEIGHTS")
    if candidate and Path(candidate).is_file():
        return Path(candidate).resolve()
    raise OracleUnavailable("lc0 network weights not found (set LC0_WEIGHTS or --lc0-weights)")


def find_maia_weights(rating, explicit=None, directory=None):
    if rating not in MAIA_RATINGS:
        raise OracleUnavailable(f"no Maia net at rating {rating}; published ratings are {MAIA_RATINGS[0]}..{MAIA_RATINGS[-1]} "
                                f"in steps of 100")
    if explicit:
        if Path(explicit).is_file():
            return Path(explicit).resolve()
        raise OracleUnavailable(f"Maia weights file {explicit} not found")
    root = directory or os.environ.get("MAIA_WEIGHTS_DIR")
    if root:
        for name in (f"maia-{rating}.pb.gz", f"maia-{rating}.pb"):
            if (Path(root) / name).is_file():
                return (Path(root) / name).resolve()
    raise OracleUnavailable(f"Maia {rating} weights not found (set MAIA_WEIGHTS_DIR to a directory holding "
                            f"maia-{rating}.pb.gz, or --maia-weights)")


class Lc0Oracle(StockfishScorer):
    """The Stockfish scorer's Rust path with lc0 as the UCI engine. `depth` carries the node count."""

    def __init__(self, label, lc0, weights, nodes, workers=2, binary=None, timeout_s=3600, backend=None):
        self.label = label
        self.nodes = self.depth = nodes
        self.workers = workers
        self.binary = find_binary(binary)
        self.lc0, self.weights = Path(lc0), Path(weights)
        self.timeout_s = timeout_s
        self.engine_name = None
        # Unset means "whatever this lc0 build picks first" (onnx-cpu on a source build with onnxruntime; a CUDA
        # build would pick a GPU backend and take the shared card). Name one (LC0_BACKEND, e.g. blas) to pin it.
        self.backend = backend or os.environ.get("LC0_BACKEND") or None
        self.stockfish = self.lc0  # `StockfishScorer._run` exports this as STOCKFISH_PATH; unused here

    def extra_args(self):
        args = ["--engine", str(self.lc0), "--nodes", "--threads", "1", "--hash-mb", "0",
                "--option", f"WeightsFile={self.weights}"]
        if self.backend:
            args += ["--option", f"Backend={self.backend}"]
        return args

    def info(self):
        return {
            "oracle": self.label, "engine": self.engine_name, "limit": "nodes", "nodes": self.nodes, "threads": 1,
            "backend": self.backend or "lc0 default",
            "lc0_path": str(self.lc0), "lc0_sha256": sha256_path(self.lc0),
            "weights_path": str(self.weights), "weights_sha256": sha256_path(self.weights),
            "knightlm_bin_sha256": sha256_path(self.binary),
            "note": "lc0 centipawns come from the net's value head; not comparable to Stockfish centipawns",
        }


def skipped(reason):
    return {"status": "skipped", "reason": reason}


def build_lc0(lc0_path=None, weights=None, nodes=LC0_NODES, workers=2):
    return Lc0Oracle("lc0", find_lc0(lc0_path), find_lc0_weights(weights), nodes, workers)


def build_maia(rating=DEFAULT_MAIA_RATING, lc0_path=None, weights=None, weights_dir=None, workers=2):
    return Lc0Oracle(f"maia-{rating}", find_lc0(lc0_path), find_maia_weights(rating, weights, weights_dir),
                     MAIA_NODES, workers)


def cpl_block(entries, n):
    cpls = [e["lc0_cpl"] for e in entries if e.get("lc0_cpl") is not None]
    return {"rows_scored": len(cpls), "rows_unscored": n - len(cpls), "n": n,
            "note": "defined only for legal moves; rows_unscored are failures, not dropped",
            "mean": r4(statistics.fmean(cpls)) if cpls else None,
            "median": percentile(cpls, 50), "p90": percentile(cpls, 90),
            "engine_best_move_rate_of_scored": rate(sum(c == 0 for c in cpls), len(cpls))}


def lc0_axis(records, modes, oracle):
    """Score every legal chosen move with lc0; fills `lc0_cpl` and `lc0_best_move` in place."""
    pairs = {(r["fen"], r[m]["move"]) for r in records for m in modes if r[m]["status"] == "ok"}
    scored = oracle.score_moves(pairs) if pairs else {}
    for record in records:
        for mode in modes:
            entry = record[mode]
            result = scored.get((record["fen"], entry["move"])) if entry["status"] == "ok" else None
            entry["lc0_cpl"] = result["cpl"] if result else None
            entry["lc0_best_move"] = result["best_move"] if result else None
            if result and result["status"] != "scored":
                raise RuntimeError(f"lc0 disagrees with compiler legality for {record['fen']} {entry['move']}")
    n = len(records)
    return {"status": "scored", "engine": oracle.info(),
            **{m: cpl_block([r[m] for r in records], n) for m in modes}}


def maia_axis(records, modes, oracle, rating):
    """Move-match to Maia's top move at `rating`. Every row is in the denominator."""
    ranked = oracle.rank({r["fen"] for r in records}, lines=1, depth=MAIA_NODES)
    n = len(records)
    for record in records:
        found = ranked[record["fen"]]
        if found["status"] != "ranked":
            raise RuntimeError(f"Maia could not rank {record['fen']}: {found['status']}")
        record["maia_move"] = found["best_move"]
        for mode in modes:
            entry = record[mode]
            entry["matches_maia"] = entry["status"] == "ok" and entry["move"] == record["maia_move"]
    target_hits = sum(r["target"] == r["maia_move"] for r in records)
    out = {"status": "scored", "rating": rating, "engine": oracle.info(),
           "reference_human_move_matches_maia": {"matches": target_hits, "n": n, "rate": rate(target_hits, n)}}
    for mode in modes:
        hits = sum(r[mode]["matches_maia"] for r in records)
        out[mode] = {"matches": hits, "n": n, "rate": rate(hits, n)}
    return out
