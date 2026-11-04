---
format: openwarrant/1.0
profile: ticket
alias: t-9c43
uuid: 01a1a54a-308e-7244-af63-c971d182d919
priority: 0
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Persist the new multi-board session, restore it, and show the one-time reset notice."
---
# Session persistence v4 and reset notice

## Checklist

- [x] Server session payload for workspaces/boards/active board (i-8df1, after t-312c, t-6c55) — done by claude, 2026-11-04
- [x] Restore on reload and after clearing localStorage (i-0831) — done by claude, 2026-11-04
- [x] One-time reset notice; legacy_sessions retained for one release (i-ba24) — done by claude, 2026-11-04
- [x] Session/CDP regression tests (i-fab6) — done by claude, 2026-11-04

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM/apps/rusty-brick-chess/frontend && cargo check --target wasm32-unknown-unknown
```

Result: passes. Browser: frontend/tests/cdp/cdp_session.js -> AI_SESSION_PASS - two workspace tabs created through the strip survive a reload, still survive after localStorage.clear() (the client id is mirrored in an rbc_client cookie and restored into localStorage), and the reset notice appears once then stays away (measured against a database seeded with one legacy_sessions row).
Covers i-8df1, i-0831, i-ba24, i-fab6.
