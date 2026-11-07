# Running RustyBrickChess

RBC runs as one local server with a static browser frontend. The server opens
SQLite directly. Stockfish and model providers are optional.

## Linux

Install the [Rust and frontend prerequisites](../../README.md#start-rbc), then
run these commands from the repository root:

```bash
./packaging/install.sh
~/.local/bin/rusty-brick-chess --open
```

The installer builds both parts, copies them to `~/.local`, and adds a
**RustyBrickChess** launcher. Use `--prefix` to choose a different install
location, `--no-desktop` to skip the launcher, or `--skip-build` for a checkout
that already has the server and frontend built.

## Windows from source

Use a Rust MSVC toolchain and the Visual Studio C++ build tools with a Windows
SDK. The [Windows prerequisites](https://v2.tauri.app/start/prerequisites/#windows)
describe the toolchain setup. Install the [same Rust WASM target and cargo-leptos](../../README.md#start-rbc),
then run this from PowerShell at the repository root:

```powershell
.\packaging\run-rbc.ps1
```

This builds the frontend and server, opens the default browser, and uses
`%LOCALAPPDATA%\RustyBrickChess\rusty-brick-chess.db` by default. Set `RBC_DB`
before launching to select an existing database. The Tauri Windows packaging
workflow is configured for a desktop installer. A published installer and
native Windows QA are still pending.

A 64-bit Windows GNU server build was smoke-tested under Wine. It served the
browser assets, created a SQLite database, and imported a PGN. The native
MSVC build and PowerShell launch still need verification on a Windows host.

## Run from a Unix checkout

```bash
cd apps/rusty-brick-chess
frontend/deploy.sh                 # cargo leptos build, then copy into public/
cargo build --release --locked
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
| database | `RBC_DB` when set. Linux defaults to `$XDG_DATA_HOME/rustybrickchess/rusty-brick-chess.db` or `~/.local/share/rustybrickchess/rusty-brick-chess.db`. The Windows quickstart uses the Local AppData path above. |

The frontend is **not** embedded in the binary. `static_dir()` looks, in order,
at `RBC_STATIC`, `<exe>/../share/rustybrickchess/frontend`,
`<exe>/frontend/public`, then the source tree. A binary copied somewhere on its
own will serve the API and no UI.

## Optional at run time

| variable | effect when unset |
|---|---|
| `STOCKFISH_PATH` | analysis and engine features report unavailability; the rest of the app works |
| `MODEL_URL` | optional default model endpoint. A provider configured in Settings can also supply the Assistant model. |
| `RBC_ADDR` | binds `127.0.0.1:8090` |
| `RBC_DB` | uses the per-user default above |
| `RBC_DATA` | data directory for models and sounds |
| `RBC_DEPTH` | default analysis depth, 15 |

## Play a local model through LAMU

RBC serves the chess UI with the AI runtime offline. Open **Play**, choose an
**Opponent model**, and click **Start game**. You play White. Drag a piece or
click its source and destination squares. RBC asks the model for a reply and
checks it against the current position before adding it to the game.

KnightLM is available in the picker immediately. Play discovers other local
models when it opens. **Play a game** in Assistant opens this same screen.

For LAMU opponents, **Start game** loads the selected model and waits until
it is ready. You can also use **Load model** and **Unload model** directly from
Play on the machine running RBC. An explicit load may evict other models
from the same LAMU service to make room. RBC asks for an 8,192-token context
for new loads to reduce GPU memory use during chess. GPU memory held by other
services, such as Ollama, can still prevent a large model from fitting.

**Longer thinking** gives compatible LAMU opponents more time and up to 2,048
tokens to reason and answer on each turn. KnightLM's pilot uses a separate
sampling mode. It keeps the prompt it was trained on and picks the most
frequent legal reply from 2–9 samples. The direct reply breaks ties when it is
among the tied moves. Use **Move samples** and **Sampling temperature** to
adjust how many replies it generates and how much they vary. Turn off
**Longer thinking** to compare with the one-move mode at temperature zero.

For Assistant chat, select **KnightLM (LAMU)** in the composer and send a
message. RBC uses the local LAMU service if it is healthy. Otherwise it
starts `lamu serve` on loopback, and LAMU loads the registered model when the
request arrives. Set `RBC_LAMU_BIN` if `lamu` is outside the app's `PATH`, and
`RBC_LAMU_PORT` to override the default LAMU port of `8020`. KnightLM
uses text replies in the Assistant. Other model IDs use the endpoint from
**Settings**. Model discovery in Play reads the runtime's catalog.

The verified LAMU build of the KnightLM pilot merges the PEFT adapter into its
pinned SmolVLM base and serves the resulting GGUF under the model ID
`knightlm-smolvlm-500m`. Register that ID in LAMU before using the Assistant
shortcut or selecting KnightLM in Play. [KnightLM model tooling](../../model/README.md)

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
