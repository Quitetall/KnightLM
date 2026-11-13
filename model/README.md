# KnightLM model tooling

Rust owns the chess state and packet compiler. Python and Mojo include bounded
protocol parity checks. Python also hosts the Hugging Face pilot below.

- Python: strict packet boundary, canonical JSON and piece-placement expansion.
- Mojo 1.0.0: native move-counter/draw-status checks; CPython interoperability
  supplies JSON parsing and shared protocol shape validation. This is not an
  independent Mojo JSON parser or chess engine.

Install Mojo locally, without changing system Python:

```sh
uv venv model/parity/mojo/.venv
uv pip install --python model/parity/mojo/.venv/bin/python -r model/parity/mojo/requirements.txt
```

Run `python3 scripts/check_ports.py --require-mojo` from the repository root.
Without `--require-mojo`, absent Mojo is explicitly reported as unavailable.
The check uses temporary artifact directories and compares byte-exact canonical
packets from Rust and both ports. It also injects unknown fields and versions.

Mojo source targets https://mojolang.org/docs/manual/python/ version 1.0.0.

## Lichess LoRA pilot

From the repository root, use Python 3.11+, `zstd`, and a PyTorch installation
with CUDA and bfloat16 support. The pilot uses the pinned snapshot in
`model/configs/models/smolvlm-500m.json`. Put the model and output directories on SSD.
Use a new directory for each output.

```sh
timeout 300 cargo build --locked --release -p knightlm-cli --bin knightlm
timeout 300 uv pip install --python "$PYTHON" -r model/train/requirements.txt
timeout 300 hf download HuggingFaceTB/SmolVLM-500M-Instruct \
  config.json generation_config.json processor_config.json preprocessor_config.json \
  chat_template.json tokenizer.json tokenizer_config.json special_tokens_map.json model.safetensors \
  --revision a7da5b986cb59b408707209984f360a5f4ad7e47 \
  --local-dir "$MODEL_DIR"
timeout 220 "$PYTHON" -m model.corpus.pipeline build --pgn "$LICHESS_PGN" --output "$CORPUS_DIR"
timeout 600 "$PYTHON" -u -m model.train.harness --corpus "$CORPUS_DIR" \
  --model-dir "$MODEL_DIR" --output "$RUN_DIR"
```

Set `PYTHON` to the interpreter in the CUDA environment and `LICHESS_PGN` to
the existing monthly `.pgn.zst`. The builder uses a bounded prefix of the
stream and compiles one decision at ply 24 for each selected rated standard
blitz or rapid game with both players rated 1600 or above. It writes 1,000
training games and 100 strictly later holdout games by default, in shards of
`--shard-games` games, and records the source file's size and SHA-256, the
compiler's SHA-256, each shard's hash and game count, and each split's months
and time range in `manifest.json`. The reader rechecks shard hashes and counts,
refuses shards the manifest does not list, and refuses game ID, ECO-prefix and
scored-position leakage across the split. Re-check a finished corpus without
rebuilding it:

```sh
"$PYTHON" -m model.corpus.pipeline audit "$CORPUS_DIR"
```

Run 1 is 200,000 training games and 10,000 later holdout games drawn from a
400,000-candidate prefix of the monthly file:

```sh
"$PYTHON" -m model.corpus.pipeline build --pgn "$LICHESS_PGN" --output "$CORPUS_DIR" \
  --train-games 200000 --holdout-games 10000 --candidates 400000
```

Keep the shards in the artifact store, not in git.

The trainer uses the checkpoint's chat template and masks loss on the prompt
and padding. It runs one epoch, checks for finite gradients and adapter updates,
then reloads the adapter and checks logits on a holdout prompt. The run manifest
binds the adapter and predictions to the base revision and corpus. The pilot
uses played moves as targets and supplies the compiler's legal-move list in the
prompt. Holdout generation is unconstrained. Teacher labels, strength testing and
image inputs are outside the pilot.

Use `--batch-size 4` or lower if GPU memory is limited.

## Talking to a checkpoint (serving)

`model/serve` loads the pinned base plus an optional LoRA adapter, so the
untrained base (a freshly initialised, no-op adapter) and trained checkpoints
go through one path. The prompt lives in `model/prompt.py` and is shared with
the trainer. Nothing in `model/serve` reaches an engine.

```sh
# one-off: environment and base weights (keep caches on /mnt/4tb)
uv venv /mnt/4tb/venvs/knightlm-serve
uv pip install --python /mnt/4tb/venvs/knightlm-serve/bin/python -r model/train/requirements.txt   torch torchvision fastapi uvicorn httpx accelerate pillow num2words huggingface_hub
hf download HuggingFaceTB/SmolVLM-500M-Instruct config.json generation_config.json processor_config.json   preprocessor_config.json chat_template.json tokenizer.json tokenizer_config.json special_tokens_map.json   model.safetensors --revision a7da5b986cb59b408707209984f360a5f4ad7e47   --local-dir /mnt/4tb/artifacts/KnightLM/expire/models/smolvlm-500m-a7da5b98

# serve (constrained: always a legal move; --mode raw: free decode, illegal reply is an HTTP 502)
BASE=/mnt/4tb/artifacts/KnightLM/expire/models/smolvlm-500m-a7da5b98
CKPT=/mnt/4tb/artifacts/KnightLM/keep/model-provisional-base-2026-11-13
/mnt/4tb/venvs/knightlm-serve/bin/python -m model.serve.server \
  --base-dir $BASE --adapter-dir $CKPT/adapter --mode constrained --port 8000

# UCI engine for a GUI (point the GUI's engine command at this line)
timeout 600 cargo build --release -p knightlm-cli --bin knightlm-uci --bin knightlm-match
target/release/knightlm-uci --player model --model http://127.0.0.1:8000/v1/move

# or a headless game against the random player
target/release/knightlm-match --white model --black random --model http://127.0.0.1:8000/v1/move \
  --games 1 --max-plies 100 --pgn /path/to/new/game.pgn
```

`GET /v1/manifest` reports the base, adapter, prompt, tokenizer and chat-template
hashes and the manifest hash returned with every move. `model/serve/make_provisional.py`
writes a no-op adapter and proves it equals the base; `model/serve/probe_holdout.py`
records raw and constrained behaviour on holdout positions. Tests:
`python -m unittest model.serve.test_serve` (the real-model test runs only with
`KNIGHTLM_BASE_DIR` set and CUDA present).

## KnightLM development checks

`make check` exercises the Rust workspace, Python packet checks, and fixture
CLI. The following commands run the model-specific checks from the repo root:

```sh
cargo run --locked -p knightlm-cli --bin knightlm -- compile fixtures/repetition.json
cargo run --locked -p knightlm-cli --bin knightlm -- evaluate-fixtures fixtures/start.json
cargo run --locked -p knightlm-cli --bin knightlm -- train-smoke /path/to/new/cpu-smoke.json
cargo run --locked -p knightlm-cli --bin knightlm -- verify-model model/configs/models/smolvlm-500m.json
```

Fixture evaluation exercises the experiment arms with scripted moves and
example scores. The CPU smoke fits a two-weight regression and verifies
finite gradients, an update, and checkpoint reload. The Burn admission command
exits nonzero until the reference parity gates pass. See the [model plan](../docs/model/plan.md)
and [contracts](../docs/model/contracts.md) for the experiment and its evidence limits.

## Training through BLUT

`model/train/harness.py` above is the pilot and reference trainer: one epoch,
whole corpus in memory, no checkpoints. The Run 1 path is the BLUT recipe
`knightlm_lora` in `apps/blut-knightlm` (see its README), which streams the
sharded corpus, checkpoints and resumes, evaluates and registers the adapter.
`model/train/run.py` is its entry point: it checks everything on CPU, prints the
plan, and launches the recipe only with `--gpu`. The recipe's tokenisation is
checked token for token against `harness.py`'s `encode`, and its prompt text is
`model.prompt` (the text the model is served with), so the reference trainer and
the BLUT path train on the same prompts.

Setup once (venv on the NVMe disk; `torchvision` is needed by the SmolVLM
processor and is missing from the engine's own profile):

```sh
uv venv /mnt/4tb/venvs/blut-hf-image-text --python 3.12
uv pip install --python /mnt/4tb/venvs/blut-hf-image-text/bin/python \
  transformers==5.5.3 peft==0.21.2 "accelerate>=1.0,<2" "safetensors>=0.4,<1" \
  "torch>=2.4,<3" torchvision "pillow>=10" num2words
echo -n 1 > /mnt/4tb/venvs/blut-hf-image-text/.blut_hf_venv_version
export BLUT_HF_VENV_IMAGE_TEXT=/mnt/4tb/venvs/blut-hf-image-text
(cd apps/blut-knightlm && oneoff cargo build --release)
PY=$BLUT_HF_VENV_IMAGE_TEXT/bin/python
CORPUS=/mnt/4tb/artifacts/KnightLM/expire/corpus/run1-2026-09-a
```

**Preflight** (CPU, launches nothing): audits the corpus, checks the base weights
against the pinned digest, checks the training prompt against `model.prompt`,
tokenises 64 rows with the trainer's own code, compares that with `harness.py`,
estimates steps, time and VRAM, takes two optimiser steps on CPU, and lists every
deviation from the recommended defaults. A tampered shard or a leak is refused
(exit 2) before anything else.

```sh
timeout 900 $PY -m model.train.run --corpus $CORPUS --run run1-a
```

Use a new `--run` for every attempt; an existing run directory is refused, and
`--resume` continues a crashed run of that name. Without a model on the machine
(CI, fixtures) add `--skip-model`: those checks are then listed as `skipped`,
never as passed.

**Joined smoke** (slice of Run 1 -> audited snapshot -> materialize -> 30 GPU
steps -> evaluate 50 holdout rows -> register; stays under 5 GB of VRAM):

```sh
timeout 1500 $PY -m model.run_pipeline --work /mnt/4tb/tmp/knightlm-smoke --gpu --steps 30 --eval-rows 50 \
  --eval-repo <checkout that has model/eval> --eval-arg=--no-stockfish --eval-arg=--no-tactics
```

Without `--gpu` it stops at the plan. `--eval-repo` is needed until the
evaluation harness (PR #18) is merged; without it the evaluate stage is recorded
as `skipped`. Crash and resume (kill the recipe mid-training, rerun, compare):
`$PY -m model.tests.crash_resume --work DIR --blut-bin apps/blut-knightlm/target/debug/blut-knightlm`
with `LAMU_TRAIN_JOBS_DIR` set. CPU tests: `python3 -m unittest model.tests.test_materialize
model.tests.test_train_run model.tests.test_pipeline`.

**Full Run 1** (not run here). 200,000 rows at an effective batch of 32 is 6,250
optimizer steps and about 99 M tokens. Batch 8 x 4 peaked at 14.8 GB in the pilot
and batch 4 x 8 at 7.9 GB in the BLUT smoke, so on the shared GPU use the latter
(~2 h at the measured 14-26 k tokens/s). Detach it and wait on it explicitly:

```sh
systemd-run --user --unit=knightlm-run1 --setenv=BLUT_HF_VENV_IMAGE_TEXT=$BLUT_HF_VENV_IMAGE_TEXT \
  $PY -m model.train.run --corpus $CORPUS --run run1-a --batch-size 4 --grad-accum 8 --gpu \
  --eval-rows 1000 --baseline /mnt/4tb/artifacts/KnightLM/keep/<base-scorecard-dir>
```

Results are in `/mnt/4tb/artifacts/KnightLM/expire/runs/run1-a/`: `entry.json`
(the exact command, corpus digest, model revision, seed, settings, status),
`run-manifest.json`, `ledger.jsonl`, `adapter/`. If the process dies, run the same
command with `--resume`.

**Qwen 3.5.** `--base qwen3_5-0.8b` (local snapshot in `/mnt/4tb/models`) uses the
same recipe; `--lora-target-regex` overrides the default targets (q/v of the full
attention layers and the fused qkv projection of the linear-attention layers).
Only a 5-step smoke was run. `flash-linear-attention` and `causal-conv1d` are not
installed, so the Gated DeltaNet layers use the slow torch fallback (planning
figure 300 tokens/s; the smoke saw 460-1,000 at batch 2) and VRAM peaked at
10.6 GB at batch 2 because of the 248k-token vocabulary. Evaluation is skipped for
this base: the serving predictor pins SmolVLM.

Two identical uninterrupted runs are not bitwise equal on this GPU, so a resumed
run is compared with the run-to-run noise, not for identity (see
`model/tests/crash_resume.py`).

