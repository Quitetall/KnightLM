---
format: openwarrant/1.0
profile: ticket
alias: t-a475
uuid: 01a1b88a-dbe0-77cc-b419-97140332d8be
priority: 2
created_at: "2026-11-08T00:24:30Z"
created_by: "codex-contributor-setup"
summary: "List the Lichess inputs available for Run 1 and show what is still needed for a later holdout."
labels:
  - "data-engineer"
paths:
  - "docs/model/run1-sources.md"
---
# Data Engineer — Inventory sources for Run 1

**Time and setup.** About 45–90 minutes of active work. A checkout is enough to
start. Ask a maintainer for the pilot manifest or shared-storage inventory if
you cannot see the source files.

Read the [pilot instructions](../../model/README.md#lichess-lora-pilot) and
the [Run 1 corpus order](t-3458.md). Start by listing the pilot source and
marking what you can verify. Add a short table in `docs/model/run1-sources.md`
with archive name, date coverage, origin, size, checksum where available, and
whether the team has access. Label missing or unverified fields. Propose a
temporal cutoff and list any missing later inputs. The maintainer can arrange
access or a bulk download after reviewing the inventory.

**Review.** The PR should let someone distinguish available files from missing
ones and decide whether the proposed holdout is feasible. A partial inventory
with a specific access blocker is a useful handoff. Leave shard generation
and leakage audits to the [full corpus order](t-3458.md).

## Checklist


- [ ] Record the inputs you can verify and label missing provenance or checksums. (i-7573)
- [ ] Propose a temporal cutoff and identify what a maintainer must supply. (i-7a11)
