---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a63a-7567-ab29-0a9330bca269
role: assurance
jurisdiction: authored
order: 60
classification: internal
---

# Assurance



## Gate Adequacy

Required at `basic`. **Adversarial question:** how could this look done and not be?
Each obligation pairs a positive claim with a refusal: a stale revision, a locked board, a
missing hash, an unapproved write, or a dropped required section. The package test in
`openwarrant/packages.json` is the executable gate; the obligation text is the contract.

## Obligations

### OBL-001: no credential is persisted, logged or placed in a stored turn

- **scope:** the provider configuration and the turn store.
- **evidence:** `the_key_never_appears_in_an_error` and `the_key_travels_in_the_authorization_header_and_not_in_the_body` in `crates/ai/tests/security.rs`.
- **gate:** gate://software.repo.war-check@1.0.0

### OBL-002: one command runs fmt, clippy, both test profiles and every browser suite

- **scope:** `scripts/check_all.sh`. Not a claim that every suite passes today: `cdp_match_ui` is red.
- **evidence:** the script stops at the first failure and reports the failing suite by name.
- **gate:** gate://software.repo.war-check@1.0.0
## Residual Risk

See `90-risks.md` in the planning bundle. Residual risk is accepted by the program owner
until the release gate records otherwise.
