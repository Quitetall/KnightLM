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


- [ ] Test first input, unchanged rerun, added input, and bad data on CPU. (i-1928)
- [ ] Record an opt-in bounded GPU run and checkpoint reload from the new snapshot. (i-a67c)
- [ ] Hand the two linked manifests and rerun command to the project lead. (i-04cd)
