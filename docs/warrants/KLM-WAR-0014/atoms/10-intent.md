---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a597-70da-9816-c5fd5c93349d
role: intent
jurisdiction: authored
order: 10
classification: internal
---

# Intent

Build `rbc-ai`, the domain and host layer behind the Assistant. It owns sessions and turns,
the tool registry, provider adapters, the agent loop, SSE events and the approval service.
The frontend and MCP consume this crate; neither reimplements it.

Out of scope: the Assistant UI, match/tournament UI, study editor, MCP transport. Those
consume this runtime.
