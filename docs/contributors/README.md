# Getting started

This repository holds two products that meet at one contract: **RBC** (the chess app) and
**KnightLM** (the ML path). Pick the path you need; you can do either without the other.
Commands run from the repository root. Anything marked *unverified* was not run when this was
written.

| I want to | Path |
| --- | --- |
| Run or change the chess app | [RBC path](#path-a-rbc-the-chess-app) |
| Work on the corpus, tokenizer, training, evaluation or serving | [KnightLM path](#path-b-knightlm-the-model) |
| Pick a task | [Work orders](work/README.md), [open issues](https://github.com/Quitetall/KnightLM/issues) |
| Open a pull request | [CONTRIBUTING](../../CONTRIBUTING.md) |

## Prerequisites

| Tool | Needed for | Notes |
| --- | --- | --- |
| Rust (`rustup`, stable, edition 2024) | everything Rust | `rustup component add rustfmt clippy`; `rustup target add wasm32-unknown-unknown` for the frontend |
| Python 3.11+ | model tests, tools, scripts | the stdlib is enough for most of `make check` |
| [`uv`](https://docs.astral.sh/uv/) or `python -m venv` | model virtualenvs | commands below use `uv` |
| Stockfish | UCI smoke tests, evaluation oracles, RBC analysis | on `PATH` or `STOCKFISH_PATH` |
| `cargo-leptos 0.3.11` | building the RBC frontend | `cargo install cargo-leptos --version 0.3.11 --locked` |
| `war` (OpenWarrant) | claiming tracked work | optional; see [CONTRIBUTING](../../CONTRIBUTING.md#claiming-work) |
| `gitleaks` | `make check-secrets` | optional; CI runs it |
| GPU with CUDA | only for training and real-checkpoint serving | never needed for tests, docs or the fake server |

## Resources to expect

| Task | Disk | RAM | GPU |
| --- | --- | --- | --- |
| Read, edit docs, `make check` | a Rust build cache (several GB, unmeasured); keep `target/` out of the repository tree if you can | a few GB | none |
| Model CPU tests with the CI environment | the CPU-only torch wheels and transformers (about 2 GB, unmeasured) | a few GB | none |
| Fake model server, `curl` | the small venv below | small | none |
| Serving a real SmolVLM-500M checkpoint | base weights (about 1 GB) plus the adapter | unmeasured | one CUDA GPU; peak not recorded for serving |
| Training (Run 1 short, measured) | corpus shards and run directory, tens of GB at Run 1 scale | the trainer streams shards, flat memory | 14.5 GB peak on an RTX 4090 for batch 8 x 4; 7.9 GB at batch 4 x 8 ([training gate](../model/training-gate.md)) |

Check `df -h` before a large build or download.

## Path A: RBC, the chess app

1. Install the frontend prerequisites once ([prerequisites](../app/running.md#prerequisites-for-building)),
   or skip building and install a release from the [Releases page](https://github.com/Quitetall/KnightLM/releases).
2. Build and run it: `./packaging/install.sh`, then `~/.local/bin/rusty-brick-chess --open`
   ([Linux](../app/running.md#linux), [Windows](../app/running.md#windows-from-source),
   [from a checkout](../app/running.md#run-from-a-unix-checkout), [desktop shell](../app/running.md#desktop-app-tauri)).
   Open <http://127.0.0.1:8090/>.
3. Check your change: `make check` (workspace), `make check-apps` (the server and wasm frontend,
   which the workspace excludes), `make check-desktop` (Tauri shell, Linux). The browser suites
   run in `make gate`, which needs Stockfish and is for release candidates.
4. To try a model in RBC, see [Using a model](../app/models.md); the KnightLM server preset
   points at `http://127.0.0.1:8000/v1`.

RBC never imports `model/`; the rule is in [the seam](../model/seam.md#what-each-side-may-import)
and enforced by `scripts/check_layers.py`.

## Path B: KnightLM, the model

Read the [model docs index](../model/README.md) for the order to read things in.

### Python environments

`make check` needs only the system `python3` and runs, with a printed list of what it skipped.
Tests that need torch skip there; `make check-torch` treats those skips as failures, so it
needs an environment with the CI dependencies:

```sh
uv venv /path/outside/the/repo/knightlm-ci          # any directory; not inside the checkout
uv pip install --python /path/outside/the/repo/knightlm-ci/bin/python \
  --extra-index-url https://download.pytorch.org/whl/cpu -r model/ci-requirements.txt
make check-torch PYTHON=/path/outside/the/repo/knightlm-ci/bin/python
```

`model/ci-requirements.txt` pins CPU-only torch, transformers, peft, Pillow, numpy, fastapi,
uvicorn, httpx and openai: the exact set CI's `model-torch` job installs. Two GPU tests skip by
design and name themselves. `test_uci_gate` also needs `cargo build --locked` and Stockfish;
CI forbids that skip, a local run may skip it.

Real training and checkpoint serving use other environments (CUDA torch, `model/train/requirements.txt`,
the BLUT image-text venv). Their setup is in [`model/README.md`](../../model/README.md).

### Run the model server with a fake predictor

No model, torch or GPU. The real serving app answers every packet with its first legal move,
so you can exercise the whole HTTP contract, `knightlm-uci` and the match driver.

```sh
uv venv /path/outside/the/repo/knightlm-fake
uv pip install --python /path/outside/the/repo/knightlm-fake/bin/python \
  fastapi==0.115.12 uvicorn==0.34.2 httpx==0.28.1 openai==1.93.0
/path/outside/the/repo/knightlm-fake/bin/python -m model.serve.fake_server --port 8123 &
```

Build a request from the start position (the packet needs a `legal_moves` list, which the
compiler's output does not carry, so the example supplies three) and post it:

```sh
cargo run --locked -q -p knightlm-cli --bin knightlm -- compile fixtures/start.json > start-packet.json
python3 - <<'PY'
import json
p = json.load(open("start-packet.json"))
p["legal_moves"] = ["e2e4", "d2d4", "g1f3"]
json.dump({"model": "knightlm", "messages": [{"role": "user", "content": json.dumps(p)}]},
          open("request.json", "w"))
PY
curl -s localhost:8123/v1/chat/completions -H 'content-type: application/json' -d @request.json
curl -s 'localhost:8123/v1/chat/completions?mode=constrained' -H 'content-type: application/json' -d @request.json
```

Expected: a `chat.completion` whose `choices[0].message.tool_calls[0]` is `submit_move` with
`{"reasoning": ..., "move": "e2e4"}`, and a `knightlm` object with `legal: true`. Other
endpoints: `GET /v1/models`, `/v1/manifest`, `/v1/health`. The full contract is in
[the seam](../model/seam.md). Stop the server with `kill %1` and delete the scratch files.
The same server backs `scripts/knightlm_uci_game_gate.sh`.

### Where artifacts go

Datasets, checkpoints, scorecards and databases stay out of git. The owner's machine keeps them
in an artifact store with two tiers: `keep/` for evidence that docs and receipts cite (never
auto-deleted) and `expire/` for checkpoints, corpora and run outputs (swept 30 days after the
last change). On your machine use any directory outside the checkout and give its path to the
tool that asks for one; docs cite evidence by directory name, for example
`keep/eval-run1short-playmode-300-20261120T221837Z`, which you will not have unless a
maintainer shares it. Keep credentials out of the tree: see [Secrets](secrets.md).

### Training needs the owner's go

Preparing a run is open to everyone: corpus builds, audits, tokenisation counts and CPU
preflights (`python -m model.train.run` without `--gpu`) launch nothing. **Launching any GPU
training run needs the owner's explicit go for that run**, because it spends shared hardware
and creates evidence that other work cites. Open a pull request or issue with the preflight
output and ask. See the [training gate](../model/training-gate.md) for the arms, order and stop rules.

## Before you open a pull request

Follow [CONTRIBUTING](../../CONTRIBUTING.md): a branch from `main`, a draft pull request early,
the checks relevant to your change listed in the description, and a green fast CI before merge.
For documentation, run `python3 scripts/check_doc_links.py`. Release qualification
(`make gate`) is the maintainers' job; it needs Stockfish.
