# Training gate: every arm, in run order (prepared, not run)

Status: only Run 1 short has trained (625 steps, `keep/eval-run1short-playmode-300-20261120T221837Z`). Everything below is
prepared and its CPU preflight has run; each GPU command needs the owner's explicit "go" for that command. Numbers are
tagged **M** (measured: tokenisation of real rows, a CPU run, the Run 1 short run) or **U** (unmeasured: planned from M
figures; re-measure on the first 200 steps of the first run, see "Run order and stop rules").

Planning throughput: 14,000 tokens/s on the shared RTX 4090 (measured range 6.5k to 26k); Run 1 short took 24 min for
625 steps x 32 rows (**M**), peak 14.5 GB GPU (**M**). "Time" below is tokens / 14k unless a block says otherwise.

## All arms

Rows are in block order (A0 to A11); `step` is the recommended run order (see "Run order and stop rules"). `tok/row` is the
trainer's own tokenisation of real rows.

| step | block | arm | what it answers | tok/row (M) | GPU time (U) | CPU preflight | compare against |
|---|---|---|---|---|---|---|---|
| 0 | A0 | baselines | what the untrained base does on each S0 prompt | n/a | 5 min each | n/a | n/a |
| 1 | A1 | T0 (S0 control) | S0 rules QA with the FEN packet | 1042.8 | 0.41 h | audit, parity, tokens (M) | S0 untrained T0 |
| 1 | A2 | T2-lite | do node questions improve when the board is a graph | 807.9 | 0.32 h | same | T0 trained |
| 1 | A3 | T2-full | do edge questions need the edges printed | 1076.1 | 0.43 h | same | T0 trained, T2-lite |
| 6 | A4 | T1 | board tokens (ctok/1) instead of text | 689.0 | 0.27 h | same + 2 steps on the extended base (M) | T0 trained |
| 6 | A4 | T3, T3-full | cgt-lite, cgt-full as ctok/1 ids | 731.3, 925.7 | 0.29 h, 0.37 h | same | T2-lite, T2-full trained |
| 2, 7 | A5 | S1 move head (warm-up, then joint) | does the last hidden state carry the human move | 990 (head-read) | 3-5 min, 25-37 min | six checks + 2 real-host steps (M) | Run 1 short, random floor |
| 3 | A6 | board-encoder pretrain (P0) | can a 5M encoder learn the rules from labels | n/a | 20-40 min | `--tiny` CPU run (M) | its own `axis.json` gate |
| 4 (opt.) | A7 | projector align (P1) | warm start of the projector | about 140 | 10-30 min | writer round trip (M) | its own loss |
| 4 | A8 | T4, T5 (adapter + projector) | does a board-grounded model stop false piece claims | 940.6, 1177.6 | 0.37 h, 0.47 h | all checks + 2 real-host steps (M) | Run 1 short |
| 5 | A9 | reasoning v2a | short compiler-grounded reasoning | 1071.7 | 0.42 h | audit, parity, tokens | Run 1 short |
| 5 | A10 | reasoning v2b | quoting read-tool results, loss on calls only | 1331.3 | 0.53 h | all checks + 2 real-host steps (M) | Run 1 short, v2a |
| 8 | A11 | perceiver | image to placement (a separate model) | n/a | 10-20 min GPU | unit smoke only | its own scorecard |

Every LLM arm trains 20,000 rows, 625 steps of 32 (batch 8 x accumulation 4), seq_len 2048, LoRA rank 8, alpha 16, lr 2e-4,
cosine, seed 42: the recipe of Run 1 short, so arms differ in the data and the model path only.

## Before any launch

Run these immediately before each launch; do not trust numbers from an earlier check. Last look (2026-11-20): GPU 4090, 24 GB with
3 GB in use; RAM 62 GB with 24 GB available; `/` 83 GB free (93%), `/mnt/4tb` 197 GB free (95%).

```bash
nvidia-smi --query-gpu=name,memory.used,memory.total --format=csv   # the card is shared
free -g | head -2                                                   # CPU preflights load the 500M host twice in float32: about 8 GB
df -h / /mnt/4tb | tail -2                                          # keep 80 GB free on /; stop above 90% if a run is over 10 GB
```

| arm | GPU free needed (`--gpu` refuses below) | run writes | note |
|---|---|---|---|
| S0 arms, v2a (`model.train.run`) | 16 GB (peak 14.5 GB, M) | a few hundred MB | the recipe path |
| S1 (`model.policy_head.train`) | 12 GB (plan about 10, U) | adapter, head, snapshots | refuses under 5 GB disk |
| T4/T5 (`model.encoder.task`) | 10 GB (plan about 8, U) | adapter, projector, rows, resume state | needs a pretrained `--encoder` |
| v2b (`model.train.masked`) | 16 GB (plan about 15, U) | adapter, rows, resume state | |
| P0 / P1 | 4 GB / 10 GB (U) | encoder, projector (MB) | |

Once, from the repository, and the shorthand the blocks below use:

```bash
cd /mnt/4tb/KnightLM                                   # main, after this page is merged
oneoff cargo build --release -p knightlm-cli           # labels, cgt text and corpus audits need the compiler
(cd apps/blut-knightlm && oneoff cargo build --release)
export KNIGHTLM_BIN=$PWD/target/release/knightlm
export BLUT_HF_VENV_IMAGE_TEXT=/mnt/4tb/venvs/blut-hf-image-text
PY=$BLUT_HF_VENV_IMAGE_TEXT/bin/python
C=/mnt/4tb/artifacts/KnightLM/expire/corpus; R=/mnt/4tb/artifacts/KnightLM/expire/runs; K=/mnt/4tb/artifacts/KnightLM/keep
BASE=/mnt/4tb/artifacts/KnightLM/expire/models/smolvlm-500m-a7da5b98
S0=s0-rules-qa-pilot-2026-11b                           # corpora $C/$S0-t0, -t2, -t2-full, -t1, -t3, -t3-full
```

The corpus manifests record the rules revision (core tree `16290625df7c`, notation tree `826802c6c8d8`) and the compiler hash; a
different compiler is refused by the audit unless `--allow-rules-drift`.

Resume. The loops of `model.policy_head.train`, `model.encoder.task` and `model.train.masked` write `<run>/resume/state.pt` every
`--save-steps` (500) steps; `--resume` (same `--run`, same settings, else refused) continues from it: weights, optimizer, scheduler,
row-stream position and RNG are restored. The recipe (`model.train.run --resume`) keeps its own checkpoints. Tested on a tiny host:
an interrupted and resumed run equals the uninterrupted run bit for bit on CPU (`model.policy_head.test_joint`,
`model.encoder.test_task`, `model.train.test_masked`).

## Run order and stop rules

Each step ends with its scorecard and `model.eval.compare`; the next step starts only after reading it.

0. **Baselines (A0)**, 5 min each: the S0 untrained cards (the play baselines already exist).
1. **T0, T2-lite, T2-full** (A1 to A3), 1.2 h total. Do S0 questions improve when the position is a graph?
2. **S1 warm-up** (A5 step 2), minutes. The cheapest signal on the human-move head.
3. **P0** (A6), under an hour. The gate before anything board-encoder.
4. **T4, then T5** (A8; A7 optional), 1 to 1.5 h. The arm the work exists for: Run 1 had 295 of 300 reasonings with a false claim.
5. **v2a, then v2b** (A9, A10), about 1 h. Short grounded reasoning, then tool quoting with the loss mask.
6. **T1, T3, T3-full** (A4), about 1 h, on the extended base. Second question: board-native tokens against text.
7. **S1 joint** (A5 step 3), 25-37 min matched, or 4-6 h on all 200k rows.
8. **Perceiver** (A11) at any time; it shares nothing with the arms above.

Stop rules (any one stops the sequence until explained; none is a result about an arm):

- Disk or card: `/` under 80 GB free, either disk above 90% with a run over 10 GB, or less GPU free than the table above: do not launch.
- First 200 steps of any run (`train.log`, `seconds` per step): loss not finite or not below its step-1 value, or under 7,000 tokens/s
  (half the planning figure): stop, re-plan the time, fix before continuing.
- Format not learned: a trained arm whose raw parse rate on the holdout is under 90% (the scorecard gate) is a pipeline fault, not a
  verdict on the arm; Run 1 short reached 99%.
- Step 1: if trained T0 does not beat its untrained baseline on S0 exact match, stop; the T2 results mean nothing. With 1,000 holdout
  rows a difference under about 3 points is a tie (binomial 95% at 50%); read `groups.node` and `groups.edge` apart.
- Step 2: if the head-only masked top-1 is not above the random-legal floor card (`keep/eval-random-legal-floor-20261113T034305Z`), the
  frozen host's last hidden state does not carry the move: skip S1 joint.
- Step 3: `axis.json` must report every head with piece-on-square board-exact and legal-move set-exact near 1 on held-out positions
  (proposed reading: 0.99 or more). Below that no projector is aligned and no T4 is trained.
- Step 4: if T4's `play_explanation` claim failure rate is not clearly under Run 1 short's, the soft tokens are not grounding: do not run
  T5 on that encoder.
- Step 5: run v2b only if v2a lowered the claim failure rate against Run 1 short; v2b also needs its corpus rebuilt with the `play-reads`
  prompt (A10).
- Step 6: T1/T3 compare against the trained T0 and T2 arms, never against an untrained base (their new rows mean nothing before training).

## Baselines and how arms are scored

Play arms (S1, T4, T5, v2a, v2b) score on the move holdout with the Run 1 control's settings and compare against
`keep/eval-run1short-playmode-300-20261120T221837Z` (raw legality 0.99, human match 0.0433, CPL 409.7 against a random floor of 417,
295 of 300 reasonings with a false claim). Other cards: `keep/eval-base-untrained-playmode-20261120T121911Z` (untrained base) and
`keep/eval-random-legal-floor-20261113T034305Z` (random legal moves). S0 arms score on their own corpus's 1,000 holdout rows against
the untrained base on the same prompt (A0) and against each other. Cards of different prompt kinds need `--allow-prompt-mismatch`
(rows, Stockfish depth and the tactical set are still checked). Never sum axes.

```bash
export KNIGHTLM_BASE_DIR=$BASE KNIGHTLM_DEVICE=cuda
RUN1=$K/eval-run1short-playmode-300-20261120T221837Z
EVAL="--rows 300 --oracles lc0 maia"       # as the Run 1 play-mode evaluation; add --games N for matches
```

Eval dirs land in `$K/eval-<label>-<stamp>` (`--out-root` puts exploratory runs elsewhere); `<stamp>` below is that suffix.

## Per-arm blocks

### A0 Baselines (untrained base on each S0 arm's prompt)

```bash
for arm in t0 t2 t2-full; do
  KNIGHTLM_ADAPTER_DIR= python -m model.eval.run --predictor model.eval.playmode:from_env \
    --corpus $C/$S0-$arm --label s0b-$arm-base --rows 1000 --no-stockfish --no-tactics
done
```

GPU inference, about 5 min each (U). Preflight: the predictor path is tested with fakes.

### A1 to A3 T0, T2-lite, T2-full (S0 rules QA, the recipe)

One question per position about the board (side to move, piece on a square, attackers, defenders, legal moves of a piece, check,
captures, material); every answer comes from the compiler. The six S0 corpora share row ids, questions and answers (`matched_sha256`
`ce5b52dd1efa21bc...`), so arms differ only in how the position is written. Format v2: the completion's `move` is `min(legal_moves)`,
never the human move; `attackers_of`/`defenders_of` are reported apart (`groups.edge` against `groups.node`).

| arm | prompt kind | position in the prompt | corpus |
|---|---|---|---|
| T0 | `play` | FEN packet JSON | `$S0-t0` |
| T2-lite | `play-cgt` | node list and legal moves per piece | `$S0-t2` |
| T2-full | `play-cgt-full` | lite plus `atk`, `def`, `pin`, `hang`, `check`, `ksafe` | `$S0-t2-full` |

```bash
for arm in t0 t2 t2-full; do   # each prints its CPU preflight first; read it, then it launches with --gpu
  python -m model.train.run --corpus $C/$S0-$arm --run s0b-$arm --baseline $K/eval-s0b-$arm-base-<stamp> \
    --eval-arg=--no-stockfish --eval-arg=--no-tactics --gpu
done
python -m model.eval.compare $K/eval-s0b-t0-base-<stamp> $K/eval-s0b-t0-<stamp>      # did training help
python -m model.eval.compare $K/eval-s0b-t0-<stamp> $K/eval-s0b-t2-<stamp>           # T0 vs T2-lite
python -m model.eval.compare $K/eval-s0b-t0-<stamp> $K/eval-s0b-t2-full-<stamp>      # T0 vs T2-full
```

Tokens (M, 64-row sample; 625 steps x 32 rows, no row dropped): T0 20.86M (p95 1194, max 1291), T2-lite 16.16M (842, 857), T2-full 21.52M
(1154, 1170). About 700 tokens of every prompt are the fixed play schema. End to end 20 to 35 min per arm plus about 5 min of evaluation
(U, from Run 1 short). The adapter lands in `$R/<run>/adapter`; the recipe evaluates the 1,000 holdout rows and compares to `--baseline`.
Preflight evidence: `$K/s0-pilot-2026-11b/` (audits, token costs, `preflight-<arm>.json`; CPU optimiser steps were recorded for t1 only, the
recipe is Run 1 short's). Read `groups.node` across all arms and `groups.edge` only between arms that print edges (T2-full, T3-full); a
lower `claim_failure_rate` is better.

### A4 T1, T3, T3-full (ctok/1 board tokens; the extended base)

```bash
# once, CPU, about 1 GB (already built at $(dirname $BASE)/smolvlm-500m-ctok1)
python -m model.train.ctok build-base --base-dir $BASE --out $(dirname $BASE)/smolvlm-500m-ctok1
python -m model.train.run --corpus $C/$S0-t1 --run s0b-t1 --tokenizer ctok1                        # preflight
python -m model.train.run --corpus $C/$S0-t3 --run s0b-t3 --tokenizer ctok1 --gpu
python -m model.train.run --corpus $C/$S0-t3-full --run s0b-t3-full --tokenizer ctok1 --gpu
```

With `--tokenizer ctok1` the preflight also checks `extended_base` (the record names this checkout's vocabulary hash and the pinned stock
weights; untied head), the ctok ids against the id-level encoding, and takes two CPU steps on the extended base with the new rows trainable.
The recipe trains `trainable_token_indices` for `embed_tokens` and `lm_head` rows 49280..51775 (blut-cookbooks PR #5): 5,611,520 trainable
parameters (LoRA 819,200 + 2 x 2,396,160) against 99,409,920 extra for the rejected `modules_to_save`. Tokens (M): T1 13.78M, T3 14.63M,
T3-full 18.51M. The recipe skips evaluation for this base (the serving predictor pins the stock digest and has not run on an extended base):
score by hand as in A1 to A3 with `KNIGHTLM_BASE_DIR=$(dirname $BASE)/smolvlm-500m-ctok1` and the adapter. Not tested: the serve-side
predictor on an extended base.

### A5 S1 human-only move head (frozen warm-up, then joint adapter + head)

Code: `model/policy_head/train.py` (entry) and `joint.py` (loop). It reuses the BLUT image-text data code, the recipe's LoRA regex and
scheduler; it is not a recipe stage because `hf_sft_train` builds one LoRA model and rejects `spec.extra`. Per row two views of `play-head`:
the head-read prompt (no move) feeds the head at the last token (`hidden_states[-1]`, the serve contract), the conditioned prompt plus the
unchanged play call feeds the language loss; both are tokenised by `Predictor._play_prompt_ids`.

```bash
CORPUS=$C/run1-reasoning-2026-09-a
# 1. CPU preflight (changes nothing): audit, mask legality, parity with serve, two joint steps on the real base, reload through the serve loader
$PY -m model.policy_head.train --corpus $CORPUS --rows 20000 --run s1-head-a
# 2. the cheap probe: frozen host, head only
$PY -m model.policy_head.train --corpus $CORPUS --rows 20000 --max-steps 625 --run s1-warmup-a --mode head-only --gpu
# 3. the arm: adapter + head, matched to T0 (625 steps, 20,000 rows); drop --max-steps for the 200k-row pass
$PY -m model.policy_head.train --corpus $CORPUS --rows 20000 --max-steps 625 --run s1-head-a --gpu
# a crashed run continues from its last checkpoint
$PY -m model.policy_head.train --corpus $CORPUS --rows 20000 --max-steps 625 --run s1-head-a --resume --gpu
```

`--gpu` refuses on a skipped check, a missing `--run`, an existing run directory (without `--resume`), under 5 GB disk or under 12 GB GPU free.
Output `$R/s1-head-a/{adapter,head}` (head: `head.pt`, `head.json`, `manifest.json` binding base, adapter, head, ctok/1, ruleset, prompt kind,
corpus hash), `train_summary.json`, snapshots and `resume/` every 500 steps. Estimate (U): 75-110 ms/row joint (head-read prompts average 990
tokens, max 1,238 in 64 rows): 20k rows 25-37 min, 200k rows 4-6 h; head-only 8-15 ms/row: 3-5 min for 20k rows, 27-50 min for 200k.
Preflight on the real base (M, 2026-11-20): all six checks ok, two steps 235 s under CPU contention (92 s earlier), 819,200 adapter and
1,893,168 head parameters, reload logit error 0.0 (`$K/gaps-preflight-2026-11/preflight-s1-head.json`).

```bash
# joint: the head's move beside what the completion wrote
KNIGHTLM_ADAPTER_DIR=$R/s1-head-a/adapter KNIGHTLM_HEAD_DIR=$R/s1-head-a/head python -m model.eval.run \
  --predictor model.eval.playmode:head_from_env --prompt-kind play-head --move-head --label s1-head $EVAL
# warm-up (frozen host: empty adapter, head only; the head axis is the measurement, completions are noise)
KNIGHTLM_ADAPTER_DIR= KNIGHTLM_HEAD_DIR=$R/s1-warmup-a/head python -m model.eval.run \
  --predictor model.eval.playmode:head_from_env --prompt-kind play-head --move-head --label s1-warmup --rows 300 --modes raw --no-tactics
python -m model.eval.compare --allow-prompt-mismatch $RUN1 $K/eval-s1-head-<stamp>
```

Read `axes.move_head` (masked top-1/top-3, NLL, ECE, CPL; raw legality and illegal mass as diagnostics) beside `axes.head_agreement` and
`axes.play_explanation`. Human match is not strength: read it with CPL against Run 1 short (409.7; random floor 417). Serve:
`python -m model.serve.server --base-dir $BASE --adapter-dir $R/s1-head-a/adapter --head-dir $R/s1-head-a/head`.

### A6 Board-encoder pretrain (P0)

```bash
# CPU sanity of the same command (minutes, tiny encoder)
python3 -m model.encoder.pretrain --corpus $C/run1-reasoning-2026-09-a --out /mnt/4tb/tmp/encoder-smoke --tiny --train-rows 2000 \
  --epochs 3 --device cpu --knightlm $KNIGHTLM_BIN
$PY -m model.encoder.pretrain --corpus $C/run1-reasoning-2026-09-a --out $R/encoder-p0-a --knightlm $KNIGHTLM_BIN \
  --train-rows 200000 --walk-share 1.0 --holdout-rows 10000 --epochs 20 --batch 256 --lr 1e-3 --device cuda
cat $R/encoder-p0-a/axis.json      # the gate (stop rule, step 3)
```

400,000 positions x 20 epochs, about 20-40 min (U; about 20 of them single-process plane building), under 4 GB GPU, a few hundred MB of
disk. Writes `<out>/encoder/` (`encoder.pt`, `config.json`, `heads.pt`, `manifest.json`: plane fingerprint, corpus and compiler hashes) and
`axis.json`. Scored by `axis.json` (per head, no composite), not by `model.eval`. Spec: `docs/model/board-encoder.md`.

### A7 Projector alignment (P1, optional before T4/T5)

`model.encoder.task` trains the projector itself, so P1 is a warm start, not a prerequisite.

```bash
python3 -m model.encoder.rows --corpus $C/$S0-t0 --kind play-enc --limit 48000 --out $R/p1-rows-t4.jsonl   # S0 -> {fen, repetition, prompt, completion}
$PY -m model.encoder.align --host $BASE --encoder $R/encoder-p0-a/encoder --rows $R/p1-rows-t4.jsonl \
  --out $R/projector-smolvlm-a --steps 3000 --batch 16 --lr 1e-3 --device cuda
```

The converter writes what the S0 builder did not: `fen` and `repetition` from `planes.packet_inputs` (the server's own function), the arm's
`play-enc` prompt plus the question, and the completion; tested in `model.encoder.test_task`. The S0 corpora are matched, so T4 and T5 rows
differ in the prompt kind only. 48,000 rows x about 140 tokens, 10-30 min (U), under 10 GB GPU. `align.py` does not apply the chat template;
the task trainer does.

### A8 T4 and T5 (task training: adapter + projector, encoder frozen)

```bash
# CPU preflight: audit, conversion, base weights, prompt parity with the server (64 slots, no FEN, planes), 2 real-host steps, reload through the serve loader
$PY -m model.encoder.task --corpus $C/$S0-t0 --kind play-enc --encoder $R/encoder-p0-a/encoder --rows 20000 --run t4-a
$PY -m model.encoder.task --corpus $C/$S0-t0 --kind play-enc --encoder $R/encoder-p0-a/encoder [--projector $R/projector-smolvlm-a] \
  --rows 20000 --run t4-a --gpu
$PY -m model.encoder.task --corpus $C/$S0-t0 --kind play-enc-cgt --encoder $R/encoder-p0-a/encoder --knightlm $KNIGHTLM_BIN \
  --rows 20000 --run t5-a --gpu                                  # a crashed run: the same command plus --resume
```

Rows: the first 20,000 S0 train rows converted by `model.encoder.rows` (the same questions as the S0 arms; a reasoning corpus converts the
same way). Output `$R/t4-a/{adapter,projector,task.json,rows,resume}`; `task.json` binds base, adapter, encoder, projector and prompt kind, and
`Predictor._load_board` refuses an adapter served with another projector, encoder, base or kind. Tokens (M, first 64 S0 rows): T4 940.6 per
row (p95 1001, max 1017) = 18.8M, T5 1177.6 (1269, 1293) = 23.6M; time 0.37 h and 0.47 h at 14k tokens/s (U; 36-145 and 45-181 ms/row over
the 26k-6.5k range). Trainable: LoRA 819,200 + projector 1,169,792. Preflight (M, random-init encoder because P0 has not run, 2026-11-20):
all checks ok, 2 steps 31 s, reload logit error 0.0 (`$K/gaps-preflight-2026-11/preflight-t4-random-encoder.json`). `--gpu` additionally
refuses without a pretrained `--encoder`.

```bash
env KNIGHTLM_ENCODER_DIR=$R/encoder-p0-a/encoder KNIGHTLM_PROJECTOR_DIR=$R/t4-a/projector KNIGHTLM_ADAPTER_DIR=$R/t4-a/adapter \
  python -m model.eval.run --predictor model.eval.playmode:from_env --prompt-kind play-enc --label t4-encoder $EVAL
env KNIGHTLM_ENCODER_DIR=$R/encoder-p0-a/encoder KNIGHTLM_PROJECTOR_DIR=$R/t5-a/projector KNIGHTLM_ADAPTER_DIR=$R/t5-a/adapter \
  python -m model.eval.run --predictor model.eval.playmode:from_env --prompt-kind play-enc-cgt --label t5-encoder-cgt $EVAL
python -m model.eval.compare --allow-prompt-mismatch $RUN1 $K/eval-t4-encoder-<stamp>
python -m model.eval.compare --allow-prompt-mismatch $K/eval-t4-encoder-<stamp> $K/eval-t5-encoder-cgt-<stamp>   # does cgt text add to soft tokens
```

Read the play axes as for T0 plus `play_explanation`: the encoder's reason to exist is that a board-grounded model stops asserting false
piece placements (Run 1: 65% of piece-on-square claims). The S0 questions cannot score these arms: `Predictor.complete` does not inject soft
tokens. Serve: `python -m model.serve.server --base-dir $BASE --adapter-dir $R/t4-a/adapter --encoder-dir $R/encoder-p0-a/encoder
--projector-dir $R/t4-a/projector --play-prompt-kind play-enc`. Not tested: encoder accuracy on held-out positions, GPU memory and latency
of soft-token decoding, `stop_strings` with embedding-input decoding on the real tokenizer.

### A9 Reasoning v2a (text, one submit_move call)

```bash
python -m model.train.run --corpus $C/reasoning-v2-pilot-2026-11-a --run r2a --baseline $RUN1 \
  --eval-arg=--no-stockfish --eval-arg=--no-tactics --gpu
KNIGHTLM_ADAPTER_DIR=$R/r2a/adapter python -m model.eval.run --predictor model.eval.playmode:from_env --prompt-kind play $EVAL --label r2a
python -m model.eval.compare $RUN1 $K/eval-r2a-<stamp>
```

20,000 train and 1,000 holdout rows, zero failed claims. Tokens (M, first 5,000 rows) 1071.7 (p95 1249, max 1686) against Run 1's 1089.0;
0.42 h plus about 5 min of evaluation (U). Preflight (corpus audit, prompt parity, tokenisation) passes without `--gpu`. Read the claim failure
rate and claims per explanation. Spec: `docs/model/reasoning-v2.md`.

### A10 Reasoning v2b (read-tool quoting, loss on the calls only)

The b pilot (`reasoning-v2-pilot-2026-11-b`) was built with the `play` prompt, which says "exactly one tool call". The builder now writes
`play-reads` rows; **rebuild the corpus before training** (the owed step; the old pilot still audits but trains the wrong prompt):

```bash
python -m model.corpus.reasoning2 build --variant b --parent $C/run1-reasoning-2026-09-a --train 20000 --holdout 1000 \
  --knightlm $KNIGHTLM_BIN --output $C/reasoning-v2-2026-11-b-reads
$PY -m model.train.masked --corpus $C/reasoning-v2-2026-11-b-reads --run r2b                  # CPU preflight
$PY -m model.train.masked --corpus $C/reasoning-v2-2026-11-b-reads --run r2b --gpu            # add --resume to continue a crashed run
KNIGHTLM_ADAPTER_DIR=$R/r2b/adapter python -m model.eval.run --predictor model.eval.playmode:from_env --prompt-kind play-reads $EVAL --label r2b
python -m model.eval.compare --allow-prompt-mismatch $RUN1 $K/eval-r2b-<stamp>
```

The recipe (`model.train.run`) refuses this corpus: the BLUT trainer (`hf_sft_train`) reads `{prompt, completion}` only and would put loss on
the tool results (`--allow-unmasked-results` overrides, as a different, labelled arm). `model.train.masked` is the same LoRA recipe with the
loss mask from `segments`. Tokens (M, first 5,000 rows) 1331.3 (p95 1530, max 2003); 39% of completion tokens are results and carry no loss
(M, 64 rows: 8,511 of 21,954). Time 0.53 h at 14k tokens/s (U). Preflight (M, on the old pilot, 2026-11-20): all checks ok, 2 steps 32 s,
reload logit error 0.0 (`$K/gaps-preflight-2026-11/preflight-r2b-masked.json`). Scoring with `--prompt-kind play-reads` needs a predictor
that runs read calls between turns; the Python server refuses the kind, so a v2b scorecard needs the Rust harness path. Not tested: that
path with a model, and any v2 transcript produced by a model.

### A11 Perceiver (image to placement)

```bash
python3 -m model.perceiver.train --out $R/../perceiver/run-a --positions 4000 --boards-per-style 2000 --epochs 12 --device cuda
python3 -m model.perceiver.evaluate --checkpoint $R/../perceiver/run-a/perceiver.pt --boards-per-style 300 \
  --out $R/../perceiver/run-a/scorecard.json
```

Rendering 12k boards takes about 4 min on one core (M); 768k crops x 12 epochs = 9.2M samples, 2.4 GB uint8 in RAM; CPU about 900 samples/s
so about 2.8 h (M rate under a build); GPU roughly 10-20 min (U, input-bound). Read held-out whole-board exact match per style, not the
train group. To improve transfer first add styles and augmentation. Spec: `docs/model/perceiver.md`.

## Not tested

No GPU run of any arm. CPU preflights ran on the real base for S1, T4/T5 (random-init encoder) and v2b; the recipe's S0 evaluation path and
`--baseline` compare are exercised by unit tests with fakes. Not tested anywhere: encoder accuracy on held-out positions, a trained adapter on
the T1/T3 extended base, soft-token decoding on a GPU, the Rust harness path for v2b transcripts, resume on a GPU (CPU is bit-identical;
CUDA kernels that are not deterministic can differ in the last bits), and the serving predictors on any trained artifact.
