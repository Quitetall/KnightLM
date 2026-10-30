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
| Engine analysis | **Partial** | lines, MultiPV, all-engines, report, eval graph; **no per-engine UCI options**, no engine-vs-engine |
| Tree editing | **Done** | variations, promote/delete, PGN rewrite, annotations |
| Openings | **Done** | local/lichess/masters explorer, ECO names, first-12-ply aggregate index |
| Puzzles | **Done** | Lichess CSV, themes, rating, play-out |
| Repertoires | **Done** | import, training, SM-2 |
| Practice | **Partial** | guess-the-move, sparring from FEN; no strength presets or eval-scored guesses |
| Workspaces | **Partial** | analysis tabs persist across reloads; **tabs are analysis-only**, no Database/Openings tabs, no rename/reorder |
| Files | **Partial** | PGN import textarea, export link, desktop "Open PGN…"; **no browser file picker/drag-drop, no recent files** |
| Accounts | **Partial** | local profile + player record; **no Lichess/Chess.com sign-in or sync** |
| Settings | **Done** | engines list, depth, threads/hash, syzygy, theme/pieces, sounds, keybinds, animation, profile, vision |
| Board themes | **Partial** | 8 piece sets, 3 board colours; no custom colours or piece upload |
| Desktop shell | **Partial** | Tauri window + native menus + installer; **only built/tested on Linux**, no CI bundles, no auto-update |
| i18n / accessibility | **Missing** | English strings inline, keyboard-only paths untested |

Rough position: **~85% of the desktop surface**, ~100% of the
database/analysis/opening/puzzle/training core. The remaining work is
concentrated in engine configuration, files/accounts, workspace flexibility and
packaging.

## Remaining work, in recommended order

### 1. Engine management parity — M
- per-engine UCI options (Hash, Threads, MultiPV, Syzygy, Ponder) stored with
  each engine, applied per spawn;
- engine settings dialog with a live "test engine" result (name, options);
- engine-vs-engine from Practice (colour, depth/time, two configured engines);
- strength limiting (Skill Level / UCI_Elo) for sparring.
- *Done when:* two engines can play each other with their own Hash/Threads, and
  an engine's options survive a restart.

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

### 4. Workspaces 2.0 / multi-window — M
- tab kinds: Database, Analysis, Openings, Puzzles (not just Analysis);
- rename/reorder tabs, per-tab scroll/selection, restore after crash;
- optional second window in the desktop shell;
- *Done when:* a Database tab and two Analysis tabs can be open at once with
  independent state.

### 5. Board customization — S
- custom board colours (light/dark, coordinates on/off);
- piece-set upload (SVG set) and persisted selection per database;
- *Done when:* a user can pick custom colours and restart with them applied.

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
