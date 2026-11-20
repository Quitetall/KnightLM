---
format: openwarrant/1.0
profile: ticket
alias: t-69c9
uuid: 01a1a54a-3284-771f-a1d8-f8a2dcf49cd8
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Secrets, permissions, approvals and MCP auth."
---
# Security review

## Checklist

- [x] Keyring/redaction audit (i-7ba0, after t-bb0e, t-851f) — done by claude, 2026-11-04: Keyring/redaction audit: no credential is persisted anywhere. api_key lives in ProviderConfig in memory and is used only to build the Authorization header; every set_setting call site was read and none writes a token; the only 'token' in rbc-db is an SAN token in update_moves. Redaction is asserted by the_key_never_appears_in_an_error (including Debug, which is what a log writes) and the_key_travels_in_the_authorization_header_and_not_in_the_body, which drives a real HTTP request at the mock server. See docs/security.md.
- [x] No key in DB/log/context/fixture (i-ac0a) — done by claude, 2026-11-04: See docs/security.md and crates/ai/tests/security.rs.
- [x] Permission/approval bypass tests (i-60af) — done by claude, 2026-11-04: See docs/security.md and crates/ai/tests/security.rs.
- [x] MCP auth/resource traversal (i-23d6) — done by claude, 2026-11-04: Five app tests: token matching refuses missing, empty, scheme-only, short, long and near-miss; rbc:// traversal attempts (../, absolute paths, file://, foreign schemes, over-long shapes) resolve to nothing while a real board uri still reads; the rate limiter refuses past its cap. Documented in docs/security.md.
- [x] Destructive action review (i-5896) — done by claude, 2026-11-04: Reviewed every path that changes stored content: board writes are policy-gated then revision-checked; MCP writes return approval_required and run nothing; study content needs a scope never granted to MCP by default; locked/published boards refuse and offer a copy; an approval answers once. No Destructive tool exists yet, said plainly rather than counted as a pass. Table in docs/security.md.

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-ai --all-features security
```

Result: 10 of 10 pass in the security binary; docs/security.md records the audit and names what is unverified.
Covers i-7ba0, i-ac0a, i-60af, i-23d6, i-5896.
