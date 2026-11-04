---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a622-7461-b929-a2e9a1941af0
role: intent
jurisdiction: authored
order: 10
classification: internal
---

# Intent

Expose RustyBrickChess to external agents through MCP. R1 is a server: stdio for child-process
clients, localhost Streamable HTTP with a bearer token, the same chess/study tools as the
in-app AI, read-only resources, deterministic prompts, and an async approval queue for writes.

Out of scope: MCP client, subscriptions, sampling, elicitation, Lua execution, AI chat
session access, app lifecycle/settings tools.
