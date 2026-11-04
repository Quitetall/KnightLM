---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e8aa-763c-85bd-1aecc0ffb5f7
role: assurance
jurisdiction: authored
order: 60
classification: internal
---

# Assurance

## Acceptance Obligations

### OBL-001 — run 1 completes within the cost cap with full lineage
- **scope:** the RunPod A100 run
- **evidence:** receipts: MLflow run ids, cost breakdown vs the $40 cap, wall time,
checkpoint hashes; a run exceeding cap is stopped by the script's guard
### OBL-002 — each eval axis is reported separately; legality gate stated
- **scope:** the 10K-game temporal holdout through the eval harness
- **evidence:** report artifact per axis: raw legality (must clear >= 90% to pass the sponsor
gate), masked legality 100% by construction, token cost with the admitted
tokenizer, assisted legality, strength proxy vs SF19 fixed depth; the report
generator literally has no composite path (filed refusal + report text)
### OBL-003 — the refusal battery proves illegality is never silently dropped
- **scope:** planted illegal model outputs in a same-format fixture run
- **evidence:** corrupt outputs show as failed moves in the report, move graph; a planted
composite-axis attempt fails to generate a report
### OBL-004 — serving consumes the artifact without the author
- **scope:** KLM-WAR-0009 M1 run by the serving lane
- **gate:** gate://software.repo.war-check@1.0.0
- **evidence:** cross-lane recorded check: load base+adapter+chat template via run-manifest,
packets formatted unaided

## Gate Adequacy

Required at `basic`.

**Adversarial question:** Could a 'winner score' creep in through an aggregate number in the report?

- **outcome:** no — the report generator has no composite path; OBL-002 is written as a refusal, not a convention.

**Adversarial question:** Could the legality gate pass with nonzero silent drops?

- **outcome:** no — planted illegal outputs are visibly counted; the move-graph evidence shows totals.


## Residual Risk

Cost overrun from a long-running data loader has a hard script-guard but
still costs some minutes. SmolVLM LoRA runs have proven trick historically
(template drift); dry-run first. Evaluation variance across seeds is a
reporting risk: the report states the seed and the count, not vibes.
