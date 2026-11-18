---
format: openwarrant/1.0
profile: ticket
alias: t-2bbf
uuid: 01a1a54a-31ba-708f-bd53-3eeb1b384a21
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Implement the standard-v1 rating pool with an auditable ledger."
---
# Glicko-2, ledger, anchors and unrated rules

## Checklist

- [x] Glicko-2 implementation and rating periods (i-6744, after t-5364) — done by claude, 2026-11-04: See the ticket note: crates/ai/src/glicko.rs and ratings.rs plus rbc-db schema v7.
- [x] Before/after ledger and materialized current table (i-d098) — done by claude, 2026-11-04: See the ticket note: crates/ai/src/glicko.rs and ratings.rs plus rbc-db schema v7.
- [x] Anchors and unrated rules (i-18cc) — done by claude, 2026-11-04: See the ticket note: crates/ai/src/glicko.rs and ratings.rs plus rbc-db schema v7.
- [x] Rebuild from history (i-cdb2) — done by claude, 2026-11-04: See the ticket note: crates/ai/src/glicko.rs and ratings.rs plus rbc-db schema v7.
- [x] Known-vector tests (i-6808) — done by claude, 2026-11-04: See the ticket note: crates/ai/src/glicko.rs and ratings.rs plus rbc-db schema v7.

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-ai --all-features glicko
```

Result: 18 tests pass, including the paper's known vector.
Covers i-6744, i-d098, i-18cc, i-cdb2, i-6808.
