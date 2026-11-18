---
format: openwarrant/1.0
profile: ticket
alias: t-5ede
uuid: 01a1a54a-320b-7079-a323-c4b331072571
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Pin the study product end to end."
---
# Study tests: CRUD, round-trip, progress, authoring

## Checklist

- [x] CRUD/order tests (i-b1dc, after t-5638, t-376e, t-2927, t-bb94, t-bb8b) — done by claude, 2026-11-04: rbc-db/tests/db.rs and foundation.rs: create, read, update, delete, chapter order, revisions, version and progress counters.
- [x] Round-trip fixtures incl. unknown fields (i-2663) — done by claude, 2026-11-04: rbc-ai/tests/study_bundle.rs: export, parse, write back; unknown top/chapter/board fields survive; a full import into a different database re-exports the same study.
- [x] Progress overlay tests (i-980c) — done by claude, 2026-11-04: rbc-db/tests/study_progress.rs: per-user and per-study isolation, reserved fields, and that progress changes no version or bundle. cdp_study.js ticks a chapter and reads 1/3.
- [x] Authoring/approval tests (i-4d4e) — done by claude, 2026-11-04: crates/ai/tests/study_authoring_approval.rs: a study write needs approval, a board-write grant does not satisfy it, in-app grants do not leak to MCP, and an approved create_study applies.
- [x] One suite command (i-8f4b) — done by claude, 2026-11-04: scripts/check_studies.sh runs every study test and the browser editor, starting its own server when none is healthy; check_all.sh calls it.

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && ./scripts/check_studies.sh
```

Result: STUDY_SUITE_PASS: 89 tests across storage, tools, approvals, bundle and the browser editor.
Covers i-b1dc, i-2663, i-980c, i-4d4e, i-8f4b.
