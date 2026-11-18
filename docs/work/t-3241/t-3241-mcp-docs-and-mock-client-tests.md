---
format: openwarrant/1.0
profile: ticket
alias: t-3241
uuid: 01a1a54a-326c-7266-b5ef-3a9e6baa2d6d
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Document setup and pin both transports end to end."
---
# MCP docs and mock client tests

## Checklist

- [x] Claude Desktop/opencode setup docs (i-5094, after t-745d, t-6498, t-851f) — done by claude, 2026-11-04: docs/mcp.md and tests/mcp/{mcp_stdio_client,mcp_http_client}.js, run by scripts/check_mcp.sh.
- [x] Mock stdio client (i-a10b) — done by claude, 2026-11-04: docs/mcp.md and tests/mcp/{mcp_stdio_client,mcp_http_client}.js, run by scripts/check_mcp.sh.
- [x] Mock HTTP client (i-3190) — done by claude, 2026-11-04: docs/mcp.md and tests/mcp/{mcp_stdio_client,mcp_http_client}.js, run by scripts/check_mcp.sh.
- [x] Read/write/approval scenarios (i-3edc) — done by claude, 2026-11-04: docs/mcp.md and tests/mcp/{mcp_stdio_client,mcp_http_client}.js, run by scripts/check_mcp.sh.
- [x] One suite command (i-376f) — done by claude, 2026-11-04: docs/mcp.md and tests/mcp/{mcp_stdio_client,mcp_http_client}.js, run by scripts/check_mcp.sh.

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && ./scripts/check_mcp.sh
```

Result: MCP_STDIO_PASS and MCP_HTTP_PASS, then MCP_SUITE_PASS.
Covers i-5094, i-a10b, i-3190, i-3edc, i-376f.
