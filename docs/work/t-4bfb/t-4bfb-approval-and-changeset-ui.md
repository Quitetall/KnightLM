---
format: openwarrant/1.0
profile: ticket
alias: t-4bfb
uuid: 01a1a54a-315e-7795-bf42-6792deab3507
priority: 0
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Review, approve selected/all, deny and configure auto-approve."
---
# Approval and changeset UI

## Checklist

- [x] Pending approval queue (i-2734, after t-bb0e, t-2883) — done by claude, 2026-11-04
- [x] Changeset diff for chapters/boards/trees/notes (i-31a4) — done by claude, 2026-11-04
- [x] Approve all/selected, deny (i-da2b) — done by claude, 2026-11-04
- [x] Auto-approve scopes per workspace/origin (i-7d41) — done by claude, 2026-11-04
- [x] CDP: approval flow (i-9c89) — done by claude, 2026-11-04

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM/apps/rusty-brick-chess/frontend && cargo check --target wasm32-unknown-unknown
```

Result: passes, plus `frontend/tests/cdp/cdp_ai_approvals.js` -> `AI_APPROVALS_PASS`
against a clean database: the model lists boards, asks to play e4, one pending
row appears naming `play_move` with the diff `moves 0 -> 1 (e4)`, approving
answers `applied · played e4`, the row clears, and the board itself moved to
revision 2 with `{"moves":["e4"]}`. Approve-all is also wired. `frontend/tests/cdp/cdp_ai_scopes.js` -> `AI_SCOPES_PASS`: with `board-write`
granted from the panel the profile is stored as
`{"in_app":["board-write"],"mcp":[]}` and the next play request writes straight
through with no pending approval (board at revision 2).
Not built: per-row selection for bulk approve.
Covers i-2734, i-31a4, i-da2b, i-7d41, i-9c89.
