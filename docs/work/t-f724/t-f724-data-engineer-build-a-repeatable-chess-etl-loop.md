---
format: openwarrant/1.0
profile: ticket
alias: t-f724
uuid: 01a1b895-f3e6-71d3-b5ab-a6c1be23a335
priority: 3
created_at: "2026-11-08T00:36:37Z"
created_by: "codex-contributor-setup"
summary: "Ingest a new chess source into a reproducible, audited corpus snapshot that the training entry point can consume."
labels:
  - "data-engineer"
paths:
  - "model/corpus/**"
  - "crates/cli/src/corpus.rs"
  - "docs/model/run1-sources.md"
x.depends_on:
  - "t-a475"
  - "t-5cd8"
requires:
  - "x.depends_on"
---
# Data Engineer — Build a repeatable chess ETL loop

**Start after** the [source inventory](t-a475.md) and
[corpus contract](t-5cd8.md). First run the agreed fixture through the
existing [PGN pipeline](../../model/corpus/pipeline.py) and identify the
single-file assumptions to replace.

Build one configurable ingestion path for standard PGN and compressed PGN.
For the first database input, use a copy of an RBC SQLite database. Add a
source adapter that opens the copy read-only and uses the existing PGN
export path to feed the same pipeline. A source configuration should accept
the database path directly, so the contributor does not have to export PGN
by hand on each run. Prove it with a tiny fixture. Choose additional database
formats separately.

Extract with the existing chess compiler, normalize and validate complete
legal replay, deduplicate across sources, and write a new audited snapshot
with the agreed temporal split. Keep a ledger of source fingerprints and
rejection counts. An unchanged rerun must reuse the same snapshot without
duplicate rows. Adding a second source must produce a new snapshot without
overwriting the prior one or silently admitting leakage. Publish only after
the split and shard checks pass. Hand MLOps the manifest path and a working
fixture command. The [full corpus order](t-3458.md) covers scaling and the
200,000/10,000 acceptance evidence.

**Review.** Show the initial run, unchanged rerun, second-source run, and a
rejected duplicate or leak on small inputs. Record commands and manifest
digests. Use the project artifact store for real sources and shards.

## Checklist


- [ ] Add source configuration, provenance, and a read-only database export fixture. (i-683e)
- [ ] Demonstrate idempotent ingestion, a second source, and a rejected leak. (i-19fc)
- [ ] Hand off an audited manifest and fixture command to MLOps. (i-d076)
