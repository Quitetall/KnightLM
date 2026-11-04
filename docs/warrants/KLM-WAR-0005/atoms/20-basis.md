---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e87b-701d-bd00-f083e15c85ff
role: basis
jurisdiction: authored
order: 20
classification: internal
---

# Basis

## Sources

- KnightLM SAS revision 0.2.1: §77 Phase 4 — Engine tooling; KLM-SAS-RQ-009
  (§106).
- Decisions of 2026-10-20: Stockfish 19 pinned (d15 evals, d8-10 sparring);
  labels from lichess_db_eval joined by normalized position key with depth
  floor 20 as primary corpus labels; own SF evals reserved for the eval set
  and scoring. Recorded in `docs/sas/KnightLM_SAS.md` §77 and this Warrant.
- Lichess database facts (database.lichess.org, live 2026-10): the evals
  export is 416M positions keyed by four-field FEN with heterogeneous depths
  and `UCI_Chess960` PVs.
- Sponsors' rule: no engine, model or dataset is ever auto-downloaded at
  runtime; `STOCKFISH_PATH` is an operator-provided contract, like the rest
  of `code/README.md`'s precedent.

## Prerequisites

- `knightlm-core` builds; no other lane blocks this one.

## Open questions

None: engine boundary semantics were settled with the sponsor on 2026-10-20.
