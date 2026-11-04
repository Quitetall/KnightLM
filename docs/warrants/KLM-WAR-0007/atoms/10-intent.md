---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e89a-75ef-b07b-79af5177d4ef
role: intent
jurisdiction: authored
order: 10
classification: internal
---

# Intent

Turn the raw bulk Lichess stream into the run-1 corpus: 2 million timed blitz
and rapid standard games at 1600+ (bullet excluded), reduced to decision-point packets,
joined with the lichess eval labels by normalized position key, split into
temporally-separated train and eval sets, audited for leaks, and materialized
as sharded JSONL with a provenance manifest — the load-bearing data contract
everything downstream consumes (KLM-SAS-RQ-011).

## Out of scope

- Training itself — KLM-WAR-0008.
- crawler/download mechanics: bulk files arrive hand-downloaded; the corpus
  work assumes they are on disk.
