---
format: openwarrant/1.0
profile: ticket
alias: t-b1ab
uuid: 01a1a54a-30e0-772a-a5bc-277d5d70a561
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Bind the read-only chess tools to existing engine/database services."
---
# Read tools: position, engine, openings, database, tablebase

## Checklist

- [x] get_position, get_history, legal_moves (i-5a97, after t-4836) — done by claude, 2026-11-04
- [x] analyze/evaluate with engine pin (i-a222) — done by claude, 2026-11-04
- [x] opening_explorer, search_database, tablebase (i-3fd9) — done by claude, 2026-11-04
- [x] Canonical tool result formatting (i-2f1a) — done by claude, 2026-11-04
- [x] Integration tests with a fake engine/database (i-28c4) — done by claude, 2026-11-04

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-ai --all-features read_tools
```

Result: 13 of 13 integration tests pass (read_tools binary). The --all-features flag is required because the read host lives behind the host cargo feature.
Covers i-5a97, i-a222, i-3fd9, i-2f1a, i-28c4.
