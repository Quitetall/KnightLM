# Using a model from RustyBrickChess

RustyBrickChess (RBC) reaches a model through one door: an OpenAI-compatible
`/v1/chat/completions` endpoint. It never imports KnightLM code. The installers do ship the
model server's Python files, which the app starts as a child process only if you press **Get the
model** (see Play AI below). A KnightLM checkpoint is one provider among others, such as Ollama, LM Studio, vLLM,
llama.cpp or a hosted API. The contract with a KnightLM server is in
[docs/model/seam.md](../model/seam.md); RBC needs none of it to work.

## Providers

A provider is a row in Settings > Providers (`/api/providers`): a label, a base URL
(`http://127.0.0.1:11434/v1`) and an optional key **reference**.

- A key reference is a name, never a value: an environment variable name
  (`OPENAI_API_KEY`) or `keyring:<service>/<account>`. A value that looks like a key
  is refused with 400, and the refusal does not repeat it. Responses report whether
  the reference resolves, not what it resolves to; no secret reaches the database,
  the audit log or a response.
- **KnightLM server** preset: Settings has an "Add KnightLM server" button, and
  `POST /api/providers {"preset": "knightlm"}` does the same. It fills the label
  `KnightLM server` and the local base URL (`http://127.0.0.1:8000/v1`, or what
  `RBC_KNIGHTLM_SERVE_URL` points at), so chat goes to
  `http://127.0.0.1:8000/v1/chat/completions`, with model name `knightlm`. It is an
  row you can edit or delete like any other, but its `kind` is `knightlm` (others are
  `openai_compatible`), which makes RBC speak the seam contract to it (next section).
  `GET /api/providers/presets` lists presets. A server added by hand is recognised the
  first time its connection test (or model refresh) sees `owned_by: "knightlm"` in
  `/v1/models`; the test answer carries `knightlm: true` and the row's kind is stored.
- **Connection test**: `POST /api/providers/{id}/test` (optional body `{"model": "..."}`)
  or, before saving, `POST /api/providers/probe {"base_url", "key_ref", "model"}`. The
  answer is `{ok, error, models, model_ids, latency_ms}`; on failure `error` is
  `{code, message}` with one of `invalid_url`, `key_unusable`, `key_unresolved`,
  `timeout`, `unreachable`, `unauthorized`, `not_found` (usually a base URL missing
  `/v1`), `server_error`, `bad_response`, `model_missing`.

## Seats and the chat toggle

A match has a white and a black seat; a person or a model (`{"kind": "model",
"provider": "provider:<id>", "model": "<name>"}`) holds each. `provider:<id>` names a
registered provider; the existing `local:<name>` seats keep working. A third seat, the
**coach**, answers chat and coach turns and defaults to the model that is playing.

`GET` and `PUT /api/model-seats` store `{white, black, coach, chat}` as defaults
(a seat naming a provider that is not registered is a 400). A model move or chat
request may carry its own `coach` and `chat`, which win over the stored values.

The chat toggle sits with the play controls on the Play tab.

| Chat | What the model gets | What comes back |
|---|---|---|
| on | a **coach turn**: the position, a rolling summary of the chat and the recent messages, the coach tools | the move (the last legal `Provisional:` line), a reply for the chat, and marks for the board |
| off | the **competition (play) harness**: the position and the play tools, `submit_move` | the move only |

For a `knightlm` provider the table holds, but the wire is the seam contract
([docs/model/seam.md](../model/seam.md)), not the generic prompts: chat off sends the
play prompt with the packet **including `legal_moves`**, raw decoding, and reads the
`submit_move` call; chat on sends the earlier chat as messages, the packet as the last
user message and `knightlm: {turn: "coach", summary}`, and the server renders the coach
prompt and reads the `Provisional:` lines. Either way the move goes through the commit
gate here. An illegal or missing move is not trusted: one more request asks for
`constrained` decoding and the result says `tools.fallback: true` (a coach fallback
shows no reply and draws nothing). The server's decoding is fixed, so there is no
feedback loop of retries. Every other provider keeps the generic prompts and text tool calls.

A rated game never chats, whatever the toggle says (`chat_on` in the answer says what
happened). The client keeps the chat and sends `summary` and `history` with
`POST /api/ai/matches/{id}/model-move` and `/chat`; the server rolls them to the
bounds in `rbc_ai::model_turn` (the last messages verbatim, older ones folded into a
summary) and returns the rolled chat in `chat`, to send back next time.

## What a coach turn draws

`model-move` returns `tools.ui`, the structured effects of the coach's calls in order,
and `tools.marks`, the marks on the board when the turn ended. The Play tab draws
arrows and marked squares from `tools.marks`, highlights the ply the coach pointed at
(`show_ply`) in the move list, and shows that position on a small read-only board.
The UI interprets data and never parses model text.

## Play AI (one-click opponent)

The **Play AI** tab (`frontend/src/views/play_ai.rs`) is the short path to an opponent that
talks. It offers two opponents and uses the ordinary match routes (`model-move` with chat on,
`chat`), so everything above applies.

- **Test bot** (`local:rbc-test-bot`, label "Test bot (random moves)"; `GET /api/ai/test-bot`).
  Built into the app (`crates/ai/src/provider/test_bot.rs`): needs no Python, model, GPU or
  network. It speaks the seam contract in process: seeded random legal moves (`RBC_TEST_BOT_SEED`
  fixes the seed), and in chat a short canned reply that says it is a test bot with random moves,
  a `Provisional:` line and one mark on the board. It is labelled as a test bot in the API
  (`is_model: false`, `knightlm.manifest_hash: "rbc-test-bot"`) and in the UI, never as the model.
- **KnightLM model**, managed by `apps/rusty-brick-chess/src/model_runtime.rs`:

| Route | What it does |
|---|---|
| `GET /api/knightlm/runtime` | `phase.state` is `not_installed`, `downloading` (`bytes`, `total`, `file`), `verifying`, `setting_up` (`step`), `ready`, `starting`, `running` (`url`) or `error` (`code`, `message`); also `python`, `python_download_bytes` (what a private Python would add, 0 if a usable one exists), `python_downloadable`, `device_note` (GPU or CPU), `provider_id`, `mode` (`real` or `fake`) |
| `POST .../install` | starts the download in the background and returns at once |
| `POST .../start` | starts `python -m model.serve.server`, waits for `/v1/models`, registers or updates the `knightlm` provider row |
| `POST .../stop` | stops it |

What it fetches is pinned in `apps/rusty-brick-chess/src/knightlm_manifest.json`: the nine base
files of `HuggingFaceTB/SmolVLM-500M-Instruct` at revision `a7da5b986cb59b408707209984f360a5f4ad7e47`
(Apache-2.0, `model.safetensors` is about 1 GB), each with its size and sha256, from Hugging Face; and
the adapter release asset `knightlm-run1-short-adapter.tar.gz` (sha256 `c67de2b9...`) from this
repository's GitHub release. Only those URLs are used. Files are downloaded to a partial directory
(resumed with a Range request after an interruption), hashed, moved into
`<RBC_DATA>/knightlm/` and marked installed last, so a partial or corrupt download never shows
as ready. The adapter archive is unpacked by hand and refused on `..`, absolute paths, links,
unlisted names or wrong hashes.

Python (`src/py_runtime.rs`). The model needs no Python on your machine. The interpreter that
builds the environment is chosen in this order, and the first one that qualifies wins:

1. `RBC_KNIGHTLM_PYTHON`, if set: used as is (any CPython 3.10 or newer); nothing is downloaded.
2. The app's **private Python** under `<RBC_DATA>/knightlm/python/`, if it is already there.
3. A **system Python 3.10 to 3.13** (`python3.13` ... `python3`, `python`, and on Windows the `py`
   launcher). A candidate counts only after it prints its own version, so the Windows Store
   `python.exe` stub (which opens the Store or exits 9009), a 3.9 and a 3.14 (no torch wheels yet)
   are skipped.
4. Otherwise **Get the model** downloads a pinned python-build-standalone CPython (3.12.16,
   `install_only` tarball from the `astral-sh/python-build-standalone` release `20261123`) for this
   OS and CPU, checks its sha256 from `knightlm_manifest.json`, unpacks it under
   `<RBC_DATA>/knightlm/python/` and uses that. It is also the fallback when a system Python cannot
   create a venv (Debian without `python3-venv`). Pinned: Linux x86_64 (glibc), Windows x86_64,
   macOS arm64 and x86_64. Other platforms (Linux arm64, musl) need their own Python 3.10 to 3.13.

Why private before system, and system before downloading: the private copy is the same build on
every machine and is never touched by OS updates, but it costs a download, so an existing suitable
Python is used rather than fetching a second one. A `python3` that already imports torch,
torchvision, transformers, peft, pillow, numpy, fastapi and uvicorn is used as it is (and the GPU too, if it
has CUDA: the server runs with `--device cuda` only then, else `--device cpu`). Otherwise the app
creates a venv under `<RBC_DATA>/knightlm/venv` and runs `pip install -r
model/serve/requirements.txt` (CPU torch wheel). Layout: `python/bin/python3` (Windows
`python/python.exe`), venv `bin/python` (Windows `Scripts/python.exe`). The order of work is Python,
then packages, then the weights, so the likelier failures cost the least to retry. The UI shows
`downloading` with byte counts for the Python and the weights, and `setting_up` with the step
(unpacking, creating the environment, the package being installed). If setup fails the error holds
the one command to run. The CPU is slow; a reply can take tens of seconds.

Size, measured on Linux x86_64 (the Play AI tab says "about 1.4 GB" before you press): Python
68 MB download (229 MB unpacked), packages 235 MB download (1.2 GB installed with torchvision, torch is 725 MB of it),
weights and adapter 1.09 GB download (972 MB on disk); about 1.4 GB downloaded and 2.4 GB on disk
in all. Windows and macOS packages differ (not measured).

The server is a child of the app. It watches its stdin, which the app holds open, so it exits
when the app does, however the app ends. Stop, and quitting the app, end it too. Its Python
code is found at `RBC_MODEL_CODE`, `<exe>/../share/rustybrickchess/model-runtime` (the installers),
the Tauri `model-runtime` resource, or the source tree. `packaging/model-runtime.files` lists the
files and `scripts/check_model_runtime_files.py` fails CI if they stop covering what the server imports.

Test and development switches, not for users: `RBC_KNIGHTLM_FAKE=1` (serve
`model.serve.fake_server`, no weights or torch), `RBC_KNIGHTLM_MANIFEST` (another manifest, may use
`file://` URLs), `RBC_KNIGHTLM_DEV_BASE_DIR` (copy the base files from a local directory instead of
downloading; they are still hash-checked), `RBC_KNIGHTLM_PYTHON`.

## Tests

- `apps/rusty-brick-chess/tests/test_bot.rs`, `knightlm_runtime.rs`: the test bot through the
  routes, and the runtime offline (pinned manifest, corrupt and partial downloads, tarball
  attacks, start and stop with the fake server, no orphan, routes).
- `apps/rusty-brick-chess/src/py_runtime.rs` and `src/engine_locate.rs` (unit tests): Python
  selection with a fake Windows Store stub, 3.9 and 3.14, layouts per OS, the archive unpacker;
  engine resolution order and the deb, rpm, AppImage, macOS and install.sh layouts as fixtures.
  `knightlm_runtime.rs` also covers the pinned Python entries and a fixture Python download.
- `apps/rusty-brick-chess/tests/knightlm_ondemand_real.rs` (ignored; real network, run by hand, see
  its header): the whole install with no system Python visible, optionally starting the real server.
- `apps/rusty-brick-chess/frontend/tests/cdp/cdp_play_ai.js`: the Play AI tab in headless Chrome,
  offline, with the test bot and the fake runtime.
- `apps/rusty-brick-chess/tests/model_wiring.rs`: a scripted fake OpenAI server on a
  loopback port, driven through the real router: provider settings, typed errors,
  seats, coach turn versus play harness, marks, rolling chat.
- `apps/rusty-brick-chess/frontend/tests/cdp/cdp_coach_board.js`: the same fake behind
  the real app in headless Chrome; arrows, pointer, chat line and the toggle.
- `board::coach_tests` (frontend crate): mark and pointer decoding.
