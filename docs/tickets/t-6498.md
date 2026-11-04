---
format: openwarrant/1.0
profile: ticket
alias: t-6498
uuid: 01a1a54a-3256-70e3-906b-e0d0d86f21e1
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Map the shared tool registry and expose read-only resources/prompts."
---
# MCP tools, resources and prompts

## Checklist

- [x] Read/write tool schemas (i-f9a3, after t-4836, t-745d) — done by claude, 2026-11-04: See the ticket note.
- [x] rbc:// resources with revisions (i-2159) — done by claude, 2026-11-04: See the ticket note.
- [x] Deterministic prompts (i-d8af) — done by claude, 2026-11-04: See the ticket note.
- [x] No Lua/app-lifecycle/chat tools (i-72e8) — done by claude, 2026-11-04: See the ticket note.
- [x] Conformance tests (i-9937) — done by claude, 2026-11-04: See the ticket note.

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-ai --all-features mcp
```

Result: 19 tests pass; resources/list, a board resource read at revision 1, and a rendered prompt verified live over POST /mcp.
Covers i-f9a3, i-2159, i-d8af, i-72e8, i-9937.
