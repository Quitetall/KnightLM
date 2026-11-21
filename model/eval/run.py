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

from model.eval import explain as explain_module
from model.eval import games as games_module
from model.eval import playmode
from model.eval import oracles
from model.eval.corpus import load_holdout, sha256_file
from model.eval.engine import REPO, StockfishScorer
from model.eval.predictors import MODES, load_predictor, predictor_manifest
from model.eval.render import render_markdown
from model.eval.scorecard import PLAY_KINDS, PROMPT_KINDS, attach_cpl, build_scorecard, call_with_timeout, predict_rows
from model.eval.tactics import load_tactics

from model.corpus.s0 import FORMATS as S0_FORMATS

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


def add_oracle_axes(args, records, modes):
    """lc0 and Maia axes. An oracle that is not installed is a named skip, never a zero."""
    axes, info = {}, {}
    wanted = {"lc0": "lc0_strength", "maia": "maia_human_likeness"}
    for name in args.oracles:
        axis = wanted[name]
        try:
            if name == "lc0":
                oracle = oracles.build_lc0(args.lc0_path, args.lc0_weights, args.lc0_nodes, args.workers)
                axes[axis] = oracles.lc0_axis(records, modes, oracle)
            else:
                oracle = oracles.build_maia(args.maia_rating, args.lc0_path, args.maia_weights,
                                            args.maia_weights_dir, args.workers)
                axes[axis] = oracles.maia_axis(records, modes, oracle, args.maia_rating)
            info[name] = axes[axis]["engine"]
        except oracles.OracleUnavailable as error:
            axes[axis] = oracles.skipped(str(error))
            info[name] = {"skipped": str(error)}
            print(f"oracle {name} skipped: {error}", file=sys.stderr)
    return axes, info


class _ExplainAdapter:
    def __init__(self, predictor):
        self.predictor = predictor

    def predict(self, packet, mode):
        return self.predictor.explain(packet)


def collect_explanations(args, rows, records, predictor):
    """One explanation per row from the predictor's optional explain(packet) or an --explanations file."""
    supplied = None
    if args.explanations:
        supplied = {}
        for line in args.explanations.read_text().splitlines():
            if line.strip():
                item = json.loads(line)
                supplied[item["fen"]] = item["text"]
    elif not callable(getattr(predictor, "explain", None)):
        return oracles.skipped("predictor has no explain(packet) method and no --explanations file was given")
    per_row, source = [], ("file" if supplied is not None else "predictor.explain")
    for row, record in zip(rows, records, strict=True):
        packet = row["packet"]
        if supplied is not None:
            text, error = supplied.get(packet["fen"]), None
            error = None if text is not None else "no explanation supplied for this position"
        else:
            text, kind, error, _ = call_with_timeout(_ExplainAdapter(predictor), packet, None, args.timeout_s)
            error = None if kind is None else f"{kind}: {error}"
        raw = record.get("raw", {})
        move = raw.get("move") if raw.get("status") == "ok" else None
        result = None if not isinstance(text, str) else explain_module.check_explanation(
            packet["fen"], packet["legal_moves"], text, move)
        record["explanation"] = {"text": text, "error": error, "check": result}
        per_row.append(result)
    axis = explain_module.aggregate(per_row)
    axis["source"] = source
    return axis


def play_settings(play):
    """What fixes the play prompt and decode, so compare can refuse a mismatch. Empty for move-only."""
    if not play:
        return {}
    import hashlib
    from model import prompt as prompt_module
    from model.serve.inference import PLAY_MAX_NEW_TOKENS
    return {"play_prompt_sha256": hashlib.sha256(prompt_module.prefix("play").encode()).hexdigest(),
            "play_max_new_tokens": PLAY_MAX_NEW_TOKENS}


def cgt_settings(kind):
    """The CGT renderer an arm reads: compiler binary hash and variant. Empty for the FEN packet and move kinds."""
    from model import prompt as prompt_module
    if kind in prompt_module.CTOK_KINDS:
        from model.tokenizer import vocab
        out = {"ctok_spec": vocab.SPEC_VERSION, "ctok_vocab_sha256": vocab.vocab_sha256()}
        if kind != "play-t1":
            import hashlib
            out["compiler_sha256"] = hashlib.sha256(prompt_module.knightlm_binary().read_bytes()).hexdigest()
        return out
    if kind not in prompt_module.CGT_VARIANT:
        return {}
    import hashlib
    return {"cgt_variant": prompt_module.CGT_VARIANT[kind], "cgt_label": prompt_module.CGT_LABEL,
            "compiler_sha256": hashlib.sha256(prompt_module.knightlm_binary().read_bytes()).hexdigest()}


def evaluate_s0(args, rows, corpus_info, predictor, description):
    """Score an S0 rules-QA holdout: exact answer per question type, no moves, no engine."""
    from model.eval import s0 as s0_module
    kind = corpus_info["prompt_kind"]
    records = s0_module.predict_rows(rows, getattr(predictor, "inner", predictor), kind, args.timeout_s)
    scorecard = s0_module.scorecard(records, kind, len(rows))
    inputs = {"corpus": corpus_info, "predictor": description, "predictor_manifest": predictor_manifest(predictor),
              "git": git_state(), "stockfish": None, "oracles": None, "tactical_set": None,
              "settings": {"rows": args.rows, "modes": ["raw"], "timeout_s": args.timeout_s, "label": args.label,
                           "prompt_kind": kind, "s0": True, **cgt_settings(kind)},
              "python": platform.python_version()}
    return records, scorecard, inputs


def evaluate(args, predictor, description):
    rows, corpus_info = load_holdout(args.corpus, args.rows)
    if corpus_info["corpus_format"] in S0_FORMATS:
        return evaluate_s0(args, rows, corpus_info, predictor, description)
    modes = tuple(args.modes)
    tactics = load_tactics(args.tactics, rows) if args.tactics else None
    play = args.prompt_kind in PLAY_KINDS
    records = predict_rows(rows, predictor, modes, args.timeout_s, args.prompt_kind)
    scorer = None if args.no_stockfish else StockfishScorer(args.depth, args.workers)
    if scorer:
        attach_cpl(records, scorer, modes)
    if args.tokenizer:
        recount_output_tokens(records, modes, args.tokenizer)
    extra_axes, oracle_info = add_oracle_axes(args, records, modes)
    if play:  # the reasoning the model wrote is the explanation; no second call
        extra_axes["play_explanation"] = playmode.explanation_axis(rows, records, modes)
    elif args.explain or args.explanations:
        extra_axes["explanation"] = collect_explanations(args, rows, records, predictor)
    scorecard = build_scorecard(records, modes, scorer.info() if scorer else None, tactics, len(rows),
                                args.prompt_kind)
    scorecard["axes"].update(extra_axes)
    if args.tokenizer:
        for mode in modes:
            counted = [(r[mode]["output_tokens"], r[mode]["output_tokens_recount"]) for r in records
                       if r[mode]["output_tokens_recount"] is not None and r[mode]["output_tokens"] is not None]
            scorecard["axes"]["token_cost"][mode]["tokenizer_recount"] = {
                "tokenizer_sha256": sha256_file(args.tokenizer), "rows": len(counted),
                "mismatches_with_reported_output_tokens": sum(a != b for a, b in counted)}
    inputs = {
        "corpus": corpus_info, "predictor": description, "predictor_manifest": predictor_manifest(predictor),
        "stockfish": scorer.info() if scorer else None, "oracles": oracle_info or None, "git": git_state(),
        "tactical_set": ({"path": str(args.tactics), "sha256": tactics["path_sha256"]} if tactics else None),
        "settings": {"rows": args.rows, "modes": list(modes), "timeout_s": args.timeout_s,
                     "cpl_depth": args.depth, "label": args.label,
                     "oracles": list(args.oracles), "maia_rating": args.maia_rating if "maia" in args.oracles else None,
                     "lc0_nodes": args.lc0_nodes if "lc0" in args.oracles else None,
                     "explain": bool(args.explain or args.explanations),
                     "prompt_kind": args.prompt_kind, **play_settings(play), **cgt_settings(args.prompt_kind)},
        "python": platform.python_version(),
    }
    return records, scorecard, inputs


def play_games(args, predictor, description):
    server = None
    # An http predictor given as host:port has no path; the match driver needs
    # the full move endpoint.
    url = getattr(predictor, "move_url", None) or description.get("url")
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
    parser.add_argument("--oracles", nargs="*", default=[], choices=["lc0", "maia"],
                        help="extra oracle axes after the model commits; a missing binary or net is a named skip")
    parser.add_argument("--lc0-path", type=Path, default=None, help="default: LC0_PATH, ~/.local/bin/lc0, PATH")
    parser.add_argument("--lc0-weights", type=Path, default=None, help="default: LC0_WEIGHTS")
    parser.add_argument("--lc0-nodes", type=int, default=oracles.LC0_NODES)
    parser.add_argument("--maia-rating", type=int, default=oracles.DEFAULT_MAIA_RATING)
    parser.add_argument("--maia-weights", type=Path, default=None, help="default: MAIA_WEIGHTS_DIR/maia-RATING.pb.gz")
    parser.add_argument("--maia-weights-dir", type=Path, default=None)
    parser.add_argument("--explain", action="store_true",
                        help="score explanations from the predictor's explain(packet)")
    parser.add_argument("--explanations", type=Path, default=None,
                        help="JSONL of {fen, text} explanations to score instead of calling the predictor")
    parser.add_argument("--prompt-kind", default="move", choices=PROMPT_KINDS,
                        help="move: the move-only prompt. play: the play-mode prompt and tool-call completion "
                             "(the reasoning corpus format); scores the reasoning too. play-cgt / play-cgt-full: "
                             "the same with the position as Chess Graph Text (arm T2); needs a predictor "
                             "trained on that prompt. An S0 rules-QA corpus sets its own kind")
    parser.add_argument("--out-root", type=Path, default=KEEP_ROOT)
    args = parser.parse_args(argv)
    if args.no_tactics:
        args.tactics = None
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    out = args.out_root / f"eval-{args.label}-{stamp}"
    if out.exists():
        parser.error(f"{out} already exists")
    manifest = json.loads((args.corpus / "manifest.json").read_text())
    if manifest["format"] in S0_FORMATS:  # the corpus fixes the prompt kind; a conflicting flag is an error
        if args.prompt_kind not in ("move", manifest["prompt_kind"]):
            parser.error(f"--prompt-kind {args.prompt_kind} conflicts with the S0 corpus kind {manifest['prompt_kind']}")
        args.prompt_kind = manifest["prompt_kind"]
        args.modes = ["raw"]
    predictor, description = load_predictor(args.predictor, args.prompt_kind)
    records, scorecard, inputs = evaluate(args, predictor, description)
    games_summary = play_games(args, predictor, description) if args.games else None
    write_outputs(out, records, scorecard, inputs, games_summary)
    print(out)
    return out


if __name__ == "__main__":
    sys.exit(0 if main() else 1)
