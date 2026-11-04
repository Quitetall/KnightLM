---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-1bea-73e5-af55-6182429637ab
role: intent
jurisdiction: authored
order: 10
classification: internal
---

# Intent

Stand up the first half of the universal notation layer so corpus building can
start without waiting for the full PGN parser: a hand-written SAN engine over
the canonical `GameState`, a `knightlm-notation` crate skeleton with a
format-detection seam, and a thin streaming reader for Lichess bulk-DB PGN
that turns zst month files into provenance-carrying `GameRecord`s.

## Out of scope

- Full PGN fidelity: comments, NAGs, recursive variations — KLM-WAR-0003.
- Other notations (X-FEN, Shredder-FEN, EPD, UCI move lists, long algebraic,
  ICCF, descriptive) and first-class FRC ingest — KLM-WAR-0004.
- Labeling, joins, splits, engines — KLM-WAR-0005 and KLM-WAR-0007.
