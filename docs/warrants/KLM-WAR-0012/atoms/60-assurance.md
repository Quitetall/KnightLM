---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a567-70df-bcf3-4af91672d90c
role: assurance
jurisdiction: authored
order: 60
classification: internal
---

# Assurance



## Gate Adequacy

Required at `basic`. **Adversarial question:** how could this look done and not be?
Each obligation pairs a positive claim with a refusal: a stale revision, a locked board, a
missing hash, an unapproved write, or a dropped required section. The package test in
`openwarrant/packages.json` is the executable gate; the obligation text is the contract.


## Obligations

### OBL-001: a v3 database moves its sessions aside and keeps its games

- **scope:** the `sessions` → `legacy_sessions` step of `migrate_v4`, applied to a
  database at version 3 that holds one session row. No claim about payloads written
  by a later version, and none about downgrading.
- **evidence:** `v4_moves_sessions_to_legacy_and_clears_active` in `crates/rbc-db/src/lib.rs`
  asserts the row arrives in `legacy_sessions` and the active table is cleared;
  `schema_is_versioned_and_sessions_round_trip` in `crates/rbc-db/tests/db.rs` asserts
  the version and that games still read back.

### OBL-002: a board write carries a revision, and a stale one is refused

- **scope:** writes through the repository with an `expected_revision`. Concurrent
  writes that omit a revision are outside this claim.
- **evidence:** `workspaces_hold_boards_and_revisions` (`crates/rbc-db/tests/db.rs`)
  shows the revision incrementing; `write_tools_play_move_reports_the_current_revision_on_conflict`
  (`crates/ai/tests/write_tools.rs`) shows the refusal naming the current revision
  rather than overwriting it.

### OBL-003: a locked or published board refuses a write and offers a copy

- **scope:** writes attempted against a board whose mutability is `locked` or
  `published`. Reading, exporting and copying are outside the claim.
- **evidence:** `locked_or_published_boards_require_a_copy` (`crates/rbc-db/tests/db.rs`).

### OBL-004: a board has one parent, and reparenting moves it

- **scope:** the one-parent rule at the repository. It says nothing about what the
  frontend does with a board whose parent changed underneath it.
- **evidence:** `workspaces_hold_boards_and_revisions` and the `reparent` path in
  `crates/rbc-db/src/domain.rs`, exercised through `POST /api/boards/{id}/reparent`.

### OBL-005: studies keep chapters, versions and progress apart

- **scope:** chapter ordering, version copies and per-chapter progress as the
  repository stores them. No claim about the editor UI.
- **evidence:** `studies_copy_chapters_versions_and_progress` (`crates/rbc-db/tests/db.rs`).

### OBL-006: the reset is announced once, and the old rows stay

- **scope:** the notice a person sees after the v4 reset, and retention of
  `legacy_sessions`. Nothing about an old payload's content beyond counting it.
- **evidence:** `frontend/tests/cdp/cdp_session.js` (`AI_SESSION_PASS`) seeds one
  `legacy_sessions` row, sees the banner on the first load and an empty banner on
  the next; `/api/session/notice` reports `{"reset":true,"legacy_sessions":1}`.

### OBL-007: a session payload above the limit is refused, not truncated

- **scope:** `PUT /api/session` with a payload over 2 MB is rejected; what the client
  does with the rejection is outside the claim.
- **evidence:** not yet demonstrated. The size guard exists in `put_session`, but no
  test drives the refusal, so this reads as unestablished until one does.

## Residual Risk

See `90-risks.md` in the planning bundle. Residual risk is accepted by the program owner
until the release gate records otherwise.
