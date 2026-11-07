# RustyBrickChess (RBC)

RustyBrickChess is a local chess workstation. Import and search PGNs, analyze
positions with Stockfish, build and compare studies, and play or watch models
in matches and tournaments.

**KnightLM is the chess model being trained for RBC.** Today the Assistant and
arena can use configured model providers. The KnightLM pilot adapter has been
trained and reloaded, and integration as a selectable RBC player is on the
[roadmap](apps/rusty-brick-chess/ROADMAP.md#knightlm-model).

## Start RBC

From a clone of this repository, install a recent Rust toolchain and the frontend
build tools once:

```sh
rustup target add wasm32-unknown-unknown
cargo install cargo-leptos --version 0.3.11 --locked
```

**Linux**

```sh
./packaging/install.sh
~/.local/bin/rusty-brick-chess --open
```

The installer builds the server and frontend together and adds **RustyBrickChess**
to your application menu. You can also open <http://127.0.0.1:8090/> while the
server is running.

**Windows**

Use the [Windows source quickstart](docs/app/running.md#windows-from-source) to
build and open RBC from PowerShell. The repository has a Tauri packaging workflow
for Windows. An installable Windows release and native Windows QA are pending.

Stockfish and a model provider are optional. Start in **Database** to import a
PGN, or open **Analysis** and explore a board. Configure an engine or model
provider in **Settings** when you want those features.

## How RBC fits together

- [`apps/rusty-brick-chess`](apps/rusty-brick-chess/README.md) serves a Leptos/WASM
  UI and the Axum API from one local process. SQLite stores games, studies,
  sessions, and approvals.
- [`crates/notation`](crates/notation) and [`crates/core`](crates/core) normalize
  chess records, replay moves, and compile state. Stockfish connects over UCI.
- [`crates/context`](crates/context) builds the Assistant's inspectable prompts.
  [`crates/ai`](crates/ai) handles providers, streaming, matches, and approval
  policy. [MCP clients](docs/app/mcp.md) use the same tool and approval paths.
- [`model/`](model/README.md) contains KnightLM's Lichess and SmolVLM training
  path. [`crates/model`](crates/model) contains the experimental Burn backend
  contracts and CPU smoke test. The [model plan](docs/model/plan.md) explains the
  fresh-state experiment and [contracts](docs/model/contracts.md) define its evidence.
- [Lichess bot integration](integrations/lichess-bot/README.md) connects the UCI
  player to lichess-bot.

## Documentation and contributing

- [Documentation index](docs/README.md) for the app, model, release evidence, and OpenWarrant records.
- [Contributor onboarding](CONTRIBUTING.md) for setup, checks, and pull requests.
- [Work orders by role](docs/contributors/work/README.md) for ready tasks and pickup commands.
- [Roadmap](apps/rusty-brick-chess/ROADMAP.md) for upcoming app and model work.
