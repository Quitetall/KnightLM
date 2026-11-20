---
format: openwarrant/1.0
profile: ticket
alias: t-bb0e
uuid: 01a1a54a-3111-71f3-b286-f179130bc210
priority: 0
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Implement reviewable changesets and the async approval queue."
---
# Approval service: changesets, async jobs, policies and audit

## Checklist

- [x] Changeset model for study/board content (i-6839, after t-4836, t-4eb3) — done by claude, 2026-11-04
- [x] pending/approved/denied/expired lifecycle (i-95ef) — done by claude, 2026-11-04
- [x] Auto-approve scopes and audit (i-54e6) — done by claude, 2026-11-04
- [x] Async approval id for MCP polling (i-0364) — done by claude, 2026-11-04
- [x] Approval bypass tests (i-0eb4) — done by claude, 2026-11-04

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-ai --all-features approvals
```

Result: 12 of 12 tests pass (approvals binary). The --all-features flag is required because the queue lives behind the host cargo feature. Also verified live over HTTP: a chat turn asking to play e4 queued one pending request with its arguments and a 15-minute deadline, the board stayed at revision 1, approving set status approved with a timestamp, and approving again was refused with 'already resolved as Approved'.
Covers i-6839, i-95ef, i-54e6, i-0364, i-0eb4.
