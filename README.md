# RustyBrickChess (RBC) and KnightLM

This repository holds two products that meet at one wire contract.

| | RBC (RustyBrickChess) | KnightLM |
| --- | --- | --- |
| What it is | A local chess workstation: import and search PGNs, analyze with Stockfish, build studies, play against a built-in test bot or a model. | The ML path: corpus, tokenizer, board encoder, training, evaluation and serving of a chess model. |
| Status | Pre-alpha. Desktop installers for Linux and Windows are published as pre-releases (unsigned; only Linux hand-tested). No macOS build. | Research code. One short training run (Run 1 short) has happened; its adapter plays at about random strength and does not chat. |
| Needs | Rust to build from source; nothing to run an installer | Python 3.11+, optional GPU for training only |

They share no code beyond [`crates/contract`](crates/contract) and the OpenAI-compatible
`/v1/chat/completions` endpoint ([the seam](docs/model/seam.md)). RBC works with no
KnightLM present and with any provider.

## Get RBC

- **Installers:** download from the [Releases page](https://github.com/Quitetall/KnightLM/releases)
  (Linux `.AppImage`, `.deb`, `.rpm`; Windows `-setup.exe`, `.msi`; Stockfish 19 is bundled).
  They are unsigned, so Windows SmartScreen will warn. Release process:
  [releasing](docs/app/releasing.md).
- **From source:** `./packaging/install.sh` on Linux, `.\packaging\run-rbc.ps1` on Windows. The
  prerequisites and every other way to run it (desktop shell, server from a checkout, environment
  variables) are in [Running RBC](docs/app/running.md).

The desktop app and the web server (`rusty-brick-chess`) are two ways to run the same app.
The main branch still has the original multi-tab interface; a redesign is in draft pull
request #14 and is not released.

**Play AI:** open the **Play AI** tab, pick a colour and press **Start**. The built-in test
bot (random moves, canned chat, labelled as a test bot) works at once. The real KnightLM model
is opt-in: **Get the model** downloads a private Python, CPU packages and pinned weights (about
1.4 GB, about 2.4 GB on disk). It is weak and does not chat. See
[Play AI](docs/app/models.md#play-ai-one-click-opponent).

## KnightLM in brief

KnightLM asks whether a fresh, complete state packet from a deterministic rules compiler
improves chess reasoning and token cost compared with carrying the game history forward.
Only Run 1 short (625 steps, 20,000 rows) has trained; the full Run 1, every other arm and
larger bases are prepared, not run, and every GPU run needs the owner's go. There is no
strength claim. The Run 1 short adapter is published as the release
[`model-run1-short-adapter-v1`](https://github.com/Quitetall/KnightLM/releases/tag/model-run1-short-adapter-v1)
(Apache-2.0). Start at the [model docs index](docs/model/README.md); tooling commands are in
[model/README.md](model/README.md).

## Build and check

`make check` runs the Rust workspace, the Python model tests and the contract and layering
checks; `make check-apps` and `make check-desktop` cover the server, the wasm frontend and the
Tauri shell. See [CONTRIBUTING](CONTRIBUTING.md#ci-jobs) for what CI runs.

## Documentation and contributing

- [Documentation index](docs/README.md).
- [Getting started](docs/contributors/README.md) for a fresh clone, for the RBC and KnightLM paths.
- [Contributor guide](CONTRIBUTING.md) for the branch and pull request flow.
- [Work orders](docs/contributors/work/README.md) and [GitHub Issues](https://github.com/Quitetall/KnightLM/issues).
- [Roadmap](apps/rusty-brick-chess/ROADMAP.md).

## License

RBC is released under the [GNU Affero General Public License, version 3](LICENSE).
See the [sound asset notices](apps/rusty-brick-chess/frontend/assets/sounds/COPYING.txt)
for the bundled Lichess sound themes.

## Credits

- [Project contributors](https://github.com/Quitetall/KnightLM/graphs/contributors)
- [Lichess](https://lichess.org/) for game data and sound themes, including the
  sound work credited to Enigmahack in the bundled notices.
- [Stockfish](https://stockfishchess.org/) for the optional analysis engine.
- [Hugging Face TB](https://huggingface.co/HuggingFaceTB/SmolVLM-500M-Instruct)
  for the SmolVLM checkpoint used in the KnightLM pilot.
