---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a609-71e2-b292-3cbf99b4cb8d
role: work_order
jurisdiction: authored
order: 40
classification: internal
---

# Work Order

Deliverables (packages PKG-070..PKG-074):

1. Compare workspace + selection UI (studies/workspaces), compact directory for the Assistant,
   scope chip, clear.
2. Structural diff: chapter alignment by id/ordinal/title; board alignment by id/title/FEN;
   move-tree, annotation, shape, note/text and metadata diffs; progress badges.
3. Diff UI: side-by-side boards, synchronized ply, change list, filters, click-to-focus, AI
   summary/chat (read-only).
4. Export diff as Markdown/JSON; forward-compatible Lua/progress fields; no mutation.
5. Tests: alignment, tree/shape/note diffs, progress overlay, export, read-only.

Seams:

- Compare consumes study/board revisions; it never writes.
- The Assistant's compare scope uses the same selection and directory.
- Lua R2 populates the reserved diff fields; the R1 envelope must not need a rewrite.

Autonomy and escalation:

Tier T2. Escalate changes to the diff change schema or compare scope semantics. The performer
owns the compare layout and navigation.

Rollback:

Additive workspace kind; removing it leaves study/board data untouched.
