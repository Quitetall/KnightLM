---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a580-74f0-be4c-a5ef4787ae41
role: basis
jurisdiction: authored
order: 20
classification: internal
---

# Basis

- `knightlm-core::state::ContextPacket` already provides a versioned canonical position
  packet (FEN, facts, legal moves, features). Reuse it.
- `knightlm-notation` provides SAN/PGN helpers.
- Decisions: default sections position/legal moves/ASCII; everything configurable; chat
  default board + sibling directory; compare scope adds a cross-workspace directory; rated
  mode pins sampling/tools/engine; deterministic input only.
- Interface fixed with `rbc-ai`: `ContextSpec`, `Evidence`, `Section`, `CompiledContext`.
