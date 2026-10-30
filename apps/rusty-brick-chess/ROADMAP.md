# RustyBrickChess roadmap

Status against **En Croissant** (the reference desktop app). "Parity" here means
the same daily workflow on Linux/macOS/Windows: a database you can search, a
board you can analyse and play from, engines you can configure, and the same
sidebar surfaces.

## Where we are

| Surface | State | Notes |
|---|---|---|
| Database: import, search, filters, position search | **Done** | player/event/result/date/Elo/variant filters, exact + partial position search, collections, multi-DB, indexes |
| Database at scale | **Done** | 1M-game slice imported; stats 38 ms, explorer 45 ms, position search 35 ms |
| Analysis board | **Done** | drag/click, instant/slide, history last-move, board editor, clocks, arrows/circles, variants |
| Engine analysis | **Done** | lines, MultiPV, all-engines, report, eval graph, per-engine UCI options, engine dialog + test, UCI console |
| AI assistant | **Done** | chat dock with OpenAI-style tool calls: analyse, draw arrows/circles, play moves |
| Engine-vs-engine | **Done** | Practice → Engine match, per-engine options, pause/reset, result |
| Tree editing | **Done** | variations, promote/delete, PGN rewrite, annotations |
| Openings | **Done** | local/lichess/masters explorer, ECO names, first-12-ply aggregate index |
| Puzzles | **Done** | Lichess CSV, themes, rating, play-out |
| Repertoires | **Done** | import, training, SM-2 |
| Practice | **Partial** | guess-the-move, sparring from FEN; no strength presets or eval-scored guesses |
| Workspaces | **Done** | Analysis/Database/Openings/Puzzles tabs, rename, reorder, session restore |
| Board customization | **Done** | custom colours, coordinates, 8 sets, custom SVG set upload |
| Files | **Done** | file picker + drag-and-drop import, recent files, Save PGN, desktop "Open PGN…" |
| Accounts | **Done** | Lichess (optional token) and Chess.com links, profile/rating, game import |
| Settings | **Done** | engines list, depth, threads/hash, syzygy, theme/pieces, sounds, keybinds, animation, profile, vision |

| Desktop shell | **Partial** | Tauri window + native menus + installer; **only built/tested on Linux**, no CI bundles, no auto-update |
| i18n / accessibility | **Partial** | English/Spanish strings, board aria labels, skip link, live regions; full translation coverage remains |

Rough position: **~85% of the desktop surface**, ~100% of the
database/analysis/opening/puzzle/training core. The remaining work is
concentrated in engine configuration, files/accounts, workspace flexibility and
packaging.

## Remaining work, in recommended order

### 1. Engine management parity — **done** (engine-vs-engine remains)
- per-engine UCI options stored with each engine and applied per spawn;
- engine dialog with a live "test engine" handshake (name, author, options);
- strength limiting via Skill Level / UCI_Elo in the dialog;
- engine console dock (persistent UCI session);
- *remaining:* engine-vs-engine from Practice.

### 2. Files — S–M
- browser PGN file input and drag-and-drop onto the window;
- "Open file…" in the desktop menu already exists; add "Save current game as…";
- recent files (per database) and "open database file" in the browser via the
  app server;
- *Done when:* dropping a `.pgn` on the window imports it, and a game can be
  saved to disk without the export link.

### 3. Accounts — M–L
- Lichess and Chess.com sign-in (token/OAuth), profile + rating display;
- "import my games since…" and incremental sync;
- keep local profiles as the default, no account required;
- *Done when:* a user can link Lichess, pull their games, and see their record
  without exporting a PGN.

### 4. Workspaces 2.0 / multi-window — **done** (second window remains)
- tab kinds, rename/reorder, session restore;
- *remaining:* an optional second window in the desktop shell.

### 5. Board customization — **done** except piece-set upload
- custom light/dark colours, coordinates toggle, persisted;
- *remaining:* uploading a custom SVG piece set.

### 6. Packaging & release — M
- GitHub Actions matrix for `.deb`/`.AppImage`/`.dmg`/`.msi`;
- per-platform icons and metadata, version stamping;
- auto-update channel (Tauri updater) and code signing when certificates are
  available;
- *Done when:* a tagged release produces installable artifacts for three OSes.

### 7. Analysis polish — S–M
- accuracy/eval graph parity (per-move bars, blunder list, export report);
- Lichess cloud eval lookup for the current FEN;
- "annotate game" pass writing NAGs/comments from the report.

### 8. i18n & accessibility — S–M
- extract strings, add at least one translated locale;
- focus order, ARIA names on board squares and panels, high-contrast check.

## Technical debt to schedule alongside

- `frontend/src/lib.rs` is ~5k lines: split into `views/` modules before adding
  more tabs.
- SQLite is comfortable to ~2–5M games; beyond that, archive mode should move to
  DuckDB/Parquet (En Croissant's choice) instead of growing the position index.
- Schema changes rely on `CREATE ... IF NOT EXISTS`; add a migration/version
  table before the next breaking change.
- Session persistence is `localStorage` only; if accounts arrive, move it to the
  server profile.
