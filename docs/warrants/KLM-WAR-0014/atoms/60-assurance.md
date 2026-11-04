---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a597-70da-9816-c5fd5c93349d
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

### OBL-001: a provider stream is parsed into events without dropping a delta

- **scope:** the OpenAI-compatible adapter against recorded chunks, including tool
  call accumulation and reasoning deltas.
- **evidence:** `crates/ai/tests/provider.rs` and `crates/ai/tests/stream.rs`.

### OBL-002: a tool that fails does not take the turn with it, and a retry is bounded

- **scope:** the agent loop's handling of tool errors and provider retries.
- **evidence:** `crates/ai/tests/agent_loop.rs`.

### OBL-003: a write the policy requires approval for is queued, not performed

- **scope:** the policy gate on board writes. It makes no claim about what happens
  after a person approves — that is `KLM-WAR-0014`'s approval path and is covered by
  the approvals obligation below.
- **evidence:** `crates/ai/tests/write_tools.rs`, and `frontend/tests/cdp/cdp_ai_approvals.js`
  (`AI_APPROVALS_PASS`) which shows one pending `play_move` row with the board untouched.

### OBL-004: an approval is answered once, expires, and cannot be replayed

- **scope:** resolution of a queued approval: not found, already resolved, expired.
- **evidence:** `crates/ai/tests/approvals.rs`.

### OBL-005: approving performs exactly the write that was reviewed

- **scope:** the replay of the stored call on approval. A refused write
  (conflict, copy required) leaving the approval approved is part of the claim.
- **evidence:** `frontend/tests/cdp/cdp_ai_approvals.js`: the board reaches revision 2
  with `{"moves":["e4"]}` after approving, and the row reads `moves 0 -> 1 (e4)` first.

### OBL-006: an unconfigured subsystem answers unavailable rather than guessing

- **scope:** read tools with no engine, no openings set, no database, no tablebase.
- **evidence:** `crates/ai/tests/read_tools.rs` asserting `result_code::UNAVAILABLE`.

## Residual Risk

See `90-risks.md` in the planning bundle. Residual risk is accepted by the program owner
until the release gate records otherwise.
