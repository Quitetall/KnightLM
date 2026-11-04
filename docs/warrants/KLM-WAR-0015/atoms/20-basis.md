---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a5ae-75f9-b5f0-2bd8ee7eb34c
role: basis
jurisdiction: authored
order: 20
classification: internal
---

# Basis

- Frontend is already split: `lib.rs`, `model.rs`, `views/`; use the `AppCtx` pattern.
- Decisions: `Assistant` sidebar tab; workspace kind `assistant`; one session rail; extra
  tabs can pin sessions; left rail/center board/right chat; board strip + left-rail boards;
  context inspector with hash/replay; provider settings; keyring.
- `rbc-ai` protocol + SSE + approvals from W-03.
