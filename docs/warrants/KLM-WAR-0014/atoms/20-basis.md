---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a597-70da-9816-c5fd5c93349d
role: basis
jurisdiction: authored
order: 20
classification: internal
---

# Basis

- Current AI is `chat_with_tools` in `apps/rusty-brick-chess/src/api.rs`: one `MODEL_URL`,
  stateless, max 3 tool rounds, no streaming, no persistence.
- Decisions: OpenAI-compatible first; keyring keys; tools with effects/policies; board writes
  revision-checked; approvals per-turn changesets and async MCP jobs; SSE; full local history.
- `rbc-context` provides `CompiledContext`; `rbc-db` v4 provides storage; `knightlm-engine`
  and the database power tools.
- Schema v4 tables from W-01.
