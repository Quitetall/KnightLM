---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e89a-75ef-b07b-79af5177d4ef
role: work_order
jurisdiction: authored
order: 40
classification: internal
---

# Work Order

## Deliverables

1. Corpus ingest driver over `nota-reader`: filter to standard chess, timed
   blitz and rapid, both players 1600+, two-move mate/abandoned games
   dropped; 2M games ingested to shard JSONL.
2. Packet compiler extension: per decision point, per-arm packets + policy
   target (the played move) + a joined label when available. Tablet narrowing
   mirroring `code/crates/core`'s exp-backed format.
3. Label join: normalized position key (from FEN) >> `lichess_db_eval.jsonl`;
   depth floor 20; highest-depth row wins ties; first PV token parsed with
   the `UCI_Chess960` flavor table (KLM-WAR-0004's); cp/mate preserved; joins
   recorded as LABELS ONLY per RQ-011.
4. Splits: 200K train games; 10K eval games strictly from the newest month of
   the pool; game-family disjoint (same opening family/traditional
   neighboring transposition clusters do not straddle splits).
5. Split audit + manifest: counts, per-shard hashes, provenance keyed to file
   names and download receipts the team records; an audit that refuses to
   bless a split with a planted leak.

## Seams and who depends on them

1. **SEAM `corpus-shards`** — directory layout + manifest schema: shard
   files, per-shard row counts/hashes, label source, split policy, filter
   params, SF version for any own labels. Consumers: KLM-WAR-0008 (the
   loader), evaluation (verifying splits), sponsor (experiment lineage).
2. **SEAM `packet-row`** — per-row packet structure extending
   `code/crates/core`'s compiler outputs with labels. Consumers: 0008's
   training and eval harnesses.

## Owner's space

Join strategy and index structure, sharding and size policy, internal cache
layout, sampling details, parallelism, packing decisions, and how family
disjointness is computed given the samples. The audit's verdicts and the
manifest must never be fabricated; how efficient they are is free.

## Premade instructions

- The split audit must be seen rejecting a planted leak; an audit that has
  only blessed neighbors is untrusted.
- Every label column is a label — provenance and join policy are data, not
  side notes: label provenance columns reflect what the join did.

## Autonomy and escalation

Tier T2. Anything touching the split policy or adding an engine to the
chain-of-custody defaults out of bounds without the sponsor.

## Rollback

Shards + manifest live under `data/corpus/run1/` (git-ignored); delete the
directory. The compiler extension and audit code remain in git history.
