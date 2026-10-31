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

## Install (desktop launcher)

```bash
./install.sh
```

The wizard builds the release binary, installs `rustybrickchess` under
`~/.local/bin`, copies the icon and writes a **RustyBrickChess** entry to your
application menu. Clicking it (or running `rustybrickchess --open`) starts the
server if needed and opens the app in your default browser; a second click
reuses the running instance.

```bash
./install.sh --yes                 # accept defaults, no prompts
./install.sh --prefix /usr/local   # install elsewhere
./install.sh --port 8095           # different default port
./install.sh --no-desktop          # binary only
./install.sh --uninstall           # remove launcher, keep your games
./install.sh --uninstall --purge   # remove launcher and data
```

By default the database lives in `$XDG_DATA_HOME/rustybrickchess`
(`~/.local/share/rustybrickchess`), so the launcher works from any directory.

Environment:

| var | meaning |
|---|---|
| `STOCKFISH_PATH` | UCI engine binary; no auto-download |
| `MODEL_URL` | AI chat endpoint (the fine-tuned model server) |
| `RBC_DB` | SQLite path (default `$XDG_DATA_HOME/rustybrickchess/rusty-brick-chess.db`) |
| `RBC_ADDR` | bind address (default `127.0.0.1:8090`) |
| `RBC_DEPTH` | default analysis depth (15) |
| `RBC_DATA` | data directory for the vision model and uploaded sounds (default `db_dir/data`) |

## API

```bash
GET    /api/health
GET    /api/games?limit=50&player=&event=&collection_id=&date_from=&date_to=
                 &result=&elo_min=&elo_max=&variant=&sort=newest|oldest|date|white|black|length
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
POST   /api/chat                   {"message": "...", "fen": "..."} -> reply + actions
POST   /api/analyze/engine         {"fen": "...", "engine": "/path", "depth": 12} one engine
GET    /api/accounts               linked Lichess / Chess.com accounts
POST   /api/accounts/link          {"provider":"lichess","username":"...","token":"..."}
POST   /api/accounts/unlink        {"provider":"lichess"}
POST   /api/accounts/import        {"provider":"lichess","max":50}
POST   /api/engines/test           {"path": "..."} -> UCI name, author, options
POST   /api/engine/console/start   {"path": "..."} persistent UCI session
POST   /api/engine/console/command {"command": "...", "wait_ms": 500}
POST   /api/engine/console/stop
GET    /api/engine/console/status
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
- **Workspaces**: tabs of any kind — Analysis (one per game, with its board,
  ply and annotations), Database, Openings and Puzzles. New tabs come from the
  strip's menu, tabs can be renamed (✎ or double-click), reordered (‹ ›),
  closed, and the whole session (kinds, names, order, active tab) is restored
  after a reload.
- **Board feel**: a piece follows the cursor at its own size while dragging,
  with pointer samples coalesced into one compositor transform per animation
  frame (no per-event layout or shadow work),
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
- **Practice**: **Guess the move** scores your move against the game, **Sparring**
  plays from any FEN against the configured engine, and **Engine match** plays
  two configured engines against each other (per-engine options, depth, start
  FEN, pause/reset, live move list, result and checkmate detection).
- **Statistics**: games, results, players, events, collections and puzzles for
  the current database
- **Advanced database filters**: date range, result, Elo range, variant and
  sort (newest, oldest, game date, player names, longest) alongside the player,
  event, collection and position filters; every filter is a SQL predicate on
  the local store.
- **Keybinds**: previous/next move, start, end and flip are rebindable in
  Settings (browser key names such as `ArrowLeft`, `Home`, `End`, or a single
  letter) and persist with the rest of the settings.
- **Engine management**: a multi-engine list where each engine opens a
  **Configure** dialog with its own UCI options (Hash, Threads, MultiPV,
  Skill Level, UCI_Elo, Syzygy path) and a **Test engine** handshake that
  shows the engine's name, author and option count before you rely on it.
  Per-engine options override the global Threads/Hash for that engine.
- **Engine console**: a dock on the right of the analysis board with a
  persistent UCI session — start/stop the selected engine, send commands
  (`isready`, `position …`, `go depth 12`) and read the raw output.
- **Files**: import a `.pgn` by picking a file or dropping it anywhere on the
  window, recent-file history, and **Save PGN** on the analysis board (the
  download is named after the game).
- **Accounts**: link a **Lichess** username (plus an optional personal API
  token for private games) or a **Chess.com** username; the Home screen shows
  the profile and rating and imports your games (Lichess: newest N; Chess.com:
  newest monthly archive). Links and tokens stay in the local database.
- **Game review extras**: **Cloud eval** fetches Lichess's cloud evaluation for
  the current position (depth, score, line), **Export report** downloads the
  classification as Markdown, and **Annotate game** writes NAGs (`!`, `?`,
  `??`, `!?`) and accuracy comments onto the mainline.
- **AI assistant with tools**: the right-hand dock chats with the configured
  KnightLM model over an OpenAI-compatible `tools` API. The model can call
  `get_position`, `legal_moves`, `analyze`, `draw_arrow`, `draw_circle`,
  `clear_shapes` and `play_move`; server-side tools run in the app, board tools
  come back as actions that draw real arrows/circles or play the move. Plain
  `{"reply": …}` model servers still work.
- **Board customization**: pick custom light/dark square colours (saved with
  the theme as "Custom"), turn coordinates on or off, choose from the eight
  piece sets, and **upload a custom SVG set** (twelve files named `K.svg` …
  `p.svg`, or `wK.svg`/`bK.svg`). Custom sets are stored under
  `data_dir/pieces` and served read-only; the size slider adjusts the board per
  session.
- **Language**: English and Spanish for the navigation, Database, Analysis,
  Practice, Openings, Puzzles and Settings controls, dialogs, placeholders and
  the board's own labels, switchable in Settings and persisted. Transient
  status/error messages stay English. The board exposes an accessible group
  with a label per square ("e4, white pawn"), a skip link, aria labels on
  icon-only controls and live status announcements.
- **Settings**: engine binary, model URL, analysis depth, board theme, piece
  set, board colours, coordinates, engine Threads/Hash, Syzygy path and a
  **multi-engine list**
  (add/remove/select), sound options, move-animation speed, keybinds, the
  optional vision model and custom sound upload, all persisted in SQLite

Build and deploy:

```bash
cd frontend && ./deploy.sh
```

The server serves `frontend/public` at `/`.

## Desktop (Tauri)

`src-tauri/` is a thin native shell around the same server and frontend:

- starts the axum server in-process on `RBC_PORT` (default `8090`), waits for
  `/api/health`, then opens a native window at that URL — the browser build and
  the desktop build run identical code;
- native menu: **File → Open PGN…** imports a PGN through the file picker,
  **File → Open database…** copies a `.db`/`.sqlite` file into the database
  directory and switches to it, **View → Reload** reloads the page;
- imported files refresh the UI through a `rbc-imported` DOM event, so the
  desktop shell needs no separate frontend bundle;
- shares `RBC_DB`, `RBC_DATA`, `STOCKFISH_PATH`, `MODEL_URL`, `RBC_DEPTH` and
  `RBC_PORT` with the server binary.

```bash
# needs webkit2gtk-4.1 + gtk3 development packages
cd src-tauri
cargo run              # dev window
cargo install tauri-cli --version "^2"
cargo tauri build      # .AppImage/.deb/.rpm, .dmg, .msi
```

Icons live in `src-tauri/icons/icon.png`; add the platform-specific sizes
before shipping installers.

## Importing Lichess monthly dumps

Lichess publishes a monthly PGN dump of every rated standard game
(`lichess_db_standard_rated_YYYY-MM.pgn.zst`, tens of GB). The importer streams
it — decompress, split, filter on the tags, import — without unpacking it:

```bash
rustybrickchess import-dump \
  --file data/lichess_db_standard_rated_2026-09.pgn.zst \
  --db ~/.local/share/rustybrickchess/lichess-2026-09-slice.db \
  --max 1000000 \
  --index-positions
```

Options:

| flag | meaning |
|---|---|
| `--file` | the `.pgn.zst` dump |
| `--db` | output database (created if missing) |
| `--max N` | stop after importing N games |
| `--player NAME` | only games with that White/Black |
| `--date-from`, `--date-to` | `YYYY-MM-DD` range |
| `--min-elo`, `--max-elo` | either side's rating |
| `--result` | `1-0`, `0-1`, `1/2-1/2` or `*` |
| `--time-control` | exact value, e.g. `120+1` |
| `--index-positions` | keep the per-ply position index (position search) |
| `--batch N` | commit every N games (default 2000) |

The index costs roughly 2× the time and size: without it a game is ~8 KB,
with it ~19 KB (about 19 GB for a million games). Drop the flag for a
browse/search-only archive. A player filter is nearly free once the file has
been read, so `--player` plus `--max 0` imports one account's month in a single
pass. Progress is printed every few seconds; the output database appears in
the app's database switcher. After loading, the importer creates the indexes
and aggregations that keep a million-game database responsive: player/event
catalogs, the first-12-ply opening explorer table, and covering indexes for
position search. A million games with the position index is about 22 GB.

Position search returns up to 500 games per position; partial (piece-only)
search is refused above 5M indexed positions, where it cannot be answered
without scanning the whole table.

## Desktop bundles

`tauri.conf.json` already points the shell at the bundled frontend. Build
installers locally or let CI do it:

```bash
cd src-tauri
npx @tauri-apps/cli@2 build          # .deb/.rpm/.AppImage on Linux
```

`.github/workflows/release.yml` builds on `v*` tags (or manually) for Linux,
macOS and Windows with `tauri-action`, producing `.deb`/`.AppImage`, `.dmg` and
`.msi`/`.exe` in a draft GitHub release. It builds the frontend first
(`cargo-leptos` + wasm-bindgen pinned to `frontend/Cargo.lock`).

### Auto-update signing

Tauri updates are signed. The **private key** stays with the release owner
(CI secret); the **public key** is baked into the app.

1. Generate the keypair once (keep the file safe — losing it means you cannot
   sign updates for installed copies, rotating it requires shipping a new
   build with the new public key):

   ```bash
   npx @tauri-apps/cli@2 signer generate -w ~/.tauri/rustybrick.key
   ```

   It prints the public key (`~/.tauri/rustybrick.key.pub`).

2. Add GitHub repository secrets:

   | secret | value |
   |---|---|
   | `TAURI_SIGNING_PRIVATE_KEY` | contents of `~/.tauri/rustybrick.key` |
   | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | the password you chose |

3. Add the updater plugin and config:

   ```bash
   cd src-tauri && cargo add tauri-plugin-updater
   ```

   ```rust
   // src-tauri/src/main.rs
   .plugin(tauri_plugin_updater::Builder::new().build())
   ```

   ```json
   // src-tauri/tauri.conf.json
   "plugins": {
     "updater": {
       "pubkey": "PASTE_THE_PUBLIC_KEY",
       "endpoints": [
         "https://github.com/Quitetall/KnightLM/releases/latest/download/latest.json"
       ]
     }
   },
   "bundle": { "createUpdaterArtifacts": true }
   ```

4. Let the release workflow sign. `tauri-action` picks up
   `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` from the
   environment; add them to its `env:` block. With
   `createUpdaterArtifacts: true` the action uploads every bundle plus its
   `.sig` and a `latest.json` manifest to the GitHub release.

5. Check for updates in the app (Rust, no JS needed):

   ```rust
   use tauri_plugin_updater::UpdaterExt;
   let app = app.handle().clone();
   tauri::async_runtime::spawn(async move {
       if let Ok(updater) = app.updater() {
           if let Ok(Some(update)) = updater.check().await {
               let _ = update.download_and_install(|_, _| {}, || {}).await;
           }
       }
   });
   ```

6. Bump `version` in `tauri.conf.json` (and the workspace version) for every
   release: the updater compares versions, so a tag without a version bump is
   never offered as an update. Draft releases are not served until published.

## Archives (Parquet)

For collections that outgrow the SQLite store (roughly beyond 2–5M games), move
them to Parquet instead of growing the per-ply position index:

```bash
rustybrickchess export-parquet --db games.db --out games.parquet
rustybrickchess import-parquet --file games.parquet --db archive.db
```

Exports carry id, tags, Elo, result, FEN, SAN moves, time control and variant.
Imports rebuild the catalogs and skip the position index by default (add
`--index-positions` when you need position search on the archive). A round-trip
is covered by a unit test.

## Schema and sessions

The database has a versioned schema (`schema_meta`); `GET /api/health` reports
`schema`. Migrations run on open, so older files upgrade in place.

Workspaces are stored twice: `localStorage` for instant restore and the server
(`/api/session`, keyed by a per-browser client id) so a cleared cache or another
window restores the same tabs.

## Session persistence

Open analysis boards (workspaces) survive a reload or app restart: tabs,
their game, side to move, history ply and the active sidebar tab are stored in
`localStorage` under `rbc.session.v1` and restored on startup.

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
