# Game and Board types: audit

There is one canonical game type, `knightlm_core::state::GameRecord` (initial FEN, UCI move list, variant,
`history_complete`; `deny_unknown_fields`, the packet contract). Every other `Game`/`Board` type below was read
against it. None is a duplicate of it, so none was collapsed. Each carries a shape fixed by a DB schema, an HTTP
API, a canonical-bytes rule or a different perspective.

Rule for new code: if you need to replay or compile a game, build a `GameRecord` at the edge and use `GameState`.
Do not add a new struct that holds `initial_fen` plus a move list.

## Game-like types

| Type | Where | Class | Why it stays |
|---|---|---|---|
| `GameRecord` | core `state.rs` | canonical | UCI moves from an initial FEN; the model/packet input |
| `rbc_db::Game` | rbc-db `lib.rs` | DB row + API DTO | `games` table row: SAN movetext string, tags, elo, `movetext` tree, `added_at`, serialized straight to the HTTP API and frontend |
| `rbc_db::ArchiveGame` | rbc-db `lib.rs` | ingest input | the pre-insert form of `Game` (no id, ply count, timestamp); different lifecycle stage, not a copy |
| `PgnGame` | notation `pgn.rs` | parse tree | PGN tags plus a variation tree; `reader.rs`/`uci.rs` already convert it to `GameRecord` at the edge |
| `context::DatabaseGame` | context `evidence.rs` | wire DTO | evidence canonical bytes (`normalized()`, `#[serde(default)]`); changing it changes evidence digests |
| `ai::reads::GameSample` | ai `reads.rs` | trait result | read-seam result type with `year` and moves; not serialized |
| `ai::tournament::Game` | ai `tournament.rs` | pairing | white/black/outcome inside a `Round`; serialized in tournament storage |
| `ai::glicko::Game` | ai `glicko.rs` | rating input | one game from one player's side (opponent rating, score) |
| `ai::ratings::RatingGame` | ai `ratings.rs` | staging input | period, colours, kind, score for the pool |
| `rbc_db::rating::RatedGame` | rbc-db `rating.rs` | DB row | per-competitor ledger row |
| `brick_chess_analysis::GameReport` | brick-chess-analysis | report | analysis output |
| frontend `GameSummary`/`GameDetail` | apps/.../frontend `api.rs` | wire mirror | client-side mirror of `rbc_db::Game`'s JSON; wasm crate cannot depend on rbc-db (excluded from workspace) |

The rating/tournament family (`tournament::Game`, `glicko::Game`, `RatingGame`, `RatedGame`) share a name but
describe results from different viewpoints, not move lists. Unifying them is a separate design question and
would touch stored rows.

## Board-like types

| Type | Where | Class | Why it stays |
|---|---|---|---|
| `rbc_db::domain::Board` | rbc-db `domain.rs` | DB row + API DTO | `boards` table row: parent, revision, mutability, lifecycle, `content_json` |
| `ai::board::BoardContent` | ai `board.rs` | stored JSON body | the parsed form of `Board.content_json`; preserves unknown fields via `#[serde(flatten)]` |
| frontend `BoardInfo`/`BoardRef` | apps/.../frontend | wire mirror | mirrors `Board`'s JSON for the wasm client |
| `context::BoardSummary` | context `evidence.rs` | wire DTO | compact sibling reference inside evidence canonical bytes |
| `ai::study_bundle::BundleBoard` | ai `study_bundle.rs` | import/export | study bundle schema |
| `BoardObservation` | core `state.rs` | image input | placement field only; vision contract |

`Board` + `BoardContent` is one layering (row plus its JSON body), not a duplicate.

## Left as is, and what would be needed to change it

- Collapsing `rbc_db::Game` onto `GameRecord` would change the HTTP API, the `games` schema and the frontend
  mirror (SAN string vs UCI list, tags, ids). Not done.
- Sharing `Game`/`Board` with the wasm frontend needs the frontend to depend on rbc-db or a new wire-types crate;
  the app crates are outside the workspace on purpose. Not done.
- `context::DatabaseGame` and `ai::reads::GameSample` overlap in four fields, but the first is digest-bearing.
  A `From<GameSample>` at the edge is the right link if one is wanted; no behaviour change is made here.
- The start-position `GameRecord { .. }` literal is repeated (cli/play `uci.rs`, `match_driver.rs`, `score.rs`,
  brick-chess `widget.rs`, ai `play.rs`); each sets its own `game_id`/`source`, which appear in packets. A shared
  constructor is possible but is a core API addition, not a type collapse.
