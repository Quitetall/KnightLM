"""Human-readable scorecard. One section per axis; no composite line."""


from model.prompt import PLAY_KINDS


def fmt(value):
    if value is None:
        return "n/a"
    if isinstance(value, bool):
        return "yes" if value else "no"
    if isinstance(value, float):
        return f"{value:.4f}".rstrip("0").rstrip(".")
    return str(value)


def render_s0(card):
    axes, inputs = card["axes"], card.get("inputs", {})
    s0 = axes["s0_rules_qa"]
    lines = [f"# S0 rules-QA scorecard: {inputs.get('settings', {}).get('label', '?')}", "",
             f"Prompt kind: **{card['prompt_kind']}**. Rows: {axes['rows']}. **No composite score**: one rate per question type.",
             "Exact match of the answer in the play call's reasoning field; an unparseable reply is a failure, not a dropped row.", "",
             "| type | n | exact | rate | claim failure rate | parse failures |", "|---|---|---|---|---|---|"]
    groups = s0.get("groups", {})
    for name, b in [*s0["by_type"].items(), *((f"group: {g}" + (" (attackers_of, defenders_of)" if g == "edge" else " (other types)"), groups[g])
                                              for g in ("node", "edge") if g in groups), ("all", s0["overall"])]:
        lines.append("| " + " | ".join(fmt(x) for x in (name, b["n"], b["exact"], b["rate"], b["claim_failure_rate"],
                                                         b["parse_failures"])) + " |")
    lines.append("")
    return "\n".join(lines)


def render_markdown(card):
    if "s0_rules_qa" in card["axes"]:
        return render_s0(card)
    axes, modes = card["axes"], card["modes"]
    inputs = card.get("inputs", {})
    kind = card.get("prompt_kind", "move")
    lines = [f"# Scorecard: {inputs.get('settings', {}).get('label', '?')}", "",
             f"Prompt kind: **{kind}**" + (" (play prompt and submit_move tool call; not comparable with a move-only card)"
                                           if kind in PLAY_KINDS else " (move-only prompt)") + ".",
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
    if "play_raw_legality" in axes:
        r = axes["play_raw_legality"]
        table("Play raw legality (completion parses, names a legal move, agrees with its reasoning)",
              ["legal", "n", "rate", "gate", "gate met"],
              [[r["legal"], r["n"], r["rate"], r["gate"]["threshold"], r["gate"]["met"]]])
    if "play_constrained_legality" in axes:
        c = axes["play_constrained_legality"]
        table("Play constrained legality", ["legal", "n", "rate"], [[c["legal"], c["n"], c["rate"]]])
    if "play_completion" in axes:
        pc = axes["play_completion"]
        table("Play completions (valid = passes every check; model valid = before any constrained repair)",
              ["mode", "valid", "model valid", "parsed move legal", "n", "path model", "path constrained_move", "path forced reasoning", "path none"],
              [[m, v["completion_valid"]["valid"], v["model_completion_valid"]["valid"], v["parsed_move_legal"]["legal"],
                v["completion_valid"]["n"], v["by_path"]["model"], v["by_path"]["constrained_move"], v["by_path"]["constrained_forced_reasoning"],
                v["by_path"]["none"]]
               for m, v in pc.items()])
        codes = sorted({c for v in pc.values() for c, k in v["by_code"].items() if k and c != "ok"})
        if codes:
            table("Play completion refusal codes", ["mode", *codes],
                  [[m, *(v["by_code"][c] for c in codes)] for m, v in pc.items()])
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
    kinds = ("illegal", "unparseable", "empty", "exception", "timeout", "missing") + (
        ("inconsistent",) if kind in PLAY_KINDS else ())
    table("Failure accounting", ["mode", "failures", "rate", *kinds, "claim disagrees"],
          [[m, v["total"], v["rate"], *(v["by_kind"][k] for k in kinds),
            v["predictor_legal_claim_disagrees"]] for m, v in axes["failures"].items()])
    lc0 = axes.get("lc0_strength")
    if lc0 and lc0.get("status") == "skipped":
        lines.extend(["## lc0 strength oracle", "", f"Skipped: {lc0['reason']}", ""])
    elif lc0:
        eng = lc0["engine"]
        table(f"lc0 centipawn loss ({eng['engine']}, {eng['nodes']} nodes, weights {eng['weights_sha256'][:12]}); "
              "not comparable to Stockfish centipawns",
              ["mode", "scored", "unscored", "n", "mean", "median", "p90", "scored = lc0 best"],
              [[m, lc0[m]["rows_scored"], lc0[m]["rows_unscored"], lc0[m]["n"], lc0[m]["mean"], lc0[m]["median"],
                lc0[m]["p90"], lc0[m]["engine_best_move_rate_of_scored"]] for m in modes])
    maia = axes.get("maia_human_likeness")
    if maia and maia.get("status") == "skipped":
        lines.extend(["## Maia human-likeness", "", f"Skipped: {maia['reason']}", ""])
    elif maia:
        ref = maia["reference_human_move_matches_maia"]
        table(f"Move match to Maia {maia['rating']} top move (reference: the human move matches it in "
              f"{ref['matches']}/{ref['n']} rows)", ["mode", "matches", "n", "rate"],
              [[m, maia[m]["matches"], maia[m]["n"], maia[m]["rate"]] for m in modes])
    expl = axes.get("explanation")
    if expl and expl.get("status") == "skipped":
        lines.extend(["## Explanation claims", "", f"Skipped: {expl['reason']}", ""])
    elif expl:
        lines.extend(["## Explanation claims checked against the board", "",
                      f"{expl['n']} explanations ({expl['explanations_missing']} missing, "
                      f"{expl['explanations_with_a_failed_claim']} with a failed claim, "
                      f"{expl['explanations_without_checkable_claims']} with no checkable claim). "
                      "Counts per claim type; no composite.", ""])
        table("Claims by type", ["type", "checkable", "failed"],
              [[k, v["checkable"], v["failed"]] for k, v in expl["claim_types"].items()])
        table("Seen but not checkable", ["reason", "count"], list(expl["unchecked_claims"].items()))
    pex = axes.get("play_explanation")
    if pex:
        for m, expl in pex.items():
            lines.extend([f"## Play reasoning checked against the board ({m})", "",
                          f"{expl['n']} completions ({expl['explanations_missing']} with no reasoning, "
                          f"{expl['explanations_with_a_failed_claim']} with a failed claim, "
                          f"{expl['explanations_without_checkable_claims']} with no checkable claim). "
                          "Counts per claim type; no composite.", ""])
            table(f"Claims by type ({m})", ["type", "checkable", "failed"],
                  [[k, v["checkable"], v["failed"]] for k, v in expl["claim_types"].items()])
            table(f"Seen but not checkable ({m})", ["reason", "count"], list(expl["unchecked_claims"].items()))
    games = axes.get("games")
    if games:
        lines.extend(["## Games vs fixed Stockfish", "",
                      f"{games['games']} games: {games['wins']}W {games['draws']}D {games['losses']}L, score "
                      f"{fmt(games['score'])}. Protocol: {games['protocol']['opponent']}; colors swapped; "
                      f"{games['openings']} openings.", ""])
        table("Outcome by termination (subject = the model)", ["termination", "win", "draw", "loss"],
              [[t, c.get("win", 0), c.get("draw", 0), c.get("loss", 0)] for t, c in games["by_termination"].items()])
    return "\n".join(lines) + "\n"
