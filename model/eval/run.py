"""python -m model.eval.run --predictor SPEC --label NAME --rows N

Writes eval-<label>-<UTC>/ under the artifacts keep directory: scorecard.json,
scorecard.md, predictions.jsonl, inputs.json (+ games.json when games are played).
"""

import argparse
import json
import platform
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

from model.eval import games as games_module
from model.eval.corpus import load_holdout, sha256_file
from model.eval.engine import REPO, StockfishScorer
from model.eval.predictors import MODES, load_predictor, predictor_manifest
from model.eval.render import render_markdown
from model.eval.scorecard import attach_cpl, build_scorecard, predict_rows
from model.eval.tactics import load_tactics

KEEP_ROOT = Path("/mnt/4tb/artifacts/KnightLM/keep")
DEFAULT_CORPUS = Path("/mnt/4tb/artifacts/KnightLM/expire/corpus/run1-2026-09-a")
DEFAULT_TACTICS = KEEP_ROOT / "eval-tactical-set-run1/tactical-set.json"


def git_state():
    def git(*args):
        return subprocess.run(["git", "-C", str(REPO), *args], capture_output=True, text=True).stdout.strip()
    return {"commit": git("rev-parse", "HEAD"), "dirty": bool(git("status", "--porcelain", "--", "model", "crates"))}


def recount_output_tokens(records, modes, tokenizer_path):
    from tokenizers import Tokenizer
    tokenizer = Tokenizer.from_file(str(tokenizer_path))
    for record in records:
        for mode in modes:
            reply = record[mode]["raw_reply"]
            record[mode]["output_tokens_recount"] = (
                len(tokenizer.encode(reply, add_special_tokens=False).ids) if isinstance(reply, str) else None)


def evaluate(args, predictor, description):
    rows, corpus_info = load_holdout(args.corpus, args.rows)
    modes = tuple(args.modes)
    tactics = load_tactics(args.tactics, rows) if args.tactics else None
    records = predict_rows(rows, predictor, modes, args.timeout_s)
    scorer = None if args.no_stockfish else StockfishScorer(args.depth, args.workers)
    if scorer:
        attach_cpl(records, scorer, modes)
    if args.tokenizer:
        recount_output_tokens(records, modes, args.tokenizer)
    scorecard = build_scorecard(records, modes, scorer.info() if scorer else None, tactics, len(rows))
    if args.tokenizer:
        for mode in modes:
            counted = [(r[mode]["output_tokens"], r[mode]["output_tokens_recount"]) for r in records
                       if r[mode]["output_tokens_recount"] is not None and r[mode]["output_tokens"] is not None]
            scorecard["axes"]["token_cost"][mode]["tokenizer_recount"] = {
                "tokenizer_sha256": sha256_file(args.tokenizer), "rows": len(counted),
                "mismatches_with_reported_output_tokens": sum(a != b for a, b in counted)}
    inputs = {
        "corpus": corpus_info, "predictor": description, "predictor_manifest": predictor_manifest(predictor),
        "stockfish": scorer.info() if scorer else None, "git": git_state(),
        "tactical_set": ({"path": str(args.tactics), "sha256": tactics["path_sha256"]} if tactics else None),
        "settings": {"rows": args.rows, "modes": list(modes), "timeout_s": args.timeout_s,
                     "cpl_depth": args.depth, "label": args.label},
        "python": platform.python_version(),
    }
    return records, scorecard, inputs


def play_games(args, predictor, description):
    server = None
    url = description.get("url")
    if description["kind"] == "python":
        server, url = games_module.serve_predictor(predictor)
    try:
        return games_module.run_games(url, args.games, args.game_mode, args.opponent_depth,
                                      args.max_plies, int(args.timeout_s))
    finally:
        if server:
            server.shutdown()
            server.server_close()


def write_outputs(out, records, scorecard, inputs, games_summary):
    out.mkdir(parents=True)
    with (out / "predictions.jsonl").open("x") as stream:
        for record in records:
            stream.write(json.dumps(record, sort_keys=True) + "\n")
    if games_summary:
        scorecard["axes"]["games"] = games_module.games_axis(games_summary)
        (out / "games.json").write_text(json.dumps(games_summary, indent=1, sort_keys=True) + "\n")
    scorecard["inputs"] = inputs
    scorecard["run"] = {"utc": datetime.now(timezone.utc).isoformat(timespec="seconds"), "dir": str(out)}
    (out / "scorecard.json").write_text(json.dumps(scorecard, indent=1, sort_keys=True) + "\n")
    (out / "inputs.json").write_text(json.dumps(inputs, indent=1, sort_keys=True) + "\n")
    (out / "scorecard.md").write_text(render_markdown(scorecard))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--predictor", required=True, help="module:factory or http://host:port")
    parser.add_argument("--label", required=True)
    parser.add_argument("--rows", type=int, required=True, help="first N holdout rows")
    parser.add_argument("--corpus", type=Path, default=DEFAULT_CORPUS)
    parser.add_argument("--modes", nargs="+", default=list(MODES), choices=MODES)
    parser.add_argument("--timeout-s", type=float, default=60.0, help="per-prediction timeout")
    parser.add_argument("--depth", type=int, default=12, help="Stockfish depth for centipawn loss")
    parser.add_argument("--workers", type=int, default=8)
    parser.add_argument("--no-stockfish", action="store_true")
    parser.add_argument("--tactics", type=Path, default=DEFAULT_TACTICS if DEFAULT_TACTICS.exists() else None)
    parser.add_argument("--no-tactics", action="store_true")
    parser.add_argument("--tokenizer", type=Path, default=None, help="tokenizer.json to recount output tokens")
    parser.add_argument("--games", type=int, default=0, help="even number of games vs fixed Stockfish")
    parser.add_argument("--game-mode", default="raw", choices=MODES)
    parser.add_argument("--opponent-depth", type=int, default=games_module.DEFAULTS["opponent_depth"])
    parser.add_argument("--max-plies", type=int, default=games_module.DEFAULTS["max_plies"])
    parser.add_argument("--out-root", type=Path, default=KEEP_ROOT)
    args = parser.parse_args(argv)
    if args.no_tactics:
        args.tactics = None
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    out = args.out_root / f"eval-{args.label}-{stamp}"
    if out.exists():
        parser.error(f"{out} already exists")
    predictor, description = load_predictor(args.predictor)
    records, scorecard, inputs = evaluate(args, predictor, description)
    games_summary = play_games(args, predictor, description) if args.games else None
    write_outputs(out, records, scorecard, inputs, games_summary)
    print(out)
    return out


if __name__ == "__main__":
    sys.exit(0 if main() else 1)
