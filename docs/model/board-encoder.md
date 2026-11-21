# Board encoder (arms T4 and T5)

Status: specification and CPU-tested reference code (`model/encoder/`). Pretraining and
projector alignment are **prepared, not run**; nothing here has trained on a GPU.

## Why

Run 1 showed a 500M text model cannot ground board claims from FEN (65% of its
piece-on-square claims were false). Text arms (A: packet, B: spatial text, C: graph text,
see `graph-text.md`) fix the input by rewriting it. The encoder fixes it by not asking the
LLM to read the board at all: a small network that has learned the rules turns the
position into about 64 soft tokens that sit in the LLM's input like image tokens.

It is a research arm. T4 gives the LLM the soft tokens and the packet's rule facts as text
(legal move list, as in T0). T5 adds CGT text (`graph-text.md`) beside the soft tokens.
Both are scored on the one scorecard against the Run 1 text control at a matched budget
(same rows, steps, base and labels).

## Invariants kept

- **One state.** Input is one position packet. History appears only as the packet's
  repetition count. Unknown stays unknown: a four-field FEN is refused, an unknown
  repetition count is a flag plane, not a zero.
- **Engine-zero.** Pretraining labels are rules facts from the compiler (shakmaty). No
  engine, no evaluation, no best move. The encoder carries no strategic signal.
- **No chess logic outside the compiler.** `model/encoder/planes.py` copies FEN fields and
  reuses `model.eval.explain.Board` for placement. Labels come only from
  `knightlm board-labels` (`crates/cli/src/board_labels.rs`). Python never computes an
  attack, a legal move, a check or a material count.
- **Separable.** The encoder is frozen after pretraining and knows nothing about any LLM.
  Each host gets one projector. Swapping the host retrains the projector only.
- **No composite.** Evaluation reports each rules head on its own.

## Input planes (`planes.py`)

`(22, 8, 8)`, square index `rank * 8 + file` (a1 = 0, h8 = 63), the same as the labels.

| planes | content |
|---|---|
| 0-5, 6-11 | white then black P N B R Q K |
| 12 | side to move is white (constant plane) |
| 13-16 | castling K Q k q (constant planes; only standard letters, else refused) |
| 17 | en passant target square (one-hot; the FEN field, copied) |
| 18 | halfmove clock / 100, clamped to 1 |
| 19 | fullmove number / 200, clamped to 1 |
| 20 | repetition count known (0 when the history is incomplete) |
| 21 | repetition count / 3, clamped to 1 (0 when unknown) |

`packet_planes(packet)` takes the corpus packet: `fen` plus `facts.repetition_count` when
`history_complete`. `build_planes(fen, repetition)` takes a bare FEN.

## Encoder (`encoder.py`)

Per-square linear embedding of the 22 plane values, plus a learned square position
embedding, then a pre-norm transformer encoder (default `d_model` 256, 6 layers, 8 heads,
GELU, 4x feed-forward, about 4.8M parameters) and a final LayerNorm. Output: one token
per square (a1 first), plus `n_global` learned global tokens appended (default 0). With
`n_global = 16` the board is 80 tokens. A CNN is a fallback if attention over 64 squares
is too slow to converge; the interface (planes in, tokens out) does not change.

Why 64: SmolVLM-500M uses `image_seq_len = 64` per image, so the board takes exactly the
slot an image takes. Any count works (see injection below); 64 is the cheap default.

## Rules-QA heads (pretraining only)

Heads read the encoder tokens and are discarded after pretraining.

| head | target (compiler label) | loss |
|---|---|---|
| `piece_on_square` | 13-way class per square (empty + 12 pieces) | cross-entropy |
| `attacked_squares` | per square: attacked by white, by black (geometric, as cgt `atk`/`def`) | BCE |
| `legal_moves` | 64x64 from/to mask of the side to move's legal moves (bilinear over square tokens; promotions collapse onto one pair) | BCE, positive weight 8 |
| `check` | side to move is in check | BCE |
| `material` | pawn, knight, bishop, rook, queen counts for both sides | smooth L1 |

Training minimises the sum of the head losses (an optimisation objective only). Evaluation
never sums or averages heads.

### Labels and positions

Positions: the Run 1 corpus `/mnt/4tb/artifacts/KnightLM/expire/corpus/run1-reasoning-2026-09-a`
(200,000 train rows, 10,000 strictly later holdout rows; FEN and repetition from each packet;
packet text and moves are not used). Held-out positions are the corpus holdout, which the
corpus audit already keeps disjoint by game, opening group and scored position.

The corpus has one decision per game at ply 24. To keep the encoder from seeing only that
distribution, train positions are doubled with `--walk-share 1.0`: for half of the rows the
compiler plays 1 to 40 pseudo-random legal moves from a corpus position
(`board-labels` rows `{"fen", "walk", "seed"}`; the generator is the compiler's, the
choice is a seeded xorshift). The holdout stays real positions only.

Labels: `knightlm board-labels` reads JSONL `{fen}` and prints, per position, `squares`,
`attacked_white`, `attacked_black`, `in_check`, `legal_moves`, `counts`, `points`. A
refused position is an error, never skipped. A 200-position fixture
(`model/encoder/fixtures/labels_smoke.jsonl`) is checked against the live compiler when the
binary is built, and against the plane builder always.

## Eval axis (`pretrain.evaluate_heads`)

Held-out, one record per head, no composite:

```
{"axis": "board-encoder-rules-qa", "n_positions": N, "heads": {
  "piece_on_square":  {"square_accuracy", "board_exact"},
  "attacked_squares": {"square_accuracy", "map_exact"},
  "legal_moves":      {"set_exact", "precision", "recall"},
  "check":            {"accuracy"},
  "material":         {"counts_exact"}}}
```

`axis.json` is written beside the encoder. Gate before projector alignment: every head
reported; piece-on-square board-exact and legal-move set-exact near 1 on held-out positions.
If a head is far from it, alignment does not start. The axis is not part of `model.eval`'s
scorecard; the downstream claim-failure rates there remain the measure of the arm.

## Injection into SmolVLM (`inject.py`)

Reused, not invented. In the installed transformers 5.5.3, `SmolVLMForConditionalGeneration.forward`
(like Idefics3) accepts `image_hidden_states` of shape `(n_blocks, tokens_per_block, text_hidden)`
instead of `pixel_values`. `SmolVLMModel.inputs_merger` writes the n-th run of
`tokens_per_block` `<image>` ids in `input_ids` with the n-th block. The projected board
tokens are passed as `image_hidden_states`; the vision tower and connector are bypassed
and the host is not patched. Requirements:

- the number of `<image>` ids per board equals the soft token count (64 for SmolVLM-500M,
  or 80 with 16 global tokens); `check_slots` refuses a mismatch;
- `image_token_id` is the host's own (SmolVLM-500M: 49190, text hidden size 960);
- the soft tokens are cast to the host dtype; gradients flow to the projector.

`host_forward(host, input_ids, soft, ...)` is the call; `injected_embeds` returns the
embeddings the language model will see. A different host family (Qwen) needs its own
injection test, since not every model has this slot; the encoder and its pretraining are
unchanged.

## Serving and scoring the arms (T4, T5)

Prompt kinds (`model/prompt.py`, goldens in `model/tests/test_prompt.py`):

- `play-enc` (T4): the play prefix, `Board tokens:` and 64 host `<image>` placeholders, then `Rules:` and the
  packet without its FEN and without its repetition ledger (the ledger keys spell the placement): the legal-move
  list and facts as text. The completion is the unchanged play call.
- `play-enc-cgt` (T5): the same plus the cgt/1 lite text (`Position graph:`) beside the soft tokens.

`Predictor(..., encoder_dir=, projector_dir=)` (or `KNIGHTLM_ENCODER_DIR` and `KNIGHTLM_PROJECTOR_DIR` with
`model.eval.playmode:from_env`, or `--encoder-dir/--projector-dir` on the server) loads the frozen encoder
and the host's projector, checks the projector against the host's hidden size, and for each request computes
the soft tokens from the packet's planes. Generation decodes from the embeddings `injected_embeds` returns, so
the soft tokens enter through the documented `image_hidden_states` slot and nothing in the host is patched; a
peft-wrapped host is unwrapped for the merge. The manifest gains `encoder_sha256`, `projector_sha256` and
`board_tokens`; a mismatch of kind and encoder (an enc kind without a board, a board with a text kind) is refused.
`model.eval.run --prompt-kind play-enc` (or `play-enc-cgt`) scores them on the same axes as every play arm.

## Projector and freeze policy

`Projector`: LayerNorm, Linear(d_enc, h), GELU, Linear(h, host_hidden); about 1.2M
parameters for SmolVLM-500M. One per host; saved as `projector.pt` plus `projector.json`.

| stage | encoder | projector | host LLM | purpose |
|---|---|---|---|---|
| P0 encoder pretraining | trains | n/a | absent | learn rules from labels |
| P1 alignment (stage A) | frozen | trains | frozen | map tokens into the host's embedding space |
| P2 task training (stage B) | frozen | trains | LoRA (BLUT recipe), same as T0 | matched-budget arm run |

The encoder is never unfrozen in this arm. Unfreezing it would couple it to one host and
break separability; it is a separate, labelled ablation if P2 plateaus. The vision tower of
the host is unused by T4/T5 and stays frozen.

## Host-LLM agnosticism

Nothing in `encoder.py` imports transformers. A host contributes only its text hidden size
(projector output width), its `<image>` id and the injection call. Moving to Qwen means a
new projector (P1 again) and a host-specific injection test.

## Tests (CPU, tiny config)

`python3 -m unittest model.encoder.test_encoder` (the model tests skip without torch):
planes (placement, state, refusals), fixture labels agree with planes, fixture matches the
live compiler, walk rows come from the compiler, token counts 64 and 80, loss decreases on
every head and heads overfit 160 positions, the eval axis has no composite, soft tokens
appear verbatim at the host's `inputs_embeds` image positions (text positions untouched,
another board changes logits), a slot-count mismatch is refused, reload of encoder and
projector gives bit-identical host logits, and alignment gradients reach only the projector.
The Rust side has unit tests in `board_labels.rs`.

## Prepared, not run

Resources: one RTX 4090 (24 GB), `/` and `/mnt/4tb` space checked before a run. All times
are estimates from parameter counts and measured CPU data preparation (about 7k positions per
second per process for planes plus targets); none was measured on GPU.

### 1. Build the label tool

```sh
cd /mnt/4tb/KnightLM && oneoff cargo build --release -p knightlm-cli   # target/release/knightlm
```

### 2. Pretrain the encoder (P0)

```sh
python3 -m model.encoder.pretrain \
  --corpus /mnt/4tb/artifacts/KnightLM/expire/corpus/run1-reasoning-2026-09-a \
  --out /mnt/4tb/artifacts/KnightLM/expire/runs/encoder-p0-a \
  --knightlm target/release/knightlm \
  --train-rows 200000 --walk-share 1.0 --holdout-rows 10000 \
  --epochs 20 --batch 256 --lr 1e-3 --device cuda
```

Volume: 400,000 train positions x 20 epochs = 8M position passes, 64 tokens each, so about
0.5B encoder tokens. Compute: 6 x 4.8M x 0.5B = 1.5e16 FLOPs. Time: about 20 to 40 minutes
on the 4090 including the single-process plane building (about 7k positions per second,
roughly 20 minutes of the total unless planes are cached or loaded by workers), plus about
2 minutes for labels. Output: `encoder/`, `axis.json`. Memory: under 4 GB. A CPU-only
sanity run of the same command with `--tiny --train-rows 2000 --epochs 3 --device cpu`
takes minutes.

### 3. Align the projector (P1)

Needs the S0 rules-QA rows (`{fen, repetition?, prompt, completion}` JSONL) from the CGT/S0
corpus builder; that corpus is not part of this change.

```sh
python3 -m model.encoder.align \
  --host /mnt/4tb/artifacts/KnightLM/expire/models/smolvlm-500m-a7da5b98 \
  --encoder /mnt/4tb/artifacts/KnightLM/expire/runs/encoder-p0-a/encoder \
  --rows <s0-rules-qa-rows.jsonl> \
  --out /mnt/4tb/artifacts/KnightLM/expire/runs/projector-smolvlm-a \
  --steps 3000 --batch 16 --lr 1e-3 --device cuda
```

Volume: 3,000 steps x 16 rows = 48,000 rows of about 140 tokens (64 soft, about 60 prompt,
about 15 completion) = 6.7M tokens. Compute: about 4 x 0.36B x 6.7M = 1e16 FLOPs (frozen
host, no weight gradients). Time: about 10 to 30 minutes. Memory: host bf16 about 1 GB plus
activations, under 10 GB.

### 4. Task training with the projector (P2) through BLUT

The draft stages in `apps/blut-knightlm/src/stages/board_encoder.rs`
(`pretrain_board_encoder`, `align_projector`) wrap steps 2 and 3. They compile but are not
registered in any recipe and are never run here. P2 would extend `knightlm_lora` with the
projector (trainable) and the same rows, steps and LoRA as T0; that wiring is not written
because it needs the T0/T2 matched settings from the S0 corpus work.

### 5. Evaluate and compare

```sh
cat /mnt/4tb/artifacts/KnightLM/expire/runs/encoder-p0-a/axis.json
python3 -m model.eval.run --prompt-kind play ...      # the T4/T5 adapter, as for T0
python3 -m model.eval.compare <T0 scorecard dir> <T4 scorecard dir>
```

Serving and scoring are in the next section; commands per arm are in `training-gate.md`.

## Known limits

- Not trained: every number above is an estimate; accuracy of the heads on held-out
  positions is unknown. The CPU tests prove the plumbing and that the heads can overfit,
  not that the encoder generalises.
- Random-walk positions are legal but unnatural; they broaden coverage of material and
  structure, not human-like play. Chess960 and variants are out of scope (castling letters refused).
- Repetition on walked positions is unknown (flag off), so the repetition planes are
  trained only on real positions.
- The legal-move head collapses promotion pieces; the LLM still receives the full legal
  list as text in T4/T5.
- Soft tokens are not interpretable per se; claim checking stays in the rules oracle
  (`model/eval/explain.py`), unchanged.
