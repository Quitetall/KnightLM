---
format: openwarrant/1.0
profile: ticket
alias: t-1b87
uuid: 01a1a54a-3299-7206-be01-3f31d31abf9c
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Bring README/ROADMAP/MCP/study/context docs and packaging in line."
---
# Docs, installer and release checklist

## Checklist

- [x] README/ROADMAP updates (i-0bf2, after t-4158) — done by claude, 2026-11-04
- [x] MCP setup and context/study format docs (i-71ca) — done by claude, 2026-11-04: docs/mcp.md: Claude Desktop and opencode stdio configurations, the HTTP endpoint with its token and the literal curl call, the tool surface, the approval return shape and the poll, the resource uris, and the limits. Verified by scripts/check_mcp.sh, which drives both transports against a real server.
- [x] Migration/reset notice (i-f5ce) — done by claude, 2026-11-04
- [x] Installer/desktop bundle (i-5e1b) — done by claude, 2026-11-04
- [x] Operator signing checklist (i-7cb6) — done by claude, 2026-11-04

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && ./packaging/install.sh --prefix /mnt/4tb/tmp/knightlm-inst/prefix --skip-build && curl -s 127.0.0.1:8091/api/health
```

Result: The install places the binary and frontend, and the installed binary run against a brand-new database answers {"schema":6} and serves / and pkg/rbc-frontend_bg.wasm with 200 (GET / -> 200, wasm -> 200). The stale-binary guard was observed firing on the four-day-old release build, which had reported schema 3.
Covers i-0bf2, i-71ca, i-f5ce, i-5e1b, i-7cb6.
