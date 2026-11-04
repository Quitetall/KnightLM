---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e889-750b-9aea-e2b104ca6e3f
role: basis
jurisdiction: authored
order: 20
classification: internal
---

# Basis

## Sources

- KnightLM SAS revision 0.2.1: §77 Phase 4; KLM-SAS-RQ-010 (§106).
- UCI protocol basics (uci/ucinewgame/isready/position/go/stop/quit,
  bestmove lines, id and option handshakes) as known from the standards
  around UCI; the sponsor confirmed the standard subset on 2026-10-20.
- `knightlm-core` move generation and the decision-point compiler (its
  legal-move arm coverage is the source of legality).

## Prerequisites

- `knightlm-core` compiles and tests pass today.

## Open questions

None.
