---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e8db-76fa-b281-c7789e936e8f
role: work_order
jurisdiction: authored
order: 40
classification: internal
---

# Work Order

## Deliverables

1. Burn environment bring-up: `burn 0.21` + a GPU backend choice (wgpu or
   cuda) proven green on a tiny smoke test; the choice recorded as evidence
   with versions (driver/toolkit compatible).
2. The architecture study doc (`code/docs/trackb-smolvlm-study.md`):
   component inventory of SigLIP encoder + pixel-shuffle projector + LLM
   decoder, parameter counts, tensor shapes per layer where documented in
   the model card, the state reset points (KV attention, recurrent/conv
   states) that RQ-013's reset isolation must quarantined, and each parity
   gate mapped to a named test.
3. The parity skeleton: a test module in `crates/model` (or a proposed
   layout for it) with the named gate tests from KLM-SAS-RQ-013, red NOW
   where the parity falls, with red/green mapping zero decoding paths (the
   SKEL test hard-fails a fake-green attempt), compile-checked.
4. A smoke session where the skeleton's IO paths (tokenizer side) actually
   run against the pinned reference and record what is currently
   represented (RED documented honestly).

## Seams and who depends on them

1. **SEAM `parity-battery`** — the named test list + expected outcomes from
   the study. Consumers: the future Burn implementation Warranty (post-0011),
   KLM-WAR-0008's model-io (an oracle target across teams).
2. Frozen consumed seams: model-io (W-0008) — the battery's target format.

## Owner's space

Where the study file lives, its doc shapes, the test module's inner
structure, the backend choice internals, the smoke session scripting.
Anything that would DICTATE the future implementation's architecture beyond
the parity contracts is sponsor territory.

## Premade instructions

- RED is honest. Greenwashed-anything is a defect: a planted fake-logit test
  MUST fail.
- Outcomes write down what you do not know (the `unknown` evidence class
  exists for you).

## Autonomy and escalation

Tier T2. Escalation: any deviation from the parity gate list (they are
sponsor'd requirements), any attempt to pull Track A's strength targets into
Track B.

## Rollback

Delete the study md and the test module; burn env brings nothing into the
workspace root config that cannot be cleanly removed (the smoke stays in
this repo, the env is local).
