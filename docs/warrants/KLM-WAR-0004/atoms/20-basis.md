---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e86b-77b5-ba65-8451473b13b3
role: basis
jurisdiction: authored
order: 20
classification: internal
---

# Basis

## Sources

- KnightLM SAS revision 0.2.1: §98 Phase 2; KLM-SAS-RQ-007 (standard and
  Chess960/DFRC first-class, in both UCI castling flavors) plus RQ-004..006
  (§106).
- cozy-chess 0.3.4 supports FRC/DFRC position representation and random
  starts (Scharnagl numbering); the origin-square castling legality rules are
  ours to express.
- python-chess (pinned, a dev-dependency) as the differential oracle.
- `code/README.md` warts: `UCI_Chess960` PV format matches cozy-chess
  internals — the flavor translation table (e1g1 vs e1h1) is written here.

## Prerequisites

- KLM-WAR-0002 and KLM-WAR-0003 seams in place.

## Open questions

None.
