---
format: openwarrant/1.0
profile: ticket
alias: t-f26f
uuid: 01a1a54a-3190-7008-add1-1269fca14cb2
priority: 0
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Render the match panel in the Assistant center pane."
---
# Match UI: cards, clocks, result and handoff

## Checklist

- [x] Player cards, clocks, move list (i-9404, after t-f436, t-5364, t-bf5d, t-1315) — done by claude, 2026-11-04
- [x] Result/termination and rematch (i-ca65) — done by claude, 2026-11-04
- [x] Open in Analysis / Save to database / Export PGN (i-c7ce) — done by claude, 2026-11-04
- [x] CDP: match UI (i-2533) — done by claude, 2026-11-04

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM/apps/rusty-brick-chess/frontend && cargo check --target wasm32-unknown-unknown
```

Result: passes. Browser: frontend/tests/cdp/cdp_match_ui.js -> AI_MATCH_UI_PASS with two player cards, plies 1. a3 / a6, a finished match reading 'draw · max plies' with Step disabled, Rematch making a new match (8 -> 9 cards, 0 plies), Pause showing 'paused', Save to database reporting 'saved 1 game(s)', Export PGN offering match.pgn, and Open in Analysis landing on the Analysis tab with the board loaded.
Covers i-9404, i-ca65, i-c7ce, i-2533.
