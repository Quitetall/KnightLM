---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e8db-76fa-b281-c7789e936e8f
role: basis
jurisdiction: authored
order: 20
classification: internal
---

# Basis

## Sources

- KnightLM SAS revision 0.2.1: §83 Phase 7; KLM-SAS-RQ-013 (§106).
- Sponsor's Track B decision (2026-10-20): 10% of ML capacity starting week
  2; admission is ONLY through the parity gates (processor/tokenizer parity,
  reference logits, gradients, overfit, reload, reset isolation between
  passes); run-1 Track A harness is the oracle when it lands.
- Burn pinned at 0.21 (the repo's existing engine dependency), plus the
  code/docs/plan.md Phase-7 sketch of the parity gate list.

## Prerequisites

- KLM-WAR-0008's `model-io` seam (the parity battery will compare against
  Track A's serving) — its M2 lands before this Warrant's M2.

## Open questions

None: the phase boundary and the gate list were settled on 2026-10-20.
