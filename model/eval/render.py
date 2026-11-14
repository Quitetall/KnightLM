"""Human-readable scorecard. One section per axis; no composite line."""


def fmt(value):
    if value is None:
        return "n/a"
    if isinstance(value, bool):
        return "yes" if value else "no"
    if isinstance(value, float):
        return f"{value:.4f}".rstrip("0").rstrip(".")
    return str(value)


def render_markdown(card):
    axes, modes = card["axes"], card["modes"]
    inputs = card.get("inputs", {})
    lines = [f"# Scorecard: {inputs.get('settings', {}).get('label', '?')}", "",
             f"Rows: {axes['rows']}. Modes: {', '.join(modes)}. **No composite score**: each axis stands alone.",
             "Every legality/match rate uses all rows as denominator; failures are counted, not dropped.", ""]

    def table(title, header, rows):
        lines.extend([f"## {title}", "", "| " + " | ".join(header) + " |", "|" + "---|" * len(header)])
        lines.extend("| " + " | ".join(fmt(c) for c in row) + " |" for row in rows)
        lines.append("")

    if "raw_legality" in axes:
        r = axes["raw_legality"]
        table("Raw legality", ["legal", "n", "rate", "gate", "gate met"],
              [[r["legal"], r["n"], r["rate"], r["gate"]["threshold"], r["gate"]["met"]]])
    if "constrained_legality" in axes:
        c = axes["constrained_legality"]
        table("Constrained legality", ["legal", "n", "rate"], [[c["legal"], c["n"], c["rate"]]])
    table("Played-move match (human move)", ["mode", "matches", "n", "rate"],
          [[m, v["matches"], v["n"], v["rate"]] for m, v in axes["played_move_match"].items()])
    cpl = axes["centipawn_loss"]
    if cpl:
        table(f"Centipawn loss ({cpl['engine']['engine']}, depth {cpl['engine']['depth']}, clip {cpl['engine']['cpl_clip_cp']})",
              ["mode", "scored", "unscored", "n", "mean", "median", "p90", "scored = engine best"],
              [[m, cpl[m]["rows_scored"], cpl[m]["rows_unscored"], cpl[m]["n"], cpl[m]["mean"], cpl[m]["median"],
                cpl[m]["p90"], cpl[m]["engine_best_move_rate_of_scored"]] for m in modes])
    else:
        lines.extend(["## Centipawn loss", "", "Not run (--no-stockfish).", ""])
    tac = axes["tactical"]
    if tac:
        table(f"Tactical set ({tac['set']['rows_in_set']} positions, margin >= {tac['set']['derivation']['min_margin_cp']} cp "
              f"at depth {tac['set']['derivation']['depth']})", ["mode", "n", "correct", "accuracy", "legal", "cpl<=50 rate"],
              [[m, tac[m]["n"], tac[m]["correct"], tac[m]["accuracy"], tac[m]["legal"], tac[m].get("cpl_le_50_rate")]
               for m in modes])
    table("Token cost (tokens reported by the predictor)", ["mode", "in mean", "in p95", "out mean", "out p95", "rows reporting", "n"],
          [[m, v["input"].get("mean"), v["input"].get("p95"), v["output"].get("mean"), v["output"].get("p95"),
            v["rows_reporting"], v["n"]] for m, v in axes["token_cost"].items()])
    table("Latency (ms)", ["mode", "n", "p50", "p90", "p95", "p99", "max"],
          [[m, v.get("n"), v.get("p50"), v.get("p90"), v.get("p95"), v.get("p99"), v.get("max")]
           for m, v in axes["latency_ms"].items()])
    table("Failure accounting", ["mode", "failures", "rate", "illegal", "unparseable", "empty", "exception", "timeout",
                                 "missing", "claim disagrees"],
          [[m, v["total"], v["rate"], *(v["by_kind"][k] for k in ("illegal", "unparseable", "empty", "exception",
                                                                  "timeout", "missing")),
            v["predictor_legal_claim_disagrees"]] for m, v in axes["failures"].items()])
    games = axes.get("games")
    if games:
        lines.extend(["## Games vs fixed Stockfish", "",
                      f"{games['games']} games: {games['wins']}W {games['draws']}D {games['losses']}L, score "
                      f"{fmt(games['score'])}. Protocol: {games['protocol']['opponent']}; colors swapped; "
                      f"{games['openings']} openings.", ""])
        table("Outcome by termination (subject = the model)", ["termination", "win", "draw", "loss"],
              [[t, c.get("win", 0), c.get("draw", 0), c.get("loss", 0)] for t, c in games["by_termination"].items()])
    return "\n".join(lines) + "\n"
