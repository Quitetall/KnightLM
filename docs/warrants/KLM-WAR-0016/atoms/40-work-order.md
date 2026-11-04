---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a5c3-75e7-9d15-76e5c65935fa
role: work_order
jurisdiction: authored
order: 40
classification: internal
---

# Work Order

Deliverables (packages PKG-040..PKG-044):

1. Match state machine + persistence: players, colours, clocks, retries, result/termination,
   move log, PGN.
2. Human vs AI: casual (chat, hints, takebacks, engine panel, reasoning visible) and
   competitive (move-only, no hints/takebacks, hidden reasoning, reveal after).
3. Watch: two model seats, start/pause/step/reset, per-side reasoning toggles, live moves.
4. Match UI: player cards, clocks, result, move list, controls, "Open in Analysis", "Save to
   database", "Export PGN".
5. Match tests with scripted mock models: legal, illegal, retry, timeout, provider error,
   resignation, checkmate, max plies.

Seams:

- Match writes use the same board write tools/revisions as chat.
- PGN export/import uses the existing notation crate.
- Results feed W-06 ratings only for rated model-vs-model standard-v1.

Autonomy and escalation:

Tier T2. Escalate changes to the failure policy, chat/reasoning rules, or clock semantics.
The performer owns the match UI layout within the Assistant panes.

Rollback:

Matches are stored; removing the UI leaves rows readable. No chess data is modified except
explicit "save to database".
