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
timeout 220 "$PYTHON" -m model.corpus.pipeline --pgn "$LICHESS_PGN" --output "$CORPUS_DIR"
timeout 600 "$PYTHON" -u -m model.train.harness --corpus "$CORPUS_DIR" \
  --model-dir "$MODEL_DIR" --output "$RUN_DIR"
```

Set `PYTHON` to the interpreter in the CUDA environment and `LICHESS_PGN` to
the existing monthly `.pgn.zst`. The builder uses a bounded prefix of the
stream and compiles one decision at ply 24 for each selected rated standard
blitz or rapid game with both players rated 1600 or above. It writes 1,000
training games and 100 strictly later holdout games by default. The reader
rechecks shard hashes and refuses game ID, ECO-prefix and scored-position
leakage across the split.

The trainer uses the checkpoint's chat template and masks loss on the prompt
and padding. It runs one epoch, checks for finite gradients and adapter updates,
then reloads the adapter and checks logits on a holdout prompt. The run manifest
binds the adapter and predictions to the base revision and corpus. The pilot
uses played moves as targets and supplies the compiler's legal-move list in the
prompt. Holdout generation is unconstrained. Teacher labels, the full Run 1
corpus, strength testing and image inputs are outside the pilot.

Use `--batch-size 4` or lower if GPU memory is limited.

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
