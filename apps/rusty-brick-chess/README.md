# RustyBrickChess

The full KnightLM chess app: a **Leptos/BrickChess** frontend, an
**En-Croissant-shaped Rust backend**, and **AI chess**.

```
┌──────────────────────────────────────────────────────────────┐
│  Leptos frontend (WASM)  — served at /                        │
│  BrickChess board + panels + (planned) game list / analysis   │
└───────────────┬──────────────────────────────────────────────┘
                │ HTTP/JSON
┌───────────────▼──────────────────────────────────────────────┐
│  axum backend                                                 │
│   /api/games      SQLite game store + PGN import              │
│   /api/analyze    Stockfish (STOCKFISH_PATH), typed Analysis  │
│   /api/report     brick-chess-analysis (labels + accuracy)    │
│   /api/chat       AI model proxy (MODEL_URL)                  │
└──────────────────────────────────────────────────────────────┘
```

## Run

```bash
# Stockfish is optional for the DB/board, required for /api/analyze.
STOCKFISH_PATH="$HOME/.local/bin/stockfish" \
MODEL_URL="http://127.0.0.1:8000/v1/chat" \   # optional; /api/chat returns 503 without it
RBC_DB="rusty-brick-chess.db" \
RBC_ADDR="127.0.0.1:8090" \
cargo run --release
```

Then open <http://127.0.0.1:8090/>.

Environment:

| var | meaning |
|---|---|
| `STOCKFISH_PATH` | UCI engine binary; no auto-download |
| `MODEL_URL` | AI chat endpoint (the fine-tuned model server) |
| `RBC_DB` | SQLite path (default `rusty-brick-chess.db`) |
| `RBC_ADDR` | bind address (default `127.0.0.1:8090`) |
| `RBC_DEPTH` | default analysis depth (15) |
| `RBC_DATA` | data directory for the vision model and uploaded sounds (default `db_dir/data`) |

## API

```bash
GET    /api/health
GET    /api/games?limit=50&player=&event=&collection_id=
POST   /api/games                  {"pgn": "..."}
POST   /api/import/lichess         {"username": "...", "max": 30}
POST   /api/import/chesscom        {"username": "...", "months": 1}
GET    /api/games/{id}             includes the full movetext tree (JSON)
PATCH  /api/games/{id}             editable PGN tags
POST   /api/games/{id}/moves       {"moves":"1. d4 d5 2. c4"}
POST   /api/games/{id}/moves/truncate {"ply": 2}
PUT    /api/games/{id}/tree        {"pgn":"1. e4 e5 (1... c5) 2. Nf3"}
POST   /api/games/{id}/tree/add    {"parent":"0","san":"e5","clock":"0:04:58"}
POST   /api/games/{id}/tree/delete {"path":"0~0.0"}
POST   /api/games/{id}/tree/promote {"path":"2~0.0"}
DELETE /api/games/{id}
GET    /api/games/{id}/pgn         PGN with variations/comments/NAGs
GET    /api/games/{id}/report      Stockfish over the whole game + labels
GET    /api/games/{id}/evals       cached per-ply evals (eval graph)
GET    /api/games/{id}/annotations
POST   /api/games/{id}/annotations {"ply":3,"path":"0.1","comment":"...","nag":1,"shapes":"[...]"}
POST   /api/analyze                {"fen": "...", "depth": 15}
GET    /api/analyze/lines?fen=...&depth=15&lines=3   MultiPV
GET    /api/analyze/engines?fen=...&depth=15&lines=3 all configured engines
GET    /api/openings?fen=...&source=local|lichess|masters
GET    /api/positions?fen=...&partial=true            absolute or partial search
GET    /api/tablebase?fen=...                         Syzygy WDL + best move
GET    /api/databases | POST /api/databases
POST   /api/databases/switch       {"name": "..."}
GET    /api/collections | POST /api/collections
PATCH  /api/collections/{id}       {"name": "..."}
DELETE /api/collections/{id}
POST   /api/collections/{id}/games/{game_id}
GET    /api/players | GET /api/events                 catalogs
GET    /api/stats                                     database statistics
GET    /api/repertoires | POST /api/repertoires
DELETE /api/repertoires/{id}
POST   /api/repertoires/{id}/import {"pgn": "..."}
POST   /api/repertoires/{id}/import-game/{game_id}
GET    /api/repertoires/{id}/next                     due card
POST   /api/repertoires/review     {"card_id":1,"correct":true}
GET    /api/puzzles/next?rating=1500&theme=fork
GET    /api/puzzles/{id}
GET    /api/puzzles/attempts?limit=10
GET    /api/puzzles/themes
POST   /api/puzzles/attempt        {"puzzle_id":"...","puzzle_rating":1500,"solved":true}
POST   /api/puzzles/import         {"csv": "PuzzleId,FEN,..."}
GET    /api/settings
PUT    /api/settings               engine/model/depth/theme/pieces/threads/hash/syzygy/engines/variant engine
POST   /api/report                 {"moves": [MoveInput...], "config": {...}}
POST   /api/chat                   {"message": "...", "fen": "..."}
GET    /api/vision                 model manifest, license, hash, install state
POST   /api/vision/download        download + SHA-256 check the optional model
POST   /api/vision/recognize       {"image":"data:image/png;base64,..."} -> FEN placement
GET    /api/sounds/custom/{name}   uploaded sound file
POST   /api/sounds/custom/{name}   {"data":"data:audio/ogg;base64,..."}
```

Settings are stored in SQLite and override the environment. Analyze, MultiPV,
the game report and the eval graph all use the saved depth; engine Threads,
Hash and SyzygyPath are applied when the engine is spawned. The online Lichess
explorer is opt-in (`source=lichess`) and cached in SQLite; on network failure
it falls back to the local database.

`MoveInput` (for `/api/report`):

```json
{"san":"e5","uci":"e7e5","eval_before":{"cp":0},"eval_after":{"cp":-300},"best_move":"g8f6"}
```

The report returns, per move, both a **Lichess-style** label and a
**clean-room Expected Points V2** label, plus win% and accuracy.

## Frontend (Leptos, no TS/React)

`frontend/` is a Leptos CSR app (`rbc-frontend`) with the En Croissant shell:

- sidebar: **Database, Analysis, Openings, Puzzles, Settings**
- **Database**: PGN import, search, game table; click a game to analyze it
- **Analysis**: BrickChess board + move list + engine panel (eval, best move,
  PV, nodes) via `POST /api/analyze`; **Lines** shows 3 MultiPV lines;
  **All engines** runs every configured engine and groups their lines;
  **Classify game** runs Stockfish over the whole game and reports Lichess
  labels + clean-room Expected Points V2, accuracy per side, a win%-sparkline
  and per-move labels
- **Play vs engine**: toggle in the engine panel, choose White or Black; the
  engine answers with its best move at the saved depth
- **Board editor**: place/erase pieces, set turn/castling/en passant and load
  the position into the analysis board
- **Tablebase**: probe the configured Syzygy directory for WDL and the
  recommended move
- **Clocks**: `[%clk]` comments from PGN are shown next to each move. Live
  clocks have Start/Pause/Reset, editable base+increment presets, increments
  applied on every move (human and engine), active-side highlight, low-time
  colour and flag detection. They start on the first move (or Start), pause on
  tree navigation, and **write `[%clk …]` comments back into the game PGN** when
  "Record moves" is enabled, so timed play is exportable and reviewable.
- **Variants**: rules-playable boards for every shakmaty variant, crazyhouse
  pockets/drops, variant-aware position indexing and engine analysis. Configure
  a **variant engine (Fairy-Stockfish)** in Settings; Chess960 also works with
  a standard engine (`UCI_Chess960` + king-to-rook UCI).
- **Analysis tree**: the full movetext tree — mainline plus **variations**,
  indented and numbered correctly (Black replies as `1…`), each node clickable,
  colored by classification and marked when annotated; clicking a variation
  loads that line on the board
- **Eval graph**: per-ply graph from the cached evals written by the game
  report (and updated live as you Analyze), White's POV, hover readout and
  click-to-jump
- **Annotations and shapes**: per-node comment + NAG (`!`, `?`, `!!`, `??`,
  `!?`, `?!`) and drawn arrows/circles (right-drag = arrow, right-click =
  circle; Shift/Alt/Ctrl pick red/blue/yellow), stored and reloaded with the
  game; the editor follows the tree
- **Game tags**: edit Event/Site/Date/Round/players/result/Elo/TimeControl
- **Board feel**: a piece follows the cursor at its own size while dragging,
  its source square is hidden, and legal targets stay highlighted for both
  drag and click-to-move; illegal drops snap back. Movement is **instant by
  default**; **Settings → Move animation** sets a slide time (0–400 ms) used
  for played moves, engine moves and forward history navigation. The
  Chess.com-style last-move squares follow the **currently viewed ply**, not
  just the final move, and disappear on Start.
- **Sounds**: Lichess AGPL theme packs (`sfx`, `piano`, `nes`, `futuristic`)
  for move/capture/check/checkmate/error/victory/draw/select, with
  enable/theme/volume settings and a **custom sound upload** (files named
  `Move.ogg`, `Capture.ogg`, … served from the data directory). Chess.com
  sounds are not bundled.
- **Image import (optional vision)**: Settings shows the NAKST YOLOv8m chess
  model card (AGPL-3.0, license + SHA-256) and downloads it on request into
  `data_dir/models`; the board editor's **Scan image** runs detection,
  picks the board among several, sanitises impossible placements, infers the
  side to move and opens the editable review board (with flip/rotate) before
  you apply the FEN. Manual editing remains the fallback.
- **Viewport**: the app shell fits the window; the board never requires page
  scrolling and has a **Size** slider (45–100%, capped by viewport height).
  Panels scroll internally where needed.
- **Move editing (tree)**: play a different move with **Record moves** on and
  it becomes a variation automatically; **delete from the selected node**
  promotes that move's first variation (standard GUI behaviour); **promote a
  variation to the mainline**; or edit the full PGN movetext including
  variations. Every line is validated move by move and
  positions/evals/annotations are rebuilt atomically
- **Database**: PGN import, **Lichess and Chess.com account import**.
  **All PGN variants are imported, playable and indexed**: standard, Chess960,
  atomic, antichess, crazyhouse (with pocket drops), horde, king of the hill,
  racing kings and three-check; unknown variants (e.g. bughouse) are still
  stored with their SAN/movetext/FEN and shown read-only. Variant positions are
  indexed under variant-qualified keys and searchable from the position box.
  Collection **rename/delete**, player search,
  **absolute and partial position search**, event filter, **collections**
  (create, filter, add game), player/event catalogs, **PGN export**, **delete**
  and **multiple database files** (create/switch from the Database header)
- **Openings**: CC0 Lichess ECO table (3,870 entries) + game explorer with
  W/D/L continuations from your database, the Lichess database or the Masters
  database; can explore the position currently on the analysis board
- **Puzzles**: Lichess puzzle CSV (CC0), seeded with a 200-puzzle sample,
  theme filter, nearest-rated selection, **play-out validation** (correct
  moves advance the line, the opponent replies, wrong moves reveal the
  solution), local Elo rating with one recorded attempt per try, solution
  reveal, retry, CSV import
- **AI commentary**: an ask box in Analysis sends the current FEN to
  `POST /api/chat` (the operator's model URL) and shows the reply
- **Repertoires**: create a White/Black repertoire, import a PGN with
  variations, **add the current analysis line or the whole game tree** to it,
  and train it with
  **SM-2 spaced repetition** (due counts, ease, lapses, correct/wrong feedback
  and scheduled reviews)
- **Statistics**: games, results, players, events, collections and puzzles for
  the current database
- **Settings**: engine binary, model URL, analysis depth, board theme, piece
  set, engine Threads/Hash, Syzygy path and a **multi-engine list**
  (add/remove/select), sound options, move-animation speed, the optional
  vision model and custom sound upload, all persisted in SQLite

Build and deploy:

```bash
cd frontend && ./deploy.sh
```

The server serves `frontend/public` at `/`.

## Backend status & En Croissant

The database layer is the ported module: **`crates/rbc-db`** — a Tauri-free,
SQLite-backed version of En Croissant's DB shape (games, collections,
annotations, position search, player/event catalogs) with no `AppState`,
`specta`, DuckDB or AIX dependency. PGN import uses `pgn-reader` + `shakmaty`;
positions are indexed per ply so position search is a SQL lookup. Opening names
come from the CC0 `lichess-org/chess-openings` tables (vendored); puzzles from
the CC0 Lichess puzzle CSV (a sample is seeded, full CSV import supported).

Engine analysis is `knightlm-engine` (`vampirc-uci`); classification is
`brick-chess-analysis`. The frontend speaks all of it over `/api`.

En Croissant's own backend (GPL-3.0) is Tauri-coupled (`AppState`,
`tauri::Emitter`, `specta::Type`) and DuckDB-backed. The planned lift, in order:

1. **`db/`** — collections, annotations, position search, PGN import
   (~3.3k lines) → strip Tauri/specta, expose over this axum API.
2. **`opening.rs`** — opening names/explorer.
3. **`puzzle.rs`** — Lichess puzzle DB (CC0).
4. **`game.rs` / engine tree** — analysis tree and engine lines.

Until then, this crate's `db.rs` is the compatible seam: same domain shape,
small enough to swap out.

## License

AGPL-3.0-only. Backend design inspired by En Croissant (GPL-3.0) and Lichess
(AGPL-3.0); move-classification thresholds and accuracy follow Lichess's
published model; Expected Points is an independent implementation of the
public model shape, not chess.com's proprietary code.
