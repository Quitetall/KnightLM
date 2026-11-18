---
format: openwarrant/1.0
profile: ticket
alias: t-a994
uuid: 01a1a54a-31cf-76dc-908e-8d9be7f53585
priority: 2
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Create/running/finished tournament views and standings."
---
# Tournament UI and end-to-end tests

## Checklist

- [x] Create form for all formats (i-4e92, after t-f26f, t-3a41, t-2bbf, t-a488) — done by claude, 2026-11-04: See the ticket note.
- [x] Live pairings and standings (i-4b04) — done by claude, 2026-11-04: See the ticket note.
- [x] Pause/resume/step, logs, export (i-797a) — done by claude, 2026-11-04: See the ticket note.
- [x] CDP: tournament end-to-end (i-34dd) — done by claude, 2026-11-04: See the ticket note.

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM/apps/rusty-brick-chess/frontend/tests/cdp && node cdp_tournament.js
```

Result: AI_TOURNAMENT_PASS - a tournament is created from the form with a chosen format and competitors, Step reports two pairings and the pairings render, four standings rows, Pause reports paused, and Export PGN offers match.pgn.
Covers i-4e92, i-4b04, i-797a, i-34dd.
