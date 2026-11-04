---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e8cb-70fa-8cfe-03c41eb0d4dd
role: work_order
jurisdiction: authored
order: 40
classification: internal
---

# Work Order

## Deliverables

1. `code/apps/viewer/` — Leptos 0.8 application (axum server side, WASM
   target) outside the current workspace root definition but sharing the
   same crates/ via a path dependency or workspace member decided at
   implementation time (sponsor informed either way).
2. Ingest: paste or drop a game in any supported notation; the server calls
   `nota-detect` and the right parser; an unparseable input renders an
   explicit error with position, never a misleading board.
3. Board surface: legal-move shaded board rendering over the shared crates;
   move list, navigation (click jump), annotated-comment display (from
   `nota-annot`), FEN display, X-FEN, and the packet facts panel (legal-move
   arms with per-arm packet presence checker).
4. Exports: every format the notation crate emits (PGN annotated, plain,
   X-FEN, Shredder-FEN, EPD, UCI move lists both flavors, long algebraic,
   ICCF, descriptive), one click each.
5. Analysis panel: SF19 fixed-depth evaluation via the engine-analysis seam
   (server side), clearly-labeled as empirical data; plus the model-move
   button calling the `serve-http` seam from KLM-WAR-0009.

## Seams and who depends on them

1. **SEAM `viewer-ingest`** — the HTTP route(s) accepting notation bytes and
   returning a render-ready game; schema versioned from day one.
2. Frozen consumed seams: `nota-*` (W-0002/0003/0004), `engine-analysis`
   (W-0005), `serve-http` (W-0009 — once that Warrant exists; until then the
   panel slot renders an "upstream pending" state — nothing hard-binds its
   schema).
3. Rust-only is not a preference: the Leptos/axum-only stack is KLM-SAS-RQ-008
   and its enforcement is an obligation below.

## Owner's space

Layout, styling, component structure, routing, state management patterns,
shortcuts, board-rendering strategy, accessibility choices. The team will
use this daily: taste belongs to the builder, informed by one usability
report from the whole team (see OBL-005).

## Premade instructions

- Every fixture in the Phase 2 battery must load: a viewer-side test list
  walks fixtures via HTTP and compares rendered-vs-expected board states.
- A confusing empty state is a defect: pasted-not-parsed shows why.

## Autonomy and escalation

Tier T2. The no-JS/no-registry rule is a sponsor rule (RQ-008): any tempted
dependency escalates BEFORE merging; a paved-road PR escape hatch exists:
crates/ (in-workspace) may not request it.

## Rollback

`code/apps/viewer/` is self-contained; deleting it leaves every crate
untouched. Fixture walk tests live inside this scope only.
