---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a5da-7086-bc7a-30b39f6fad0c
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

### OBL-001: only model-versus-model games under the rated ruleset are rateable

- **scope:** games staged through `ratings::stage`. Not a claim that every game is rated.
- **evidence:** `crates/ai/tests/glicko.rs` and `tournament.rs` assert the rated-ruleset filter and the update.
- **gate:** gate://software.repo.war-check@1.0.0

### OBL-002: a tournament budget pauses the run rather than aborting it

- **scope:** runs driven through `Running`. 
- **evidence:** `crates/ai/tests/tournament.rs` covers the budget pause; `cdp_leaderboard.js` (`AI_LEADERBOARD_PASS`) drives step and pause.
- **gate:** gate://software.repo.war-check@1.0.0
## Residual Risk

See `90-risks.md` in the planning bundle. Residual risk is accepted by the program owner
until the release gate records otherwise.
