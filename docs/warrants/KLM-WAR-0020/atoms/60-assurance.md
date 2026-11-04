---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a622-7461-b929-a2e9a1941af0
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

### OBL-001: a write over MCP asks for approval and runs nothing until a person answers

- **scope:** the MCP write path with a default profile.
- **evidence:** `mcp_a_write_without_a_grant_asks_for_approval_and_runs_nothing` in `crates/ai/tests/mcp.rs` asserts the host was never called.
- **gate:** gate://software.repo.war-check@1.0.0

### OBL-002: no request without the bearer token is dispatched, and rotation invalidates the old one

- **scope:** the HTTP transport. Not a claim about TLS, which the server does not offer.
- **evidence:** `mcp_http_client.js` (run by `scripts/check_mcp.sh`) asserts 401 without a token, 401 for a wrong one, and 401 after rotation.
- **gate:** gate://software.repo.war-check@1.0.0

### OBL-003: an rbc:// path cannot escape into a neighbouring resource

- **scope:** the resource matcher.
- **evidence:** `a_resource_path_cannot_escape_into_another_resource` in `apps/rusty-brick-chess/src/mcp.rs`.
- **gate:** gate://software.repo.war-check@1.0.0
## Residual Risk

See `90-risks.md` in the planning bundle. Residual risk is accepted by the program owner
until the release gate records otherwise.
