---
format: openwarrant/1.0
profile: ticket
alias: t-851f
uuid: 01a1a54a-3260-73c8-936a-2b30590e860f
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Gate writes and make every call auditable."
---
# MCP approval jobs, polling and audit

## Checklist

- [x] approval_required + polling (i-c14f, after t-bb0e, t-745d) — done by claude, 2026-11-04: See the ticket note.
- [x] approve/deny/expire (i-2b0c) — done by claude, 2026-11-04: See the ticket note.
- [x] auto-approve scopes (i-4229) — done by claude, 2026-11-04: See the ticket note.
- [x] Audit rows (i-30d5) — done by claude, 2026-11-04: See the ticket note.
- [x] Approval/audit tests (i-366e) — done by claude, 2026-11-04: See the ticket note.

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-ai --all-features mcp
```

Result: 23 MCP tests pass; both feature profiles green.
Covers i-c14f, i-2b0c, i-4229, i-30d5, i-366e.
