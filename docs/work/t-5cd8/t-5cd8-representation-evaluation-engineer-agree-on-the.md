---
format: openwarrant/1.0
profile: ticket
alias: t-5cd8
uuid: 01a1b895-f3c3-711d-80c2-6ab539b06ec3
priority: 3
created_at: "2026-11-08T00:36:37Z"
created_by: "codex-contributor-setup"
summary: "Define the versioned corpus manifest and a small shared fixture so the data builder and trainer can work independently."
labels:
  - "representation-evaluation"
paths:
  - "docs/model/corpus-contract.md"
  - "model/fixtures/corpus-contract/**"
---
# Representation/Evaluation Engineer — Agree on the corpus-to-trainer contract

**First sitting.** Set aside about an hour to read the pilot
[corpus and training guide](../../model/README.md#lichess-lora-pilot).
Ask Data and MLOps to review one example manifest before implementing it.

Keep standard untimed chess and the existing replay compiler as the first
target. Specify how a run names source provenance, split boundaries, shard
hashes and counts, packet version, target UCI move, and held-out data.
Define where ingestion ends and how the trainer rejects an incompatible or
unaudited artifact. Keep teacher labels and future image inputs outside the
initial training packet.

**Review.** Deliver `docs/model/corpus-contract.md` and a tiny, hand-checkable
fixture in `model/fixtures/corpus-contract/`. Include a valid corpus and
one invalid example. Data checks that it can produce it, MLOps checks that
the trainer can read it, and the project lead records which version to use.
This is a design and fixture task that can be done on CPU.

## Checklist


- [ ] Draft the manifest and packet interface with one worked example. (i-3f7b)
- [ ] Commit a tiny valid and invalid fixture and record the two handoff reviews. (i-b16f)
