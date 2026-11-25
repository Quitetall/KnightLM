# Running RustyBrickChess

This is the canonical page for running RBC. The desktop app (Tauri) and the web server
(`rusty-brick-chess`) are two ways to run the same app: the same server and frontend code, one
in a native window and one in a browser tab. SQLite is opened directly. Stockfish and model
providers are optional.

## Install a release

Download an installer from the [Releases page](https://github.com/Quitetall/KnightLM/releases).
The latest, `v0.1.0-prealpha.4`, has a Linux `.AppImage`, `.deb` and `.rpm` and a Windows
`-setup.exe` and `.msi`. There is no macOS build (GitHub assigned no macOS runner); build from
source on a Mac. The installers are not code-signed (Windows SmartScreen warns: "More info > Run
anyway"). Only Linux has been run by hand. Linux: `chmod +x` the AppImage and run it, or install
the `.deb`/`.rpm`. How releases are made: [releasing](releasing.md).

## Prerequisites for building

```sh
rustup target add wasm32-unknown-unknown
cargo install cargo-leptos --version 0.3.11 --locked
```

## Linux

Install the [prerequisites](#prerequisites-for-building), then run these commands from the
repository root:

```bash
./packaging/install.sh
~/.local/bin/rusty-brick-chess --open
```

The installer builds both parts, copies them to `~/.local`, and adds a
**RustyBrickChess** launcher. Use `--prefix` to choose a different install
location, `--no-desktop` to skip the launcher, or `--skip-build` for a checkout
that already has the server and frontend built.

### Play AI after installing

The installer also copies the model server's Python code to
`~/.local/share/rustybrickchess/model-runtime` (the list is
`packaging/model-runtime.files`). Open the app and choose **Play AI**: the test bot
needs nothing. For the KnightLM model press **Get the model** (about 1.4 GB, one time):
the app uses a Python 3.10 to 3.13 it finds, or downloads its own pinned Python, then builds
a private environment for the packages under its data directory (or tells you the one command
to run). No system Python is needed. Details: [Play AI](models.md#play-ai-one-click-opponent).

## Desktop app (Tauri)

A native window instead of a browser tab, with menus, drag-and-drop of PGN
files, single-instance behaviour and installers. Install the Tauri system
libraries (Debian/Ubuntu: `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev
librsvg2-dev patchelf`), build the frontend, then from the repository root:

```bash
make rbc-desktop          # run it
make rbc-desktop-bundle   # .deb and .AppImage under src-tauri/target/release/bundle/
```

The window talks to an in-process server on a random loopback port, protected
by a per-launch token, so other local programs cannot call the API. Details,
data locations and Stockfish discovery are in the
[app README](../../apps/rusty-brick-chess/README.md#desktop-tauri). Only Linux
has been run by hand (the bundle carries the model server code as a `model-runtime`
resource, so Play AI works from an installed app). The release workflow also builds the Windows
installers, which only CI has built; its macOS job has never run because GitHub assigned no runner.

**Stockfish is bundled.** The release workflow downloads the official Stockfish 19 build for
each OS, checks a pinned sha256 (`scripts/fetch_bundled_stockfish.sh`) and Tauri ships it as the
resource `engine/stockfish` (`stockfish.exe` on Windows). The engine is chosen in this order:
`STOCKFISH_PATH`, the bundled binary, a `stockfish` on `PATH` (desktop shell only), else none.
The bundled binary is looked up in the resource directory and around the executable, so the
`.deb`/`.rpm` (`/usr/lib/RustyBrickChess/engine/`), the AppImage (`$APPDIR/usr/lib/RustyBrickChess/engine/`),
macOS (`Contents/Resources/engine/`) and Windows (next to the exe) layouts are all handled
(`src/engine_locate.rs`, tested with fixtures of each layout). With no engine the analysis
routes answer 503 "No chess engine found..." and Settings shows the same sentence; nothing
crashes. We build the "universal" asset: one binary (about 100 MB, networks inside) that picks
AVX-512, AVX2 or SSE at start, so it runs on every x86-64 CPU; a fixed `x86-64-avx2` build
would be smaller but fail on CPUs without AVX2. Licence and source: [third-party software](third-party.md).

## Windows from source

Use a Rust MSVC toolchain and the Visual Studio C++ build tools with a Windows
SDK. The [Windows prerequisites](https://v2.tauri.app/start/prerequisites/#windows)
describe the toolchain setup. Install the [same Rust WASM target and cargo-leptos](#prerequisites-for-building),
then run this from PowerShell at the repository root:

```powershell
.\packaging\run-rbc.ps1
```

This builds the frontend and server, opens the default browser, and uses
`%LOCALAPPDATA%\RustyBrickChess\rusty-brick-chess.db` by default. Set `RBC_DB`
before launching to select an existing database. Published Windows
installers exist (see above) but have not been opened by a person.

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

## Environment variables

All optional.

| variable | effect |
|---|---|
| `RBC_ADDR` | server bind address; default `127.0.0.1:8090` |
| `RBC_DB` | database file; default is the per-user path in the table above |
| `RBC_DATA` | data directory for models, sounds and the managed KnightLM install |
| `RBC_STATIC` | frontend directory to serve |
| `RBC_DEPTH` | default analysis depth, 15 |
| `STOCKFISH_PATH` | engine to use. Unset: the Stockfish bundled with the desktop installers (`engine/stockfish`), then `stockfish` on `PATH` (desktop shell only); with none, analysis answers "No chess engine found" and the rest of the app works. A path saved in Settings or chosen from the File menu overrides all of these. |
| `MODEL_URL` | optional default chat endpoint; a provider configured in Settings can also supply the Assistant model |
| `RBC_KNIGHTLM_PYTHON` | the model's Python: used as is (3.10 or newer), nothing downloaded; see [Play AI](models.md#play-ai-one-click-opponent) |
| `RBC_KNIGHTLM_SERVE_URL` | endpoint the KnightLM server preset points at (default `http://127.0.0.1:8000/v1`) |
| `RBC_MODEL_CODE` | where the model server's Python code is found |
| `RBC_LAMU_BIN`, `RBC_LAMU_PORT` | optional LAMU integration, below |

More switches (test bot seed, fake model server, desktop token and port) are listed in
[Play AI](models.md#play-ai-one-click-opponent) and the
[app README](../../apps/rusty-brick-chess/README.md#desktop-tauri).

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
sampling mode. Both KnightLM modes keep the trained prompt and constrain
generation to the legal UCI moves for the current position. RBC sends the
constraint to the local llama.cpp backend managed by LAMU. Sampling picks the
most frequent reply from 2–9 samples. The direct reply breaks ties when it is
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

On `main` the sidebar has 13 tabs: Home, Database, Analysis, Assistant, Play, Play AI, Study,
Compare, Practice, Openings, Puzzles, Stats and Settings (`frontend/src/model.rs`). A redesign of
this interface is in draft pull request #14 and is not on `main`.

## Other commands

```bash
rusty-brick-chess export-parquet --db DB --out FILE.parquet
rusty-brick-chess import-parquet --file FILE.parquet [--db DB] [--index-positions]
rusty-brick-chess import-dump --file DUMP.zst --db OUT.db [filters]
rusty-brick-chess --version
```
