# blut-knightlm

The KnightLM cookbook for [BLUT](https://github.com/Quitetall/blut): one recipe,
`knightlm_lora`, from an audited `knightlm-lichess-v2` corpus to an evaluated,
registered image-text LoRA adapter. The binary is `blut-knightlm`.

```
materialize_corpus -> register_corpus -> claim_run -> hf_sft_train -> evaluate_checkpoint -> register_run
```

| stage | from | what it does |
|---|---|---|
| `materialize_corpus` | this crate (`python/knightlm_blut/materialize.py`) | runs `python3 -m model.corpus.pipeline audit` (a failure refuses), converts the train shards to `{"prompt","completion"}` with `model.prompt.user_text` and the played move, records per-shard hashes in the run ledger; re-hashes and reuses an existing result, else rebuilds |
| `register_corpus` | this crate, wraps the standard `register_dataset` | registers `knightlm-<corpus sha12>-<prompt sha8>`; a rerun with the same bytes is a no-op (the standard stage fails on an existing name) |
| `claim_run` | this crate | crash gate on `<run>/state.json` using BLUT's `decide_resume`: fresh, resume a dead or stale run, refuse a live one, refuse a finished one |
| `hf_sft_train` | `blut-cookbook-standard` | `arch = image_text`, LoRA, bf16, cosine + 3% warmup, clip 1, checkpoints every 500 steps (keep 3) in `<run>/checkpoints`, `resume = true`, base weights sha256 checked |
| `evaluate_checkpoint` | this crate | calls KnightLM's `python -m model.eval.run` (PR #18) with this adapter on the holdout; the scorecard path goes in the ledger; optional `baseline` runs `model.eval.compare` (per axis, no composite). Absent harness: recorded as `skipped` |
| `register_run` | this crate | copies the adapter to `<run>/adapter`, writes `run-manifest.json`, registers the adapter's sha256 in BLUT's model registry and points `model://knightlm-lora-<base>@candidate` at it |

Everything for a run lives in `/mnt/4tb/artifacts/KnightLM/expire/runs/<run>/`:
`ledger.jsonl` (append-only), `state.json`, `checkpoints/`, `adapter/`,
`run-manifest.json`.

## Use

Use `python3 -m model.train.run` (see `model/README.md`): it does the CPU
preflight and launches this recipe only with `--gpu`. Directly:

```sh
cd apps/blut-knightlm
oneoff cargo build --release            # target/release/blut-knightlm
export BLUT_HF_VENV_IMAGE_TEXT=/mnt/4tb/venvs/blut-hf-image-text   # see below
target/release/blut-knightlm recipe run knightlm_lora --args '{"run":"run1-a","base":"smolvlm-500m"}'
```

Args are in `blut-knightlm recipe show knightlm_lora`. `base` selects the pinned
base (`smolvlm-500m`, `qwen3_5-0.8b`; `src/profile.rs`), `lora_target_regex`
overrides the LoRA targets. A finished run is never extended: retry under a new
`run` name. A killed run is resumed by running the same command again.

## Environment

* **Image-text venv.** The trainer runs in its own venv (transformers 5.5.3,
  peft 0.21.2). Point `BLUT_HF_VENV_IMAGE_TEXT` at one on the NVMe disk or the
  engine builds it under `~/.local/share/blut` (root disk). The profile in
  blut-cookbooks PR #3 lacks **torchvision**, which transformers 5 needs for the
  SmolVLM image processor: a venv built from the profile alone fails with
  "Unrecognized image processor". Add `torchvision` (until PR #3 does).
* The binary points the standard trainer at its `hf_trainer_runner.py` from the
  cargo checkout of the pinned rev (`BLUT_HF_TRAINER_RUNNER` overrides).
* `KNIGHTLM_REPO`, `KNIGHTLM_BLUT_PAYLOAD`, `KNIGHTLM_PYTHON` (materializer's
  interpreter, default `python3`) and `KNIGHTLM_EVAL_PYTHON` (evaluation's, default
  the image-text venv) override locations.
* Isolating a smoke from your registries: `LAMU_TRAIN_JOBS_DIR`,
  `LAMU_TRAIN_DATA_DIR`, `LAMU_MEMORY_DB`, `BLUT_REGISTRY_DB`, `BLUT_REGISTRY_PATH`.

## Dependencies and placement

`Cargo.toml` pins `blut` to the published `=0.2.0-alpha.1` (the version
`blut-cookbook-standard` requires; lamu and lamquant still pin the older git rev
`e1bb077`, whose types do not unify with it) and `blut-cookbook-standard` to the
head of blut-cookbooks PR #3 (`agent/hf-trainer-image-text`).

**Re-pin `blut-backends` to the PR #3 merge commit on blut-cookbooks `main` once
it lands** (the branch is deleted on merge); also update `STANDARD_REV_SHORT` in
`src/runctx.rs` (a test checks it matches), then `cargo update -p blut-cookbook-standard`.

The crate lives here because `apps/*` is outside the KnightLM workspace (own
`[workspace]` and `Cargo.lock`). It is symlinked into the blut-cookbooks tree as
`knightlm`. Gate: `scripts/check_apps.sh` (fmt, clippy `-D warnings`, tests).
