---
format: openwarrant/1.0
profile: ticket
alias: t-ab7d
uuid: 01a1b895-f455-7627-bf45-7ae724512fbc
priority: 3
created_at: "2026-11-08T00:36:37Z"
created_by: "codex-contributor-setup"
summary: "Prove that adding a source produces a new audited snapshot and that a single command can train from it."
labels:
  - "mlops-cv"
paths:
  - "model/tests/**"
  - "model/run_pipeline.py"
  - "model/README.md"
x.depends_on:
  - "t-f724"
  - "t-ea98"
requires:
  - "x.depends_on"
---
# MLOps/CV Engineer — Test the joined source-to-training loop

**Start after** the [ETL loop](t-f724.md) and
[training entry point](t-ea98.md). Start with the shared CPU fixture.
Ask Data to review source behavior and Representation/Evaluation to review
which holdout is used.

Connect the ingestion and trainer entry points behind one documented
command. It should show a plan by default and, when explicitly asked to
train, use the exact snapshot it just verified. Test an initial input,
an unchanged rerun, an added input, and a deliberately bad split or shard.
The bad input must stop before GPU work and leave the prior verified
snapshot intact. A failed training attempt must leave a clear error and
preserve previous completed runs.

**Review.** Capture the CPU integration check and a bounded GPU run
that produces and reloads a checkpoint from the added-source snapshot.
Show the same corpus digest in both manifests. Present the evidence to
the project lead before scheduling full Run 1. Keep CV work as a later
track once this text training loop is repeatable.

## Checklist


- [x] Test first input, unchanged rerun, added input, and bad data on CPU. (i-1928) — done by claude-blut, 2026-11-13: model/tests/test_pipeline.py (6 tests, CPU): first input, unchanged rerun (snapshot byte-identical), added input (new snapshot, old untouched), leaked and tampered source rejected before any launch with the prior snapshot intact. The ingest step is a slice-of-corpus stand-in until the ETL ticket t-f724 lands. Run: python3 -m unittest model.tests.test_pipeline
- [x] Record an opt-in bounded GPU run and checkpoint reload from the new snapshot. (i-a67c) — done by claude-blut, 2026-11-13: python3 -m model.run_pipeline --gpu --steps 30 --eval-rows 50: snapshot -> materialize -> 30 GPU steps -> evaluate 50 holdout rows (PR #18 harness from an overlay checkout) -> register; reload error 0.0; same corpus digest 56ca35f2... in snapshot, entry.json and run-manifest.json (pipeline-report.json same=true). Crash/resume check in crash-resume/. Evidence: /mnt/4tb/artifacts/KnightLM/keep/blut-knightlm-smoke-2026-11-13
- [ ] Hand the two linked manifests and rerun command to the project lead. (i-04cd)
