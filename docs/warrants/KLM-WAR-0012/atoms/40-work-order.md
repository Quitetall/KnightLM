---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a567-70df-bcf3-4af91672d90c
role: work_order
jurisdiction: authored
order: 40
classification: internal
---

# Work Order

Deliverables (packages PKG-001..PKG-006):

1. v4 tables: workspaces, boards, board_revisions, studies, study_chapters, study_versions,
   study_progress, legacy_sessions. Bump `SCHEMA_VERSION` to 4; `/api/health` reports it.
2. v4 migration: create tables, copy old session payloads into `legacy_sessions`, clear the
   active session rows, leave chess game tables untouched.
3. Repository API: board/workspace CRUD, UUIDs, revision increments, expected-revision
   writes, conflict error, copy, lock/unlock, reparent, one-parent enforcement; study/chapter
   CRUD, ordering, versions, publish, copy, progress.
4. Frontend multi-board model: workspace → board list, active board, board strip, left-rail
   board list; old `Workspace.pgn` path replaced by board content; session restore from the
   server.
5. Reset notice: one-time UI explaining that open workspaces reset and the chess database is
   untouched; `legacy_sessions` retained for one release.
6. Fixtures/tests: v3→v4 migration, reset, repository, revision conflict, lock/copy,
   one-parent, frontend restore.

Seams:

- `rbc-db` repository traits are the only write path for boards/studies.
- `rbc-ai`, MCP and diff consume board/study IDs and revisions; they never write session JSON.
- The frontend board model is the only place that knows about the active board per tab.

Autonomy and escalation:

Tier T2. Escalate any change to the one-parent rule, the reset decision, or the ID/revision
contract. Implementation choices (UUID version, table column order, JSON content shape) are
the performer's.

Rollback:

Keep the v3 tables and `legacy_sessions`; a downgrade path is not promised, but no chess game
rows are migrated or deleted. If the frontend model fails, the old session payload remains in
`legacy_sessions` for recovery.
