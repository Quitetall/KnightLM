---
format: openwarrant/1.0
profile: ticket
alias: t-312c
uuid: 01a1a54a-3075-7231-811c-3b874b34df07
priority: 0
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Expose UUID, revision, parent, mutability and copy operations for boards/workspaces."
---
# Board and workspace repository API

## Checklist

- [x] Board/workspace CRUD with stable UUIDs and monotonic revisions (i-59c5, after t-e7a7) — done by claude, 2026-11-04
- [x] Expected-revision writes with a typed conflict error (i-b438) — done by claude, 2026-11-04
- [x] Lock/unlock, copy with provenance, reparent, one-parent enforcement (i-7e12) — done by claude, 2026-11-04
- [x] Archive/delete semantics with no orphan boards (i-fc63) — done by claude, 2026-11-04
- [x] Repository tests for conflict, copy, lock and parent rules (i-76e7) — done by claude, 2026-11-04
