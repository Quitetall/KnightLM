---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a567-70df-bcf3-4af91672d90c
role: intent
jurisdiction: authored
order: 10
classification: internal
---

# Intent

Replace the v3 session JSON and one-board workspace model with the foundation every later
pillar needs:

- normalized workspace/board/study/chapter identity in SQLite schema v4;
- stable UUIDs, monotonic revisions, one parent per board, mutability/lifecycle;
- a repository API for create/read/update/copy/lock/publish/reparent;
- a frontend workspace model that holds multiple boards with one active per tab;
- reset of existing workspaces, with the legacy payload backed up for one release.

Out of scope: AI sessions, context compilation, studies UI, diff, MCP. This work order only
makes the chess/domain model correct.
