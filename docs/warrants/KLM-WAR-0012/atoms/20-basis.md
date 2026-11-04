---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a567-70df-bcf3-4af91672d90c
role: basis
jurisdiction: authored
order: 20
classification: internal
---

# Basis

- `crates/rbc-db` is at `SCHEMA_VERSION = 3`; sessions are a JSON payload keyed by client id.
- `frontend/src/model.rs` has `Workspace { key, kind, pgn, moves, ... }`; one board per
  workspace, integer keys from `ws_seq`.
- `frontend/src/lib.rs` restores/ persists the session payload and renders workspaces.
- Decisions: stable UUIDs, multiple boards per workspace, one parent, revisions, optimistic
  concurrency, manual lock + study lifecycle, schema v4 resets workspaces, normalized
  identity + versioned JSON content.
- OpenWarrant root is `/home/brianklam/Projects/KnightLM`; code is `code/` → `/mnt/4tb/KnightLM`.
