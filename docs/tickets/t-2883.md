---
format: openwarrant/1.0
profile: ticket
alias: t-2883
uuid: 01a1a54a-3125-76bd-8a12-3f0211313081
priority: 0
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Add the Assistant workspace kind, session list and restore."
---
# Assistant tab, session rail and persistence

## Checklist

- [x] Assistant sidebar tab and workspace kind (i-1fac, after t-6c55, t-e833, t-d0bf) — done by claude, 2026-11-04
- [x] Session rail: new/rename/archive/search (i-7e73) — done by claude, 2026-11-04
- [x] Open/close tabs, pin a session to an extra tab (i-8cc7) — done by claude, 2026-11-04
- [x] Restore active session and transcript (i-78ae) — done by claude, 2026-11-04
- [x] CDP: tab/session/restore (i-4f4e) — done by claude, 2026-11-04

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM/apps/rusty-brick-chess/frontend && cargo check --target wasm32-unknown-unknown
```

Result: passes; plus two Chrome checks against the deployed app: /tmp/cdp_ai_rail.js -> AI_RAIL_PASS (Assistant tab opens, New chat creates a session, the search box filters the rail 1->0->1, the open session survives a page reload) and /tmp/cdp_ai_rail2.js -> AI_RAIL2_PASS (rename saves into the rail, archive removes exactly one row). Pin-a-session-to-an-extra-tab (i-8cc7) is implemented and checked: /tmp/cdp_ai_pin.js
-> AI_PIN_PASS (pinning opens a workspace tab and keeps the Assistant view;
pinning the same conversation again focuses that tab rather than opening a
second one). The three browser checks are committed at
apps/rusty-brick-chess/frontend/tests/cdp/ with a README.
Covers i-1fac, i-7e73, i-8cc7, i-78ae, i-4f4e.
