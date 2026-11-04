---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a5c3-75e7-9d15-76e5c65935fa
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

### OBL-001: a model reply that is not a legal move is retried and then refused, with the board untouched

- **scope:** the play policy against a match. Not a claim about model strength.
- **evidence:** `a_model_reply_that_is_not_a_legal_move_is_never_recorded` in `crates/ai/tests/security.rs`.
- **gate:** gate://software.repo.war-check@1.0.0

### OBL-002: a match can be paused, stepped and resumed without losing its moves

- **scope:** matches driven through the API and the UI.
- **evidence:** `crates/ai/tests/match_suite.rs` and `cdp_ai_watch.js` (`AI_WATCH_PASS`).
- **gate:** gate://software.repo.war-check@1.0.0
## Residual Risk

See `90-risks.md` in the planning bundle. Residual risk is accepted by the program owner
until the release gate records otherwise.
