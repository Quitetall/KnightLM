# Training gate: S0 board-grounding arms (prepared, not run)

Status: nothing in this document has been launched. Every command below is prepared; each GPU command
needs an explicit "go" from the owner. This page covers the S0 rules-QA arms T0, T2-lite, T2-full, T1 and
T3. The scoring commands for the move-policy head and the board-encoder arms (T4/T5) are at the end of this
page; reasoning v2 is added by its own change.

## What the S0 arms test

One question per position about the board (side to move, piece on a square, attackers, defenders, legal
moves of a piece, check, captures, material), answered in the `reasoning` field of a `submit_move` call.
Every answer comes from the compiler (`knightlm cgt`, `knightlm corpus-facts`, the packet's legal-move
list); nothing from an engine. The six corpora share row ids, questions and answers (`matched_sha256`
`ce5b52dd1efa21bc...`), so the arms differ only in how the position is written in the prompt
(docs/model/graph-text.md, docs/model/tokenizer.md).

Format v2 fixes the two review findings of the first pilot: the completion's `move` is the first legal
move in canonical order (`min(legal_moves)`), never the human move, so the probe does not train move
imitation; and `attackers_of`/`defenders_of` are reported apart from the other types
(`s0_rules_qa.groups.edge` against `.groups.node`), with a like-for-like edge arm (`t2-full`).

| arm | name | prompt kind | position in the prompt | corpus (under `/mnt/4tb/artifacts/KnightLM/expire/corpus/`) |
|---|---|---|---|---|
| T0 | control | `play` | FEN packet JSON | `s0-rules-qa-pilot-2026-11b-t0` |
| T2-lite | cgt-lite text | `play-cgt` | node list and legal moves per piece (no edges) | `s0-rules-qa-pilot-2026-11b-t2` |
| T2-full | cgt-full text | `play-cgt-full` | lite plus `atk`, `def`, `pin`, `hang`, `check`, `ksafe` | `s0-rules-qa-pilot-2026-11b-t2-full` |
| T1 | board tokens | `play-t1` | ctok/1: 64 piece tokens plus legal-move tokens | `s0-rules-qa-pilot-2026-11b-t1` |
| T3 | cgt-lite tokens | `play-t3` | ctok/1 ids of cgt-lite | `s0-rules-qa-pilot-2026-11b-t3` |
| T3-full | cgt-full tokens | `play-t3-full` | ctok/1 ids of cgt-full | `s0-rules-qa-pilot-2026-11b-t3-full` |

Evidence (audits, token costs, preflight plans, CPU tests):
`/mnt/4tb/artifacts/KnightLM/keep/s0-pilot-2026-11b/`. Each corpus has 20,000 train and 1,000 holdout rows;
`python -m model.corpus.pipeline audit DIR --knightlm BIN` accepted all six (every answer, prompt and
completion re-derived from the compiler). The first pilot (`s0-rules-qa-pilot-2026-11-t0`, `-t2`, format v1)
is kept unchanged and is superseded.

## Tokens and time

Trainer tokenisation of a 64-row sample (`model.train.run` preflight, which runs the BLUT trainer's own
`encode`), 625 optimizer steps of 32 rows (batch 8 x accumulation 4, one epoch over 20,000 rows, seq_len
2048, no row dropped). Time is tokens divided by the planning throughput of the pinned profile
(14,000 tokens/s on the shared RTX 4090, a conservative figure; the measured range is 6.5k to 26k).

| arm | tokens/row mean (p95, max) | total train tokens | train time at 14k tok/s |
|---|---|---|---|
| T0 | 1042.8 (1194, 1291) | 20.86M | 0.41 h |
| T2-lite | 807.9 (842, 857) | 16.16M | 0.32 h |
| T2-full | 1076.1 (1154, 1170) | 21.52M | 0.43 h |
| T1 | 689.0 (707, 723) | 13.78M | 0.27 h |
| T3 | 731.3 (754, 765) | 14.63M | 0.29 h |
| T3-full | 925.7 (983, 988) | 18.51M | 0.37 h |

About 700 tokens of every prompt are the fixed play-schema prefix, so the position is the smaller part.
Reference: the Run 1 short reasoning run (625 steps, 20,000 rows, same recipe) took 24 minutes from start to
adapter registered, including materialisation and the evaluation, with a peak of 14.5 GB GPU memory.
Expect 20 to 35 minutes per arm end to end plus about 5 minutes for the 1,000-row S0 evaluation.

Planning budget for T0, T2-lite and T2-full together: about 1.2 hours of GPU time for training.

## Resource check (2026-11-20, before any launch)

Run these immediately before a launch; do not trust the numbers below.

```bash
nvidia-smi --query-gpu=name,memory.used,memory.total --format=csv   # 24 GB card; 2.5 GB in use at the check
free -g | head -2                                                   # 62 GB RAM, about 26 GB available
df -h / /mnt/4tb | tail -2                                          # / 119 GB free (88%); /mnt/4tb 239 GB free (94%)
```

A run writes a few hundred MB under `/mnt/4tb/artifacts/KnightLM/expire/runs/<run>` (adapter, three kept
checkpoints with optimizer state, logs). Stop if either disk is above 90% and the run is larger than 10 GB;
none of these is. The GPU is shared; a 14.5 GB peak needs the card otherwise quiet.

## Prerequisites (once)

```bash
cd /mnt/4tb/KnightLM                       # main after the PR that adds this page is merged
oneoff cargo build --release -p knightlm-cli
(cd apps/blut-knightlm && oneoff cargo build --release)
export KNIGHTLM_BIN=$PWD/target/release/knightlm
export BLUT_HF_VENV_IMAGE_TEXT=/mnt/4tb/venvs/blut-hf-image-text
```

The corpus manifests record the rules revision (core tree `16290625df7c`, notation tree `826802c6c8d8`) and
the compiler binary hash; a different compiler is refused by the audit unless `--allow-rules-drift`.

## Training commands

Each command first prints a CPU preflight plan and launches only with `--gpu`. Run without `--gpu` first
(it checks the corpus audit, prompt parity, the pinned base weights, tokenisation, the trainer's encode
against the harness's, and two CPU optimiser steps) and read it.

### T0 (control)

```bash
python -m model.train.run --corpus /mnt/4tb/artifacts/KnightLM/expire/corpus/s0-rules-qa-pilot-2026-11b-t0 \
  --run s0b-t0 --baseline <t0 untrained scorecard dir> --eval-arg=--no-stockfish --eval-arg=--no-tactics --gpu
```

### T2-lite

```bash
python -m model.train.run --corpus /mnt/4tb/artifacts/KnightLM/expire/corpus/s0-rules-qa-pilot-2026-11b-t2 \
  --run s0b-t2 --baseline <t2 untrained scorecard dir> --eval-arg=--no-stockfish --eval-arg=--no-tactics --gpu
```

### T2-full

```bash
python -m model.train.run --corpus /mnt/4tb/artifacts/KnightLM/expire/corpus/s0-rules-qa-pilot-2026-11b-t2-full \
  --run s0b-t2-full --baseline <t2-full untrained scorecard dir> --eval-arg=--no-stockfish --eval-arg=--no-tactics --gpu
```

All three use the recommended recipe unchanged (rank 8, alpha 16, lr 2e-4, cosine, seed 42), so the arms
differ only in the corpus. The adapter lands in `<run root>/<run>/adapter`; the recipe evaluates the 1,000
holdout rows and writes the scorecard, and compares to `--baseline` when given.

### T1 and T3 (preflight ready, training blocked)

```bash
python -m model.train.run --corpus .../s0-rules-qa-pilot-2026-11b-t1 --run s0b-t1 --tokenizer ctok1      # preflight
python -m model.train.run --corpus .../s0-rules-qa-pilot-2026-11b-t3 --run s0b-t3 --tokenizer ctok1
python -m model.train.run --corpus .../s0-rules-qa-pilot-2026-11b-t3-full --run s0b-t3-full --tokenizer ctok1
```

With `--tokenizer ctok1` the preflight extends the SmolVLM tokenizer by the 2,496 ctok/1 tokens (`base =
49280`, tokenizer length 51,776), checks `prompt_parity`, `tokenisation` and `trainer_vs_harness_tokens`
(the ctok ids in the trainer's prompt equal the id-level encoding at `base + id`, no ctok id in a completion,
and the serve path's chat-template tokenisation equals the trainer's prompt ids), and takes two CPU steps
with the new rows trainable. `--gpu` is refused with the reason. What the GPU run needs, in order:

1. In blut-cookbooks (separate repository, pinned by `apps/blut-knightlm/Cargo.toml`): the image-text trainer
   builds `peft.LoraConfig` without `trainable_token_indices` and rejects `spec.extra`. Add a spec field that
   passes `trainable_token_indices={"embed_tokens": ids, "lm_head": ids}` (ids `49280..51775`, from
   `model.train.ctok.trainable_token_indices`) and accepts an extended base snapshot.
2. Build the extended base once (CPU, about 1 GB):
   `python -m model.train.ctok build-base --base-dir /mnt/4tb/artifacts/KnightLM/expire/models/smolvlm-500m-a7da5b98 --out /mnt/4tb/artifacts/KnightLM/expire/models/smolvlm-500m-ctok1-<hash>`
   (records `ctok-extension.json`: vocabulary hash, seed, both weight hashes).
3. A BLUT profile for that base (`apps/blut-knightlm/src/profile.rs` pins the stock weights hash) and the
   tokenizer manifest check on load.

Embedding-extension path: `peft` `trainable_token_indices`, tested on CPU (`model/train/test_ctok.py`) for
identical logits after saving the adapter and reloading it on a freshly loaded extended base, and for host
rows that never move. Trainable parameters on SmolVLM-500M, from the preflight CPU steps
(`preflight-t1.json`): LoRA 819,200 + new embedding rows 2,396,160 + new `lm_head` rows 2,396,160 =
**5,611,520**. The rejected alternative, `modules_to_save=["embed_tokens", "lm_head"]`, adds 99,409,920
instead of 4,792,320 (20.7x). Do not use it. A serving layer should mask the ctok ids when sampling.

## Evaluation and comparison commands

Evaluation scores the 1,000 holdout rows of the arm's own corpus: exact answer match per question type, the
node and edge groups, no engine, no composite score. A reply that does not parse is a failure on that row.

```bash
export KNIGHTLM_BASE_DIR=/mnt/4tb/artifacts/KnightLM/expire/models/smolvlm-500m-a7da5b98
export KNIGHTLM_DEVICE=cuda
# baseline: the untrained base on the arm's own prompt (prepared; GPU inference)
KNIGHTLM_ADAPTER_DIR= python -m model.eval.run --predictor model.eval.playmode:from_env \
  --corpus /mnt/4tb/artifacts/KnightLM/expire/corpus/s0-rules-qa-pilot-2026-11b-t0 --label s0b-t0-base \
  --rows 1000 --no-stockfish --no-tactics
# a trained arm (what the recipe already does after training; use this to re-run on an adapter)
KNIGHTLM_ADAPTER_DIR=/mnt/4tb/artifacts/KnightLM/expire/runs/s0b-t0/adapter python -m model.eval.run \
  --predictor model.eval.playmode:from_env \
  --corpus /mnt/4tb/artifacts/KnightLM/expire/corpus/s0-rules-qa-pilot-2026-11b-t0 --label s0b-t0 \
  --rows 1000 --no-stockfish --no-tactics
```

Repeat for `t2` and `t2-full` (baselines use the same corpus names). Compare trained arm against baseline
and arms against each other; cards of one matched S0 corpus compare without a flag:

```bash
python -m model.eval.compare <s0b-t0-base dir> <s0b-t0 dir>          # did training help on T0
python -m model.eval.compare <s0b-t0 dir> <s0b-t2 dir>               # T0 vs T2-lite
python -m model.eval.compare <s0b-t0 dir> <s0b-t2-full dir>          # T0 vs T2-full
```

Read the result on two groups: `s0_rules_qa.groups.node` across all arms, and `s0_rules_qa.groups.edge` only
between arms that print the edges (T2-full and T3-full do; T0 and T2-lite do not, so their edge numbers
show what the model computes without them). Per-type rates are in `by_type`. A lower
`claim_failure_rate` is better.

Eval dirs go to the keep root by default (`/mnt/4tb/artifacts/KnightLM/keep/eval-<label>-<stamp>`); pass
`--out-root` to put exploratory runs elsewhere. T1 and T3 are evaluated the same way with
`KNIGHTLM_BASE_DIR` set to the extended base and `KNIGHTLM_ADAPTER_DIR` to the trained adapter; the
predictor needs the extended tokenizer (it loads it from the base directory) and has not been run against a
ctok adapter. T1 and T3 compare against the trained T0 and T2 arms, not against an untrained base (their
new rows are meaningless before training).

## Recommended order

1. T0 and T2-lite (matched pair on the node group, 0.7 h GPU), then T2-full (0.43 h): do the S0 questions
   improve when the position is a graph, and do the edge questions need the edges printed.
2. T1/T3 only after the cookbook change above lands, as the second question: does a board-native token
   input ground better than text.

## Not tested

No GPU run of any arm. The BLUT recipe's S0 evaluation path and `--baseline` compare on S0 corpora are
exercised by unit tests with fakes, not against a trained adapter. T1/T3: no GPU training, no evaluation of
an adapter with new rows, and the serve-side predictor on an extended base is untested.

## Scoring the move-head and board-encoder arms (prepared, not run)

Nothing here has run. The prompts, predictors and eval wiring are tested on a tiny random CPU host and with
fake predictors; the commands below need trained artifacts that do not exist yet (the joint adapter and head
run, the pretrained encoder and the aligned projector, the T4/T5 adapters) and a GPU for the 500M host. Each
needs the owner's go. They score on the move holdout (`--corpus` default `run1-2026-09-a`), the same rows and
scorecard as the Run 1 control, so arms compare on one table.

| arm | prompt kind | what the model sees | artifacts it needs |
|---|---|---|---|
| T0 control | `play` | FEN packet | the Run 1 reasoning adapter |
| S1 human-only move head | `play-head` | FEN packet, head-read line, head's move | adapter + head (`save_head`) |
| S1 warm-up probe | `play-head` | the same, frozen host | head only (no adapter) |
| T4 encoder | `play-enc` | 64 soft tokens + rule facts | adapter + encoder + projector |
| T5 encoder + cgt | `play-enc-cgt` | soft tokens + rule facts + cgt-lite text | adapter + encoder + projector; `knightlm` binary |

```bash
export KNIGHTLM_BASE_DIR=/mnt/4tb/artifacts/KnightLM/expire/models/smolvlm-500m-a7da5b98
export KNIGHTLM_DEVICE=cuda
RUNS=/mnt/4tb/artifacts/KnightLM/expire/runs      # <...> below are the not-yet-existing run directories
EVAL="--rows 300 --oracles lc0 maia"              # as the Run 1 play-mode evaluation; add --games N for matches

# T0 control (exists once the Run 1 adapter is at hand)
KNIGHTLM_ADAPTER_DIR=$RUNS/<run1-reasoning>/adapter python -m model.eval.run \
  --predictor model.eval.playmode:from_env --prompt-kind play --label t0-control $EVAL

# S1 human-only move head: the head's move scored beside what the completion wrote (agreement)
KNIGHTLM_ADAPTER_DIR=$RUNS/<s1>/adapter KNIGHTLM_HEAD_DIR=$RUNS/<s1>/head python -m model.eval.run \
  --predictor model.eval.playmode:head_from_env --prompt-kind play-head --move-head --label s1-head $EVAL

# S1 warm-up probe (frozen host: empty adapter, head only): the head axis is the measurement; completions are noise
KNIGHTLM_ADAPTER_DIR= KNIGHTLM_HEAD_DIR=$RUNS/<s1-warmup>/head python -m model.eval.run \
  --predictor model.eval.playmode:head_from_env --prompt-kind play-head --move-head --label s1-warmup \
  --rows 300 --modes raw --no-tactics

# T4 board encoder (play-enc) and T5 board encoder + cgt text (play-enc-cgt)
ENC="KNIGHTLM_ENCODER_DIR=$RUNS/encoder-p0-a/encoder KNIGHTLM_PROJECTOR_DIR=$RUNS/projector-smolvlm-a"
env $ENC KNIGHTLM_ADAPTER_DIR=$RUNS/<t4>/adapter python -m model.eval.run \
  --predictor model.eval.playmode:from_env --prompt-kind play-enc --label t4-encoder $EVAL
env $ENC KNIGHTLM_ADAPTER_DIR=$RUNS/<t5>/adapter python -m model.eval.run \
  --predictor model.eval.playmode:from_env --prompt-kind play-enc-cgt --label t5-encoder-cgt $EVAL
```

Compare on the one table. Cards of different prompt kinds measure different tasks, so every pair below needs
`--allow-prompt-mismatch`; the rows, Stockfish depth and tactical set are still checked and a difference there
still refuses. Read the arms side by side, never summed:

```bash
python -m model.eval.compare --allow-prompt-mismatch <t0-control dir> <s1-head dir>    # head vs the language move
python -m model.eval.compare --allow-prompt-mismatch <t0-control dir> <t4-encoder dir>
python -m model.eval.compare --allow-prompt-mismatch <t4-encoder dir> <t5-encoder dir> # does cgt text add to soft tokens
```

What to read: for S1, `axes.move_head` (masked top-1/top-3, NLL, ECE, CPL; raw legality and illegal mass as the
diagnostic) beside `axes.head_agreement` (does the written reasoning agree with the head's move) and
`axes.play_explanation` (are its claims true); human match is not strength, so read it with the CPL against the
control (CPL 409, random floor 417). For T4/T5 the same play axes as T0, plus `play_explanation` claim failures:
the encoder's reason to exist is that a board-grounded model stops asserting false piece placements (Run 1: 65%).
All numbers carry the arm label ("human-only move head" for S1).

Serving the same artifacts (not part of scoring): `python -m model.serve.server --base-dir DIR --adapter-dir DIR
--head-dir DIR` or `--encoder-dir DIR --projector-dir DIR --play-prompt-kind play-enc`.

What the training runs must satisfy for these commands to be valid (owed, not written): the S1 adapter and head
are trained on the two `play-head` views (head read at the last token of the head-read prompt,
`hidden_states[-1]`; completion on the conditioned prompt) with the same rows, steps and LoRA as T0; the T4/T5
adapters are trained on `play-enc`/`play-enc-cgt` rows with the projector trainable and the encoder frozen,
through the `image_hidden_states` path. Not tested: any of this on the real SmolVLM, GPU memory and latency of
soft-token decoding, and `stop_strings` with embedding-input decoding on the real tokenizer.
