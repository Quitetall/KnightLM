---
format: openwarrant/1.0
profile: ticket
alias: t-7a66
uuid: 01a1a54a-311a-7369-b2b8-a9951cb6b024
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Script streaming, tool calls, illegal moves, timeouts and approvals without network."
---
# Mock provider and runtime integration tests

## Checklist

- [x] Mock OpenAI-compatible server (i-74ed, after t-6584, t-7e92, t-d0bf, t-bb0e) — done by claude, 2026-11-04: Covered in crates/ai/tests/common (MockServer) and tests/runtime_mock.rs.
- [x] Scripted legal/illegal/timeout/429 responses (i-92a0) — done by claude, 2026-11-04: Covered in crates/ai/tests/common (MockServer) and tests/runtime_mock.rs.
- [x] Approval and conflict scenarios (i-d14c) — done by claude, 2026-11-04: Covered in crates/ai/tests/common (MockServer) and tests/runtime_mock.rs.
- [x] No live network in tests (i-b529) — done by claude, 2026-11-04: Covered in crates/ai/tests/common (MockServer) and tests/runtime_mock.rs.

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-ai --all-features runtime_mock
```

Result: 10 of 10 pass in the runtime_mock binary.
Covers i-74ed, i-92a0, i-d14c, i-b529.
