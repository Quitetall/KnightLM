---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e89a-75ef-b07b-79af5177d4ef
role: basis
jurisdiction: authored
order: 20
classification: internal
---

# Basis

## Sources

- KnightLM SAS revision 0.2.1: §79 Phase 5; KLM-SAS-RQ-011 and RQ-006 (§106).
- The sponsor's run-1 decisions (2026-10-20): 2M standard games; filter
  blitz+rapid both players averaged 1600+; Chess960 excluded from the run-1
  corpus; 200K train games; 10K eval games held out temporally; labels
  joined from `lichess_db_eval.jsonl` by normalized position key with a
  depth floor of 20, highest depth per position; first PV (parsed as
  `UCI_Chess960`) is the policy target; the played move stays the primary
  signal.
- Lichess database facts (database.lichess.org, live 2026-10): bulk game
  files are monthly `.pgn.zst`; the evals export is keyed by four-field FEN
  and carries `depth`, `pv` (UCI_Chess960 flavored), and `cp`/`mate` lines.

## Prerequisites

- KLM-WAR-0002 M2/M3 landed (`nota-reader`, CLI ingest working).
- KLM-WAR-0005 M2 landed (engine + label shape, for any own-engine labels).

## Open questions

None: the split policy and join keying were settled on 2026-10-20.
