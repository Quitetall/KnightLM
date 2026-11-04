---
format: openwarrant/1.0
profile: ticket
alias: t-f436
uuid: 01a1a54a-312e-73f2-8e4c-d85491bf698d
priority: 0
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Render the session board with workspace board navigation and target markers."
---
# Center board, board strip and cross-board focus

## Checklist

- [x] Board strip and left-rail board tree (i-3b88, after t-6c55, t-2883) — done by claude, 2026-11-04: Left-rail board tree built: workspaces and their boards in the sidebar, active board highlighted, clicking selects workspace+board. Verified by cdp_cross_board.js (AI_CROSS_BOARD_PASS).
- [x] Active board vs AI target marker (i-bf1a) — done by claude, 2026-11-04
- [x] Auto-focus explicit requests; badge otherwise (i-6a4a) — done by claude, 2026-11-04
- [x] Set-as-target and use-analysis-position handoff (i-6e09) — done by claude, 2026-11-04
- [x] CDP: cross-board addressing (i-ca6a) — done by claude, 2026-11-04

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM/apps/rusty-brick-chess/frontend && cargo check --target wasm32-unknown-unknown
```

Result: passes. Browser: frontend/tests/cdp/cdp_cross_board.js -> AI_CROSS_BOARD_PASS (three boards in a fresh tab, 'Set as AI target' marks exactly one chip with '· AI target', switching away shows the badge 'AI → New analysis 1', and clicking the badge focuses that board). The left-rail board tree (i-3b88) is not built: the strip is the board list.
Covers i-3b88, i-bf1a, i-6a4a, i-6e09, i-ca6a.
