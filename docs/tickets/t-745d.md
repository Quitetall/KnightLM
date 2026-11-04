---
format: openwarrant/1.0
profile: ticket
alias: t-745d
uuid: 01a1a54a-324a-76bd-8416-88b1212f29a4
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Expose RBC over MCP with local transports and auth."
---
# MCP server: stdio, HTTP, token auth and lifecycle

## Checklist

- [x] stdio server (i-c129, after t-4836) — done by claude, 2026-11-04: See the ticket note.
- [x] Streamable HTTP on 127.0.0.1 (i-55f7) — done by claude, 2026-11-04: See the ticket note.
- [x] Bearer token generate/rotate (i-589b) — done by claude, 2026-11-04: See the ticket note.
- [x] Connection registry, rate limit, max size (i-758c) — done by claude, 2026-11-04: See the ticket note.
- [x] Transport tests (i-1d22) — done by claude, 2026-11-04: See the ticket note.

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-ai --all-features mcp
```

Result: 13 protocol tests pass; transports verified live: no token 401, initialize and tools/list (30 tools), tools/call runs the shared host, a notification answers 202, rotation leaves the old token 401 and the new one 200, 240 requests then 429, and the stdio server answers over a pipe.
Covers i-c129, i-55f7, i-589b, i-758c, i-1d22.
