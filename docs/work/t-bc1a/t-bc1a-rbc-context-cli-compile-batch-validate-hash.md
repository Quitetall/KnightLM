---
format: openwarrant/1.0
profile: ticket
alias: t-bc1a
uuid: 01a1a54a-30bc-77cc-b52f-1799f7753965
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Expose the compiler headlessly with JSON/JSONL and optional target/eval fields."
---
# rbc-context CLI: compile, batch, validate, hash

## Checklist

- [x] compile from FEN/PGN/ply (i-d0ab, after t-6abc, t-dfbc, t-bf2a) — done by claude, 2026-11-04
- [x] batch positions.jsonl → contexts.jsonl (i-fef1) — done by claude, 2026-11-04
- [x] validate spec and hash a compiled context (i-749f) — done by claude, 2026-11-04
- [x] Preserve line order; report invalid lines; never invent targets (i-f2da) — done by claude, 2026-11-04
- [x] CLI round-trip tests (i-2312) — done by claude, 2026-11-04
