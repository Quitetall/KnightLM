---
format: openwarrant/1.0
profile: ticket
alias: t-e135
uuid: 01a1b88a-dc19-76ed-afc6-d5a5e3086c95
priority: 2
created_at: "2026-11-08T00:24:30Z"
created_by: "codex-contributor-setup"
summary: "Draft a small repeatable evaluation that separates legal move generation from playing strength."
labels:
  - "research"
paths:
  - "docs/model/strength-baseline.md"
---
# Research Contributor — Define a KnightLM strength baseline

**Time and setup.** About 60–90 minutes. Read the
[research plan](../model/plan.md). No training run is needed to draft the
protocol.

Propose a small held-out tactical set and a fixed opponent for color-swapped
games. State where the positions come from, what makes them independent of
training data, and how to handle repetition, move caps, failures, and time.
Write the protocol in `docs/model/strength-baseline.md` with fixed model and
runtime settings. Separate raw model legality, constrained legality, tactical
accuracy, game results, and latency. Include one worked scoring example.

**Review.** A second person should be able to run the proposed comparison
without guessing the settings or deciding how to score failures midway.
Ask for review before anyone runs a large match series.

## Checklist


- [x] Specify the held-out positions, fixed opponent, and reproducible settings. (i-37c0) — done by model-eval-agent, 2026-11-13: Drafted in docs/model/strength-baseline.md: holdout rows, derived tactical set (1324 positions, sha recorded), SF19 depth-1 opponent, fixed openings, settings. Needs second-person review.
- [x] Define the scoring rules and include one worked example. (i-3788) — done by model-eval-agent, 2026-11-13: Scoring rules and worked example (6-row fixture) in strength-baseline.md; reproduced by model.eval.test_eval. Needs second-person review.
