---
format: openwarrant/1.0
profile: ticket
alias: t-5364
uuid: 01a1a54a-3171-7658-9a46-3878c640b7e6
priority: 0
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Create the shared match loop for human and model seats."
---
# Match state machine, persistence and PGN

## Checklist

- [x] MatchConfig/Player/MatchState types (i-ace8, after t-7e92) — done by claude, 2026-11-04
- [x] Persist transitions before side effects (i-97b8) — done by claude, 2026-11-04
- [x] Move log, clocks, result/termination (i-b4e9) — done by claude, 2026-11-04
- [x] PGN export with headers/comments (i-8fad) — done by claude, 2026-11-04
- [x] Restart/resume tests (i-dffb) — done by claude, 2026-11-04

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-ai --all-features match
```

Result: 26 of 26 tests pass (match binary; the test target also picks up the game module's own tests). The --all-features flag is required because the persistence half lives behind the host cargo feature. Also verified live: a timed match created over HTTP starts at 60s, a 2s think left 57.98s, and resigning charged the other side before ending 1-0 by resignation.
Covers i-ace8, i-97b8, i-b4e9, i-8fad, i-dffb.
