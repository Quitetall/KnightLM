---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e89a-75ef-b07b-79af5177d4ef
role: assurance
jurisdiction: authored
order: 60
classification: internal
---

# Assurance

## Acceptance Obligations

### OBL-001 — ingest preserves provenance per row
- **scope:** the 2M-game run over monthly bulk files
- **evidence:** every `GameRecord` carries bulk-source file names; the manifest ties files
to receipts the team recorded at download
### OBL-002 — label joins are policy-correct and label-only
- **scope:** sampled positions from the eval set checked against the lichess eval file
- **evidence:** spot-check table: normalized position key match, depth >= 20, highest-depth
row chosen, first PV as policy target; zero rows where the label reads like
ground truth (labels + engine + depth columns present instead)
### OBL-003 — the split audit refuses a planted leak
- **scope:** a planted duplicate game family straddling splits and a canonical-position
duplicate across train/eval
- **evidence:** audit run record: both plants rejected by name; split stats recomputed
post-rejection
### OBL-004 — counts materialize as contracted
- **scope:** final shard tree + manifest
- **evidence:** manifest: 2M-game train pool shards; 200K train / 10K eval rows; per-shard
hashes; train/eval strictly temporal
### OBL-005 — the consumer trains without the author
- **scope:** KLM-WAR-0008's loader stage run by the ML lane
- **gate:** gate://software.repo.war-check@1.0.0
- **evidence:** cross-lane recorded check: the loader reads the manifest and trains or
evaluates on shards directly, with no author queries

## Gate Adequacy

Required at `basic`.

**Adversarial question:** Could a leak survive if the audit only checks whole-game duplicate IDs but not family/canonical duplicates?

- **outcome:** no — OBL-003 plants both kinds and requires rejection; the audit cannot pass on a single check kind.

**Adversarial question:** Could labels creep into 'data' and lose their provenance column?

- **outcome:** no — OBL-002 checks label columns exist and look like labels, not like truth.


## Residual Risk

Join cardinality: many eval rows per position with ending-variant depths; the
tie policy is decided pre-run and the audit verifies it. Disk footprint on
the order of tens of GB — sized before the run. Malformed Lichess rows are
expected and are an explicit-refusal count, not a silent skip, per RQ-004;
weekly ingest reports log refusal shapes.
