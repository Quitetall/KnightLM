---
format: openwarrant/1.0
profile: ticket
alias: t-bb94
uuid: 01a1a54a-31f5-70e2-912b-c3988e1a9d77
priority: 2
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Per-user completion, last visited and placeholder grading fields."
---
# Study progress overlay and manual completion

## Checklist

- [x] Progress read/write and badges (i-769f, after t-4200) — done by claude, 2026-11-04: A checkbox per chapter writes progress and a done/total counter sits in the study header; verified in cdp_study.js as 1/3.
- [x] Manual completion and last visited (i-3d3b) — done by claude, 2026-11-04: Manual completion is the checkbox. Last visited is remembered per study in localStorage (a personal cursor, not database state), so reopening a study returns to the chapter you left.
- [x] Reserved attempts/score/hints/mastery fields (i-e18d) — done by claude, 2026-11-04: attempts/score/hints/mastery are recorded through set_study_progress and read back; a test asserts they survive, because a schema that does not record them cannot be retrofitted without a migration.
- [x] No content-hash impact (i-2d03) — done by claude, 2026-11-04: A test asserts ticking a chapter changes neither the study updated timestamp, nor current_version, nor the published bundle bytes, and that the bundle contains no completion data.
- [x] Progress isolation tests (i-f21c) — done by claude, 2026-11-04: crates/rbc-db/tests/study_progress.rs: per-user isolation (alex completed, sam has not, own attempt counts), per-study isolation, and a clean empty result for a user who never opened it.

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-db --test study_progress && cd apps/rusty-brick-chess/frontend/tests/cdp && node cdp_study.js
```

Result: 4 progress tests pass; AI_STUDY_PASS with ticking a chapter (1/3), the checkbox surviving a reload, and the header count.
Covers i-769f, i-3d3b, i-e18d, i-2d03, i-f21c.
