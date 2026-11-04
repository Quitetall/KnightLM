---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-1bea-73e5-af55-6182429637ab
role: basis
jurisdiction: authored
order: 20
classification: internal
---

# Basis

## Sources

- KnightLM SAS revision 0.2.1 (sha256 f634dbd65e4a, proposed): §98 Phase 2 —
  Universal state hub; KLM-SAS-RQ-004 and KLM-SAS-RQ-006 (§106).
- `code/README.md` "upstream warts": Lichess bulk export emits four-field FENs
  and `UCI_Chess960` PV keys; heterogeneous analysis depths exist in the eval
  side, not the game side. A four-field-FEN source never yields
  `history_complete: true` (KLM-SAS-RQ-006).
- cozy-chess 0.3.4 provides move generation but has no SAN parser or writer
  (checked against docs.rs). The SAN engine is hand-written here.
- Lichess database facts (database.lichess.org, fetched from the live site on
  2026-10): bulk game export is monthly `.pgn.zst`; the thin reader only needs
  the Seven-Tag Roster plus movetext.

## Prerequisites

- `code/crates/core` compiles and `cargo test -p knightlm-core` passes today.

## Open questions

None: scope boundaries were settled with the sponsor on 2026-10-20.
