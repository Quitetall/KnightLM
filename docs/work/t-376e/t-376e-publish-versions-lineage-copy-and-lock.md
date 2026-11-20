---
format: openwarrant/1.0
profile: ticket
alias: t-376e
uuid: 01a1a54a-31e1-7587-ae09-a7cb3cfe4d49
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Make published content immutable and copies traceable."
---
# Publish, versions, lineage, copy and lock

## Checklist

- [x] Publish freezes a version (i-67d8, after t-4200, t-5638) — done by claude, 2026-11-04: Publishing freezes a schema rbc-study/1 bundle of the study, its chapters and their boards; a later chapter does not change the frozen version.
- [x] New draft branches from a version (i-b26a) — done by claude, 2026-11-04: branch_study_from_version creates a new draft carrying branched_from {study_id, version} in metadata.
- [x] Manual lock on study/board (i-142a) — done by claude, 2026-11-04: schema v11 studies.locked, lock_study, and the tools refuse a locked study with a conflict naming the lock.
- [x] copy_required/copy with provenance (i-9dac) — done by claude, 2026-11-04: Board writes still answer copy_required for locked/published boards and copy_board carries the copy; study copy keeps its chapters.
- [x] Tests for immutability/lineage (i-60f5) — done by claude, 2026-11-04: crates/rbc-db/tests/study_versions.rs (5 tests) plus the locked-study test in crates/ai/tests/study_tools.rs.

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-db --test study_versions && cargo test -p rbc-ai --all-features study_tools
```

Result: 5 version tests and 5 tool tests pass; verified live that publish makes version 2, versions lists [2], branch returns a draft with branched_from, and lock returns true.
Covers i-67d8, i-b26a, i-142a, i-9dac, i-60f5.
