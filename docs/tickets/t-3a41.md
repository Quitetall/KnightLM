---
format: openwarrant/1.0
profile: ticket
alias: t-3a41
uuid: 01a1a54a-31af-755d-a6d8-6de3d396037c
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Store tournaments and resume without replaying or losing games."
---
# Tournament persistence, standings and combined PGN

## Checklist

- [x] Tournament/state/standings tables (i-e738, after t-a98f) — done by claude, 2026-11-04: Schema v8 adds tournaments and tournament_standings; crates/rbc-db/src/tournament.rs is the storage API and crates/ai/src/tournament_store.rs the save/load layer.
- [x] Resume after restart (i-3cb8) — done by claude, 2026-11-04: crates/rbc-db/src/tournament.rs (schema v8) and crates/ai/src/tournament_store.rs.
- [x] Combined PGN and per-game logs (i-dda9) — done by claude, 2026-11-04: crates/rbc-db/src/tournament.rs (schema v8) and crates/ai/src/tournament_store.rs.
- [x] Standings projection (i-0513) — done by claude, 2026-11-04: crates/rbc-db/src/tournament.rs (schema v8) and crates/ai/src/tournament_store.rs.
- [x] Resume tests (i-b14e) — done by claude, 2026-11-04: crates/rbc-db/src/tournament.rs (schema v8) and crates/ai/src/tournament_store.rs.

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-ai --all-features tournament
```

Result: 26 tests in the tournament suite, including a resume round-trip.
Covers i-e738, i-3cb8, i-dda9, i-0513, i-b14e.
