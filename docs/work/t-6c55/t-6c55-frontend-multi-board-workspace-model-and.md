---
format: openwarrant/1.0
profile: ticket
alias: t-6c55
uuid: 01a1a54a-3086-7179-9bfe-5dba7acbba38
priority: 0
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Replace one-board workspaces with a board list, active board, board strip and left-rail tree."
---
# Frontend multi-board workspace model and navigation

## Checklist

- [x] Workspace state: board ids, active board, board metadata (i-3955, after t-312c) — done by claude, 2026-11-04
- [x] Board strip above the board and board list in the left rail (i-dd02) — done by claude, 2026-11-04
- [x] Create/copy/lock/reparent actions wired to the repository API (i-a52a) — done by claude, 2026-11-04
- [x] Persist and restore the active board and workspace (i-5f8c) — done by claude, 2026-11-04
- [x] Frontend check and layout/CDP regression (i-9cc5) — done by claude, 2026-11-04

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM/apps/rusty-brick-chess/frontend && cargo check --target wasm32-unknown-unknown
```

Result: passes (also run through the OpenWarrant root's `code/` symlink).

The CDP half of i-9cc5 was run after `./deploy.sh`:
`node cdp_layout.js` -> `LAYOUT_PASS` (board geometry and no document overflow
on the Openings, Analysis, small-window and Puzzle layouts with the new strip
in place), `cdp_i18n_a11y.js` -> `I18N_A11Y_PASS`,
`cdp_i18n_dynamic.js` -> `I18N_DYNAMIC_PASS`.
Covers i-3955, i-dd02, i-a52a, i-5f8c, i-9cc5.
