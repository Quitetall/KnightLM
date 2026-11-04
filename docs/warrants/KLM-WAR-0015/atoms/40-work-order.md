---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a5ae-75f9-b5f0-2bd8ee7eb34c
role: work_order
jurisdiction: authored
order: 40
classification: internal
---

# Work Order

Deliverables (packages PKG-030..PKG-037):

1. Assistant tab + workspace kind + session rail (new/rename/archive/search) + restore.
2. Center board + board strip + left-rail board tree + active/target markers + focus-board.
3. Transcript: streaming content, reasoning toggle, tool/retry/action cards, usage, errors.
4. Composer: slash commands, `@`-mentions, attachments (PGN/FEN/image), context chips, model
   picker, stop/steer/regenerate, command palette, keybinds.
5. Context inspector: sections, token counts, included/truncated, canonical prompt, hash,
   pin/live, replay/diff.
6. Provider settings: list/add/test, base URL, keyring, localhost scan, capability metadata.
7. Approval/changeset UI: pending queue, diff, approve all/selected, deny, auto-approve
   scopes.
8. i18n (en/es), a11y labels/live regions, empty/error states.

Seams:

- The tab reads `rbc-ai` protocol DTOs and SSE; no direct SQLite.
- Board actions from tool events go through the board repository API; no local mutation.
- The Analysis dock keeps working through `/api/chat` compatibility.

Autonomy and escalation:

Tier T2. Escalate changes to the three-pane layout, session/tab semantics, or context
inspector contract. The performer owns component structure and styling within the existing
design system.

Rollback:

The tab is additive; removing it leaves the old Analysis dock and `/api/chat` shim.
