---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a5f2-71d6-86c5-3e26a98aadef
role: work_order
jurisdiction: authored
order: 40
classification: internal
---

# Work Order

Deliverables (packages PKG-060..PKG-065):

1. Study tab/editor: chapter list, add/reorder/duplicate/delete, primary/additional board
   strip, board + move tree + arrows + annotations, Markdown editor/preview, metadata.
2. Publish/version/lineage/copy/lock: draft → published immutable version; new draft branched
   from a version; manual lock; `copy_required`; provenance.
3. `.rbcstudy` export/import (versioned, unknown fields preserved), PGN export, Lichess
   import adapter; round-trip tests.
4. Progress: schema/overlay, manual completion, last visited, placeholders for
   attempts/score/hints/mastery; badges in the tree; no content-hash impact.
5. AI/MCP authoring tools: create study/chapter/board, reorder, write Markdown, annotate,
   publish; changeset review; immutable/published protection.
6. Tests: CRUD/order, versions, round-trip, progress overlay, authoring changesets.

Seams:

- Boards remain the single content object; chapters reference board IDs.
- Compare (W-08) reads study/board revisions; it never mutates.
- Lua R2 adds script/state/actions without changing the R1 bundle envelope.

Autonomy and escalation:

Tier T2. Escalate changes to the study/chapter/board hierarchy, version lineage, or progress
semantics. The performer owns editor layout within the Study tab.

Rollback:

Studies are additive. If the editor is reverted, study rows/bundles remain readable; exported
bundles are self-contained.
