---
format: openwarrant/1.0
profile: ticket
alias: t-bf2a
uuid: 01a1a54a-30b4-7630-991d-60aa33f47049
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Implement deterministic budgeting and required-section fail-closed behavior."
---
# Token budget, truncation and tokenizer abstraction

## Checklist

- [x] Tokenizer trait and default estimate with a version id (i-601d, after t-15fc) — done by claude, 2026-11-04
- [x] Priority-based inclusion and oldest-turn-first truncation (i-cd8d) — done by claude, 2026-11-04
- [x] Required-section fail-closed error (i-527c) — done by claude, 2026-11-04
- [x] Dropped/truncated metadata for the inspector (i-6e9a) — done by claude, 2026-11-04
- [x] Budget tests (i-b050) — done by claude, 2026-11-04
