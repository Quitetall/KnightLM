---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e8ba-73df-97ed-ca7d7a99d0c9
role: basis
jurisdiction: authored
order: 20
classification: internal
---

# Basis

## Sources

- KnightLM SAS revision 0.2.1: §81 Phase 6; KLM-SAS-RQ-009 and RQ-010 (§106).
- Sponsor's serving decision (2026-10-20): a Python server first (FastAPI +
  transformers); adapters load directly on the pinned base model; no port
  to native inference before run-1's eval verdict.
- KLM-WAR-0006's Player seam (the UCI walls), and KLM-WAR-0008's
  `model-io`/`run-manifest` seams (what serving consumes).

## Prerequisites

- KLM-WAR-0006 M3: KnightLM-UCI plays scripted games today.
- KLM-WAR-0008 M2: a run-1 checkpoint + manifest exist.

## Open questions

None.
