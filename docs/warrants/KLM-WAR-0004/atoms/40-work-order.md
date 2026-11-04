---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e86b-77b5-ba65-8451473b13b3
role: work_order
jurisdiction: authored
order: 40
classification: internal
---

# Work Order

## Deliverables

1. Parsers and exporters for: X-FEN, Shredder-FEN, EPD, UCI move lists
   (standard `e1g1` and `e1h1` Chess960 flavors), long algebraic, ICCF
   numeric, and descriptive. Each: parse + emit, refusal on malformation.
2. FRC/DFRC ingest first-class: Scharnagl start enumeration, `uci_chess960`
   games replayable; GameRecord gains an OPTIONAL `variant` field (additive,
   default `standard` — old records parse unchanged; this recorded addition
   is the Warrant's one seam amendment).
3. Differential parity harness: every fixture in the corpus (W-0002/0003
   paths included) run through both knightlm and python-chess; disagreement
   is a filed bug or a disposition, never silence.
4. Export menu completed for the viewer (W-0010) to call.

## Seams and who depends on them

1. **SEAM `nota-detect` amendment** — `Format` gains its remaining variants,
   additively, as the seam contract from W-0002 pre-declared. Consumers:
   KLM-WAR-0007 (FRC games in corpus), KLM-WAR-0010 (export menu).
2. **SEAM `record-variant`** — `GameRecord.variant: Option<Variant>`, default
   standard. Consumers: every later Warrant reading records; old shards stay
   valid (no rewrite required).
3. Frozen: `knightlm-core` public API beyond the additive field; the
   thin/annotated PGN seams.

## Owner's space

Internal representations of each format, name mappings, error granularity,
harness architecture, test corpus growth. Consumers decide only whether the
output satisfies their contracts.

## Premade instructions

- Every new format lands with fixtures and refusals together.
- python-chess disagreements: never suppress; file or disposition.

## Autonomy and escalation

Tier T2. The defensive additive-forward rule for seams is agreed; anything
breaking a recorded consumer escalates.

## Rollback

Revert the additive `variant` field (old records never depended on it) and
drop the new modules; no other surface changes.
