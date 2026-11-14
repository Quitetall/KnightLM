---
format: openwarrant/1.0
profile: ticket
alias: t-c199
uuid: 01a1b895-f42f-752e-bc70-d053d6f49f5a
priority: 3
created_at: "2026-11-08T00:36:37Z"
created_by: "codex-contributor-setup"
summary: "Score baseline and trained model predictions under the same rules and retain the evidence for the project lead."
labels:
  - "representation-evaluation"
paths:
  - "model/eval/**"
x.depends_on:
  - "t-e135"
  - "t-5cd8"
requires:
  - "x.depends_on"
---
# Representation/Evaluation Engineer — Implement the held-out scorecard

**Start after** the [strength protocol](t-e135.md) and
[corpus contract](t-5cd8.md). First implement the worked scoring example
as a tiny fixture so review can happen before a large evaluation.

Build a repeatable scorecard over an identified holdout manifest and fixed
model predictions. Separate raw legality, constrained legality, tactical
accuracy, game outcomes by termination, and latency. Include denominators,
missing predictions and failures. Keep eval inputs and outputs separate from
training data. When using a fixed opponent, record the openings and color
swap. Record model, corpus and evaluation settings so the pilot and Run 1 can
be compared fairly.

**Review.** A second person can reproduce one fixture score and see failures
in the report. Once predictions exist, record a pilot scorecard with links
to its inputs. Hardware access is needed for inference only.

## Checklist


- [x] Implement the worked fixture and retain failure counts in the scorecard. (i-1756) — done by model-eval-agent, 2026-11-13: model/eval scorecard + worked fixture (model.eval.fakes:worked_example); per-status failure counts in the scorecard; planted illegal/garbled/timeout/crash tests. i-d91b stays open: needs a real checkpoint.
- [ ] Record reproducible settings and a pilot scorecard when predictions are available. (i-d91b)
