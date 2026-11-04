---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e8db-76fa-b281-c7789e936e8f
role: assurance
jurisdiction: authored
order: 60
classification: internal
---

# Assurance

## Acceptance Obligations

### OBL-001 — the environment and the study are real
- **scope:** the burn smoke test record; the study doc
- **evidence:** smoke log with backend/driver versions; the study doc's component inventory
checked against the pinned HF reference (line-item cites)
### OBL-002 — the parity skeleton is honestly red
- **scope:** the named gate tests (RQ-013: processor/tokenizer parity, reference
logits, gradients, overfit, reload, reset isolation)
- **evidence:** test run record: each gate named, status RED with the reason for red; zero
green without reference evidence
### OBL-003 — fake-green attempts refuse
- **scope:** a planted test claiming parity with fabricated logits
- **evidence:** the battery's self-check test discovers fabrications (e.g., a hash mismatch
or a missing-reference marker) and fails visibly
### OBL-004 — the next implementer can continue without the author
- **scope:** an ML-lane member new to Track B reads the study, runs the skeleton
- **evidence:** hand-off note: both artifacts understood, next actions named, questions
listed for the sponsor

## Gate Adequacy

Required at `basic`.

**Adversarial question:** Could 'red' become theater (a test that can never go green without being noticed as such)?

- **outcome:** no — each gate maps to a named test and an expected outcome; the fake-green refusal test is itself in the battery.

**Adversarial question:** Could the study cite the reference model while pinning the wrong revision?

- **outcome:** no — the study doc carries pinned revisions; reference mismatches are a named failure mode.


## Residual Risk

Burn 0.21's GPU backend selection may land differently than planned
(wgpu/cuda feature differences); the study records both options rather than
promising one. Reset isolation points in SmolVLM need care (attention cache
reuse across contexts); the study inventories them for the future implementer,
not this Warrant.
