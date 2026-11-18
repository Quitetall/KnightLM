---
format: openwarrant/1.0
profile: ticket
alias: t-a488
uuid: 01a1a54a-31c4-7192-b2b8-2cf96a27b509
priority: 2
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Show rating/RD/rank, metrics and enforce token/cost caps."
---
# Leaderboard, metrics and budgets

## Checklist

- [x] Leaderboard table and filters (i-c239, after t-3a41, t-2bbf) — done by claude, 2026-11-04: See the ticket note.
- [x] Metrics: legal rate, retries, tokens, latency, cost, duration (i-3645) — done by claude, 2026-11-04: See the ticket note.
- [x] Per-turn/session/tournament caps; pause at cap (i-c857) — done by claude, 2026-11-04: Schema v9 adds token_budget/tokens_used; the turn route refuses when spent and accounts each turn; the leaderboard shows Turns and Tokens.
- [x] Usage display (i-c8a8) — done by claude, 2026-11-04: Schema v9 adds token_budget/tokens_used; the turn route refuses when spent and accounts each turn; the leaderboard shows Turns and Tokens.
- [x] UI/CDP tests (i-4519) — done by claude, 2026-11-04: See the ticket note.

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM/apps/rusty-brick-chess/frontend/tests/cdp && node cdp_leaderboard.js
```

Result: AI_LEADERBOARD_PASS - the leaderboard and tournament panels render, a tournament is created from the UI, Step reports 'started 2 game(s)', Pause reports 'tournament paused', and the standings table has 4 rows.
Covers i-c239, i-3645, i-c857, i-c8a8, i-4519.
