# Running RustyBrickChess

The app is one binary plus the static frontend it serves. There is no separate
server process, no Node runtime at run time, and no database server: SQLite is
opened directly.

```bash
cd apps/rusty-brick-chess
frontend/deploy.sh                 # cargo leptos build, then copy into public/
cargo build --release
./target/release/rusty-brick-chess --open
```

`--open` starts the server on `127.0.0.1:8090` and opens it in the default
browser. If something is already listening it just opens the browser and exits,
so a launcher or a second invocation is harmless.

## Where things come from

| piece | where |
|---|---|
| binary | `apps/rusty-brick-chess/target/release/rusty-brick-chess` |
| frontend | `apps/rusty-brick-chess/frontend/public` (`index.html`, `pkg/`, `sounds/`) |
| database | `${RBC_DB:-$XDG_DATA_HOME/rustybrickchess/rusty-brick-chess.db}` |

The frontend is **not** embedded in the binary. `static_dir()` looks, in order,
at `RBC_STATIC`, `<exe>/../share/rustybrickchess/frontend`,
`<exe>/frontend/public`, then the source tree. A binary copied somewhere on its
own will serve the API and no UI.

## Install for a user

```bash
packaging/install.sh                     # ~/.local
packaging/install.sh --prefix /usr/local # system-wide
packaging/install.sh --no-desktop        # skip the .desktop entry
```

It builds the release binary, places the frontend where `static_dir()` expects
it (`<prefix>/share/rustybrickchess/frontend`), and writes a desktop entry whose
`Exec` is `<prefix>/bin/rusty-brick-chess --open`. It warns if the binary is
older than the newest source file, because a stale binary installs a working app
that silently lacks newer migrations and routes.

## Optional at run time

| variable | effect when unset |
|---|---|
| `STOCKFISH_PATH` | analysis and engine features report unavailability; the rest of the app works |
| `MODEL_URL` | the Assistant tab answers `model URL is not set` |
| `RBC_ADDR` | binds `127.0.0.1:8090` |
| `RBC_DB` | uses the per-user default above |
| `RBC_DATA` | data directory for models and sounds |
| `RBC_DEPTH` | default analysis depth, 15 |

## The tabs

`Home`, `Database`, `Analysis`, `Assistant`, `Play`, `Practice`, `Openings`,
`Puzzles`, `Stats`, `Settings` — a workspace tab strip sits above the board, and
the sidebar carries the board tree so a stored board can be reached without
switching tabs. The Analysis tab keeps the older dock: the engine panel, the
chat endpoint (`/api/chat`), and everything the app had before the assistant
work started.

## Other commands

```bash
rusty-brick-chess export-parquet --db DB --out FILE.parquet
rusty-brick-chess import-parquet --file FILE.parquet [--db DB] [--index-positions]
rusty-brick-chess import-dump --file DUMP.zst --db OUT.db [filters]
rusty-brick-chess --version
```
