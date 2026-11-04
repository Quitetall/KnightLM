---
format: openwarrant/1.0
profile: ticket
alias: t-bf5d
uuid: 01a1a54a-317b-738e-8f02-4c6becac2718
priority: 0
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Wire human moves, chat rules, reasoning toggles and clocks."
---
# Human vs AI casual and competitive

## Checklist

- [x] Human move submission and validation (i-e719, after t-5364) — done by claude, 2026-11-04
- [x] Casual chat/hints/takebacks/engine (i-2af4) — done by claude, 2026-11-04
- [x] Competitive move-only and hidden reasoning (i-3630) — done by claude, 2026-11-04
- [x] Reasoning visibility/export/request toggles (i-41ec) — done by claude, 2026-11-04
- [x] CDP: human play (i-5087) — done by claude, 2026-11-04

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-ai --all-features play
```

Result: 13 of 13 tests pass (play binary). Also verified live against a provider that answers moves: a casual match accepted e4, the model replied e5 in one attempt, chat answered, and a takeback undid e5; a rated (competitive) match refused chat and takeback with 'not available in a competitive game' while its model move still worked and returned no reasoning. Browser check: frontend/tests/cdp/cdp_ai_play.js -> AI_PLAY_PASS.
Covers i-e719, i-2af4, i-3630, i-41ec, i-5087.
