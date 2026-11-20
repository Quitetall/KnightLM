"""Per-axis scorecard over holdout predictions. There is no composite score.

Every axis reports its own denominator. The denominator of the legality and
match axes is every requested row: a missing, timed-out, empty, unparseable or
illegal reply is a failure on that row, never a dropped one. Centipawn loss is
only defined for legal moves, so its block reports how many rows it covers
beside the legality that gates it.
"""

import math
import re
import statistics
import threading
import time

from model.eval import SCORECARD_SCHEMA
from model.eval.predictors import MODES

UCI_MOVE = re.compile(r"^[a-h][1-8][a-h][1-8][qrbn]?$")
RAW_LEGALITY_GATE = 0.90  # KLM-WAR-0008: reported threshold, not a filter
FAILURE_KINDS = ("illegal", "unparseable", "empty", "exception", "timeout", "missing")
PLAY_FAILURE_KINDS = FAILURE_KINDS + ("inconsistent",)  # legal move, but not the one the reasoning states
PROMPT_KINDS = ("move", "play")
PLAY_EXTRAS = ("reasoning", "play_code", "play_path", "play_raw_code", "stated_move", "parsed_move_legal")


def call_with_timeout(predictor, packet, mode, timeout_s):
    """Return (prediction, error_kind, error_text, wall_ms). Never raises."""
    box = {}

    def work():
        try:
            box["value"] = predictor.predict(packet, mode)
        except BaseException as error:  # noqa: BLE001 - a predictor crash is a scored failure
            box["error"] = f"{type(error).__name__}: {error}"

    started = time.monotonic()
    thread = threading.Thread(target=work, daemon=True)
    thread.start()
    thread.join(timeout_s)
    wall = (time.monotonic() - started) * 1000
    if thread.is_alive():
        return None, "timeout", f"no reply within {timeout_s}s", wall
    if "error" in box:
        return None, "exception", box["error"], wall
    return box["value"], None, None, wall


def classify(move, legal_moves):
    if move is None or move == "":
        return "empty"
    if not isinstance(move, str) or not UCI_MOVE.match(move):
        return "unparseable"
    return None if move in legal_moves else "illegal"


def classify_play(move, code, legal_moves):
    """Status of a play-mode answer: the move gate, then the completion code the serve layer reported."""
    if code is None:
        return classify(move, legal_moves)
    if code == "ok":
        return classify(move, legal_moves)
    if code == "illegal_move":
        return "illegal"
    if code == "stated_move_mismatch":
        return "inconsistent"
    return "unparseable"


def predict_rows(rows, predictor, modes=MODES, timeout_s=60.0, prompt_kind="move"):
    """Run every row through every mode. Returns one record per row.

    prompt_kind "play" adds the play-mode fields (reasoning, completion code, path) to each
    entry and classifies by the predictor's completion code; "move" records are unchanged."""
    play = prompt_kind == "play"
    if prompt_kind not in PROMPT_KINDS:
        raise ValueError(f"prompt_kind must be one of {PROMPT_KINDS}")
    records = []
    for index, row in enumerate(rows):
        packet = row["packet"]
        record = {"idx": index, "game_id": row["game_id"], "position_key": row["position_key"],
                  "fen": packet["fen"], "target": row["target"], "legal_count": len(packet["legal_moves"])}
        for mode in modes:
            value, kind, text, wall = call_with_timeout(predictor, packet, mode, timeout_s)
            entry = {"move": None, "raw_reply": None, "claimed_legal": None, "input_tokens": None,
                     "output_tokens": None, "latency_ms": None, "error": text}
            if value is not None:
                move = getattr(value, "move", None)
                latency = getattr(value, "latency_ms", None)
                entry.update(
                    move=move, raw_reply=getattr(value, "raw_reply", None),
                    claimed_legal=getattr(value, "legal", None),
                    input_tokens=getattr(value, "input_tokens", None),
                    output_tokens=getattr(value, "output_tokens", None),
                    latency_ms=latency if latency is not None else round(wall, 3),
                )
                if play:
                    entry.update(reasoning=getattr(value, "reasoning", None), play_code=getattr(value, "code", None),
                                 play_path=getattr(value, "path", None), play_raw_code=getattr(value, "raw_code", None),
                                 stated_move=getattr(value, "stated_move", None))
                    entry["parsed_move_legal"] = isinstance(move, str) and move in packet["legal_moves"]
                    kind = classify_play(move, entry["play_code"], packet["legal_moves"])
                else:
                    kind = classify(move, packet["legal_moves"])
            elif play:
                entry.update(dict.fromkeys(PLAY_EXTRAS))
            entry["status"] = kind or "ok"
            entry["matches_target"] = entry["status"] == "ok" and entry["move"] == row["target"]
            record[mode] = entry
        records.append(record)
    return records


def percentile(values, p):
    """Nearest-rank percentile; None when there are no values."""
    if not values:
        return None
    ordered = sorted(values)
    return ordered[max(0, math.ceil(p / 100 * len(ordered)) - 1)]


def r4(value):
    return None if value is None else round(value, 4)


def rate(count, total):
    return r4(count / total) if total else None


def dist(values, points=(50, 90, 95, 99)):
    if not values:
        return {"n": 0}
    out = {"n": len(values), "mean": r4(statistics.fmean(values)), "max": max(values)}
    out.update({f"p{p}": percentile(values, p) for p in points})
    return out


def attach_cpl(records, scorer, modes):
    """Score every legal chosen move with Stockfish; fill cpl fields in place."""
    pairs = {(r["fen"], r[m]["move"]) for r in records for m in modes if r[m]["status"] == "ok"}
    scored = scorer.score_moves(pairs) if pairs else {}
    for record in records:
        for mode in modes:
            entry = record[mode]
            result = scored.get((record["fen"], entry["move"])) if entry["status"] == "ok" else None
            entry["cpl"] = result["cpl"] if result else None
            entry["best_move"] = result["best_move"] if result else None
            entry["engine_status"] = result["status"] if result else None
            if result and result["status"] != "scored":
                raise RuntimeError(f"engine disagrees with compiler legality for {record['fen']} {entry['move']}")


def mode_block(records, mode, scored, kinds_list=FAILURE_KINDS):
    n = len(records)
    entries = [r[mode] for r in records]
    kinds = {k: sum(e["status"] == k for e in entries) for k in kinds_list}
    legal = sum(e["status"] == "ok" for e in entries)
    out = {
        "legality": {"legal": legal, "n": n, "rate": rate(legal, n)},
        "played_move_match": {"matches": sum(e["matches_target"] for e in entries), "n": n,
                              "rate": rate(sum(e["matches_target"] for e in entries), n)},
        "failures": {"total": n - legal, "rate": rate(n - legal, n), "by_kind": kinds,
                     "predictor_legal_claim_disagrees": sum(
                         e["claimed_legal"] is not None and bool(e["claimed_legal"]) != (e["status"] == "ok")
                         for e in entries)},
        "token_cost": {
            "input": dist([e["input_tokens"] for e in entries if e["input_tokens"] is not None]),
            "output": dist([e["output_tokens"] for e in entries if e["output_tokens"] is not None]),
            "rows_reporting": sum(e["input_tokens"] is not None for e in entries), "n": n,
        },
        "latency_ms": dist([e["latency_ms"] for e in entries if e["latency_ms"] is not None]),
    }
    if scored:
        cpls = [e["cpl"] for e in entries if e["cpl"] is not None]
        out["centipawn_loss"] = {
            "rows_scored": len(cpls), "rows_unscored": n - len(cpls), "n": n,
            "note": "defined only for legal moves; read with legality, rows_unscored are failures",
            "mean": r4(statistics.fmean(cpls)) if cpls else None,
            "median": percentile(cpls, 50), "p90": percentile(cpls, 90),
            "engine_best_move_rate_of_scored": rate(sum(c == 0 for c in cpls), len(cpls)),
        }
    return out


def tactical_block(records, mode, tactics, scored):
    """Accuracy on the tactical subset; every tactical row is in the denominator."""
    subset = [r for r in records if r["idx"] in tactics["by_idx"]]
    n = len(subset)
    correct = sum(r[mode]["status"] == "ok" and r[mode]["move"] == tactics["by_idx"][r["idx"]]["best_move"]
                  for r in subset)
    out = {"n": n, "correct": correct, "accuracy": rate(correct, n),
           "legal": sum(r[mode]["status"] == "ok" for r in subset)}
    if scored and n:
        near = sum(r[mode]["cpl"] is not None and r[mode]["cpl"] <= 50 for r in subset)
        out["cpl_le_50"] = near
        out["cpl_le_50_rate"] = rate(near, n)
    return out


def play_completion_block(records, mode):
    """Play-mode completion accounting: how the model's own text fared, by stable code and by path."""
    from model.serve.play import PATHS, PLAY_CODES
    n = len(records)
    entries = [r[mode] for r in records]
    by_code = {c: 0 for c in PLAY_CODES}
    by_code["no_reply"] = 0  # timeout or exception: nothing came back to check
    for e in entries:
        by_code[e["play_code"] if e.get("play_code") in by_code else "no_reply"] += 1
    paths = {p: sum(e.get("play_path") == p for e in entries) for p in PATHS}
    raw_ok = sum(e.get("play_raw_code") == "ok" for e in entries)
    parsed_legal = sum(bool(e.get("parsed_move_legal")) for e in entries)
    return {"completion_valid": {"valid": by_code["ok"], "n": n, "rate": rate(by_code["ok"], n)},
            "model_completion_valid": {"valid": raw_ok, "n": n, "rate": rate(raw_ok, n),
                                       "note": "the model's own text passed every check, before any constrained repair"},
            "parsed_move_legal": {"legal": parsed_legal, "n": n, "rate": rate(parsed_legal, n)},
            "by_code": by_code, "by_path": paths}


def build_scorecard(records, modes, scorer_info=None, tactics=None, requested_rows=None, prompt_kind="move"):
    n = len(records)
    requested = requested_rows if requested_rows is not None else n
    if n < requested:  # rows never attempted are failures, not omissions
        raise ValueError(f"{requested - n} requested rows have no prediction record")
    scored = scorer_info is not None
    play = prompt_kind == "play"
    per_mode = {m: mode_block(records, m, scored, PLAY_FAILURE_KINDS if play else FAILURE_KINDS) for m in modes}
    axes = {"rows": n}
    if play:
        # Own axis names: a play scorecard is never silently the same thing as a move-only one.
        if "raw" in per_mode:
            raw = per_mode["raw"]["legality"]
            axes["play_raw_legality"] = dict(raw, gate={"threshold": RAW_LEGALITY_GATE,
                                                        "met": raw["rate"] is not None and raw["rate"] >= RAW_LEGALITY_GATE,
                                                        "note": "reported threshold; the completion must parse, name a "
                                                                "legal move and agree with its reasoning"})
        if "constrained" in per_mode:
            axes["play_constrained_legality"] = per_mode["constrained"]["legality"]
        axes["play_completion"] = {m: play_completion_block(records, m) for m in modes}
    elif "raw" in per_mode:
        raw = per_mode["raw"]["legality"]
        axes["raw_legality"] = dict(raw, gate={"threshold": RAW_LEGALITY_GATE,
                                               "met": raw["rate"] is not None and raw["rate"] >= RAW_LEGALITY_GATE,
                                               "note": "reported threshold; every row stays in the denominator"})
    if "constrained" in per_mode and not play:
        axes["constrained_legality"] = per_mode["constrained"]["legality"]
    axes["played_move_match"] = {m: per_mode[m]["played_move_match"] for m in modes}
    axes["centipawn_loss"] = ({m: per_mode[m]["centipawn_loss"] for m in modes} | {"engine": scorer_info}
                              if scored else None)
    axes["tactical"] = ({"set": {k: tactics[k] for k in ("path_sha256", "rows_in_set", "derivation")},
                         **{m: tactical_block(records, m, tactics, scored) for m in modes}}
                        if tactics else None)
    axes["token_cost"] = {m: per_mode[m]["token_cost"] for m in modes}
    axes["latency_ms"] = {m: per_mode[m]["latency_ms"] for m in modes}
    axes["failures"] = {m: per_mode[m]["failures"] for m in modes}
    axes["games"] = None
    return {"schema": SCORECARD_SCHEMA, "composite_score": None,
            "composite_note": "none by design: axes are reported separately (t-c199, KLM-WAR-0008 OBL-002)",
            "prompt_kind": prompt_kind, "modes": list(modes), "axes": axes}
