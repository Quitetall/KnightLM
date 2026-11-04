---
format: openwarrant/1.0
profile: ticket
alias: t-1315
uuid: 01a1a54a-3185-76b5-8778-4348cba9fef2
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Two model seats with live/pause/step and per-side reasoning."
---
# Watch mode, pause/step and annotated PGN

## Checklist

- [x] Two model seats and colour assignment (i-52e2, after t-5364) — done by claude, 2026-11-04
- [x] Start/pause/step/reset (i-9665) — done by claude, 2026-11-04
- [x] Per-side reasoning toggles (i-8201) — done by claude, 2026-11-04
- [x] Annotated PGN export (i-3df7) — done by claude, 2026-11-04
- [x] CDP: watch (i-e3c4) — done by claude, 2026-11-04

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-ai --all-features --test watch
```

Result: 10 of 10 pass. Live: a two-model match stepped 2 plies, paused (a step then answered 'the game is paused'), resumed to awaiting_white, and reset to 0 plies. Browser: frontend/tests/cdp/cdp_ai_watch.js -> AI_WATCH_PASS - white's ply annotated and black's not in the same game, PGN '1. a3 {controls the centre} a6 *'.
Covers i-52e2, i-9665, i-8201, i-3df7, i-e3c4.
