---
format: openwarrant/1.0
profile: ticket
alias: t-15fc
uuid: 01a1a54a-309d-7601-b8c4-b69c2148a79d
priority: 0
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Create the pure context compiler with versioned sections, canonical rendering and SHA-256."
---
# rbc-context crate, sections, canonical JSON and hash

## Checklist

- [x] Crate skeleton, Section/ContextSpec/CompiledContext types (i-ab0c, after t-312c) — done by claude, 2026-11-04
- [x] Canonical JSON and rendered prompt with fixed ordering (i-90e5) — done by claude, 2026-11-04
- [x] SHA-256 over canonical bytes; spec/template versions (i-0596) — done by claude, 2026-11-04
- [x] No HTTP/DB/engine/clock dependencies (i-c98b) — done by claude, 2026-11-04
- [x] Unit tests for ordering and hashing (i-e35d) — done by claude, 2026-11-04
