---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e8aa-763c-85bd-1aecc0ffb5f7
role: basis
jurisdiction: authored
order: 20
classification: internal
---

# Basis

## Sources

- KnightLM SAS revision 0.2.1: §81 Phase 6; KLM-SAS-RQ-012 (§106).
- Sponsor's run-1 decisions (2026-10-20): SmolVLM-500M LoRA first; text-only
  packets; exit gate = raw legality >= 90% on held-out positions and masked
  legality = 100% by construction; all axes reported; no strength threshold;
  MLflow self-hosted as the system of record, Weights-and-Biases optional
  behind one logging seam; RunPod A100; $40 per run.
- `code/crates/core`'s packet compiler (its packet contracts are the model
  I/O form).
- Corpus from KLM-WAR-0007 (200K train / 10K eval temporal shards).

## Prerequisites

- KLM-WAR-0007 M3 complete: shards + manifest audited.
- KLM-WAR-0005 M2 for the strength-axis labels (fixed-depth SF19).

## Open questions

None: axis definitions and the exit gate were settled on 2026-10-20.
