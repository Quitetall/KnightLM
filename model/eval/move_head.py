"""Move-head axis (arm label: human-only move head). No composite score, no engine at inference.

A predictor has `logits(packet) -> sequence of 1968 floats` (raw, unmasked, in model.policy_head.space
order). Per row the axis reports:

  masked  the product: argmax over the packet's legal_moves. top-1 / top-3 match with the human move
          (the row's `target`), masked NLL, calibration (ECE over top-1 confidence), centipawn loss
          of the masked top-1 through the existing Stockfish scoring path (post-decision only).
  raw     the diagnostic: argmax over all 1968 moves. Its legality rate, its match with the human
          move, and the probability mass the unmasked softmax puts on illegal moves.

A row whose predictor raised, or returned the wrong length, is a failure and stays in every
denominator. CPL is defined only for legal moves; unscored rows are reported beside it.
"""

import statistics

from model.eval.scorecard import attach_cpl, call_with_timeout, percentile, r4, rate
from model.policy_head import metrics, space

MODES = ("masked", "raw")
SCHEMA = "knightlm.scorecard/move_head"
LABEL = "human-only move head"


class _Adapter:
    def __init__(self, predictor):
        self.predictor = predictor

    def predict(self, packet, _mode):
        return list(self.predictor.logits(packet))


def predict_rows(rows, predictor, timeout_s=60.0):
    if not callable(getattr(predictor, "logits", None)):
        raise ValueError("a move_head evaluation needs a predictor with logits(packet)")
    records = []
    adapter = _Adapter(predictor)
    for index, row in enumerate(rows):
        packet, target = row["packet"], row["target"]
        legal_idx = space.indices(packet["legal_moves"])
        record = {"idx": index, "game_id": row.get("game_id"), "fen": packet["fen"], "target": target,
                  "legal_count": len(legal_idx), "error": None, "stats": None}
        empty = {"move": None, "status": "missing", "matches_target": False}
        record["masked"], record["raw"] = dict(empty), dict(empty)
        value, kind, text, _wall = call_with_timeout(adapter, packet, None, timeout_s)
        if value is not None and len(value) != space.SIZE:
            value, text = None, f"logits must have {space.SIZE} entries, got {len(value)}"
        if value is None:
            record["error"] = text or kind
        else:
            stats = metrics.row_stats(value, legal_idx, space.INDEX[target])
            record["stats"] = stats
            top, raw = space.UCI[stats["masked_top1"]], space.UCI[stats["raw_top1"]]
            record["masked"] = {"move": top, "status": "ok", "matches_target": top == target}
            record["raw"] = {"move": raw, "status": "ok" if stats["raw_top1_legal"] else "illegal",
                             "matches_target": raw == target}
        records.append(record)
    return records


def cpl_block(records, mode):
    cpls = [r[mode]["cpl"] for r in records if r[mode].get("cpl") is not None]
    n = len(records)
    return {"rows_scored": len(cpls), "rows_unscored": n - len(cpls), "n": n,
            "note": "defined only for legal moves; raw-mode rows with an illegal argmax are unscored",
            "mean": r4(statistics.fmean(cpls)) if cpls else None,
            "median": percentile(cpls, 50), "p90": percentile(cpls, 90)}


def scorecard(records, n_rows, scorer_info=None):
    if len(records) < n_rows:
        raise ValueError(f"{n_rows - len(records)} requested rows have no record")
    n = len(records)
    stats = [r["stats"] for r in records if r["stats"] is not None]
    agg = metrics.aggregate(stats)
    ok = len(stats)

    def over_n(count):  # every requested row is in the denominator
        return {"count": count, "n": n, "rate": rate(count, n)}

    axis = {
        "label": LABEL, "rows": n, "rows_with_logits": ok,
        "masked": {"top1_match": over_n(sum(s["masked_top1_hit"] for s in stats)),
                   "top3_match": over_n(sum(s["masked_top3_hit"] for s in stats)),
                   "nll_mean_over_scored_rows": agg.get("masked_nll"),
                   "calibration": agg.get("calibration"), "legality": over_n(ok)},
        "raw": {"argmax_legal": over_n(sum(s["raw_top1_legal"] for s in stats)),
                "top1_match": over_n(sum(s["raw_top1_hit"] for s in stats)),
                "illegal_mass_mean": agg.get("raw_illegal_mass_mean"),
                "note": "diagnostic only: the product is the masked distribution"},
        "centipawn_loss": None,
    }
    if scorer_info is not None:
        axis["centipawn_loss"] = {m: cpl_block(records, m) for m in MODES} | {"engine": scorer_info}
    return {"schema": SCHEMA, "composite_score": None,
            "composite_note": "none by design: the move_head axis is reported per measure",
            "arm": LABEL, "axes": {"move_head": axis}}


def evaluate(rows, predictor, scorer=None, timeout_s=60.0):
    """Run the rows, optionally score CPL with a StockfishScorer (or any object with score_moves/info)."""
    records = predict_rows(rows, predictor, timeout_s)
    if scorer is not None:
        attach_cpl(records, scorer, MODES)
    return records, scorecard(records, len(rows), scorer.info() if scorer is not None else None)
