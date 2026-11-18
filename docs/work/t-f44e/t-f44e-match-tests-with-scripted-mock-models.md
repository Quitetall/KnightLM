---
format: openwarrant/1.0
profile: ticket
alias: t-f44e
uuid: 01a1a54a-3199-7521-aa23-b7346ac08579
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Pin legal/illegal/timeout/provider-error/resign/checkmate flows."
---
# Match tests with scripted mock models

## Checklist

- [x] Scripted move sequences (i-f3f8, after t-5364, t-bf5d, t-1315, t-f26f) — done by claude, 2026-11-04: Covered in crates/ai/tests/match_suite.rs.
- [x] Illegal retry then success/forfeit (i-c8d3) — done by claude, 2026-11-04: Covered in crates/ai/tests/match_suite.rs.
- [x] Timeout and provider error unrated (i-8258) — done by claude, 2026-11-04: Covered in crates/ai/tests/match_suite.rs.
- [x] Resign/checkmate/max plies (i-dfad) — done by claude, 2026-11-04: Covered in crates/ai/tests/match_suite.rs.
- [x] One test command for the suite (i-88cb) — done by claude, 2026-11-04: Covered in crates/ai/tests/match_suite.rs.

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-ai --all-features match_suite
```

Result: 9 of 9 pass in the match_suite binary; the whole AI suite is 225 tests with 'cargo test -p rbc-ai --all-features'.
Covers i-f3f8, i-c8d3, i-8258, i-dfad, i-88cb.
