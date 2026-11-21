# Move-policy head (arm: human-only move head)

Status: specification and Python reference (`model/policy_head/`, `model/eval/move_head.py`), plus the
prompt kind `play-head` (`model/prompt.py`), the serve predictor (`model/serve/head.py`) and the eval wiring
(`--move-head`, `model.eval.playmode:head_from_env`). Nothing trains and nothing has run on a GPU: the serve and
eval paths are tested on a tiny random CPU host only. This is goal item 5 of the approved plan ("move-policy
head over legal moves, a labelled arm").

## What it is

A classifier over the 1968 moves of the `ctok/1` move space (`docs/model/tokenizer.md`; the list is
`model.tokenizer.moves.UCI`, reused, never re-derived; promotions are separate classes such as
`e7e8q`) that reads the host LLM's last hidden state. The hidden state is taken at the last prompt
token (right-padded batches pick the last non-pad position, `head.last_hidden`), after the position
view (T0 FEN packet, T1/T2/T3 or encoder tokens; the head is indifferent to which).

    packet -> prompt view -> LLM body -> last hidden h -> LayerNorm -> Linear(H, 1968) -> raw logits z
    legal mask m (from packet.legal_moves)  ->  p = softmax(z where m else -inf)   <- the product

**Label:** every number from this arm says "human-only move head". It is trained on human moves
only (CC0 database); engine labels would be a different arm.

## Masking

- The mask is built from the compiler's packet `legal_moves` list (`space.legal_mask_row`), the same
  list the play prompt shows. A legal move outside the move space, an empty list (terminal position)
  or a target that is not legal is refused loudly (`MoveSpaceError`), never dropped silently.
- **Train:** cross-entropy over legal moves only (illegal logits are -inf before the softmax), so
  the head never spends capacity ranking illegal moves.
- **Inference:** the same mask. The legal-masked distribution is the product; the move is its argmax
  (or a sample, if a temperature is ever added; not in this spec).
- **Diagnostic:** raw logits stay available. The unmasked argmax is scored on every eval as
  a diagnostic: its legality rate, its match with the human move and the softmax mass it puts on
  illegal moves. Masking can hide a head that has not learned the rules, so that number is always
  reported beside the masked ones (invariant 3: constrained decoding never hides a raw failure).

## Inference order and agreement

1. The compiler produces the packet and the legal list.
2. The head picks the move m* (masked argmax). No engine is reachable (engine-zero invariant).
3. The language head generates the `submit_move` call with reasoning conditioned on m*. The play
   call has reasoning first, then the move, so conditioning means a prompt line naming m* and the
   move field being pinned to m* on the `constrained_move` path. The prompt kind is `play-head`
   (`model/prompt.py`, byte goldens in `model/tests/test_prompt.py`):

       head-read prompt   = play prompt + "\nMove head:"            (the head reads the last hidden state here)
       conditioned prompt = head-read prompt + " " + m*             (the language stage continues from here)
       completion         = the unchanged play call (reasoning, then move == m*)

   The head reads a prompt that does not contain its own answer; the same text, with the move after the
   label, is what the adapter is trained and served on. The adapter and the head are trained jointly on
   these two views of one row. The hidden-state contract: `hidden_states[-1]` of the language model (after
   its final norm) at the last token of the chat-templated head-read prompt (`Predictor.last_hidden`).
4. **Agreement check** (`policy_head.agree.interpret_with_head`): the completion goes through the
   existing `serve.play.interpret`; then a move field or a stated move in the reasoning that differs
   from m* is `stated_move_mismatch`. There is no repair and no fallback move. The code feeds
   `play_completion` unchanged, so the scorecard's completion axis already counts it.

An explanation that disagrees with the head's move is a failure, not a different move.

## Training (S1 arm, none run)

- Rows: `run1-reasoning-2026-09-a` train shards: `packet.legal_moves` is the mask, `target` the human
  move. Holdout is the existing `holdout-*.jsonl` (game-level split); held-out positions and families
  are preserved because the same corpus audit applies.
- Loss: legal-masked cross-entropy, nothing else. Host: the LoRA adapter on the same base as Run 1,
  trained jointly with the head (head lr about 10x the adapter lr); a head-only warm-up (frozen
  host) is the cheap first probe.
- Budget matched to the other arms: same rows and steps as the control comparison.

### Prepared command and estimate (not run)

    python3 -m model.policy_head.train --corpus /mnt/4tb/artifacts/KnightLM/expire/corpus/run1-2026-09-a --rows 200000 --epochs 1

This is a CPU preflight: it audits rows against the move space and prints rows, steps and the
estimate. `--gpu` is refused; the launch wiring (host forward plus head step, checkpointing) is owed
and needs the owner's go.

Estimate, unmeasured: Run 1 cost about 75 ms/row (625 steps x 32 rows in 25 minutes) with about 100
generated tokens per row. A prompt-only forward/backward is assumed 3 to 5 times cheaper, 15-25 ms/row:
20k rows about 5-8 minutes; 200k rows (one epoch) about 50-85 minutes, plus holdout scoring. Re-measure
on the first 200 steps before committing to the full pass.

## Evaluation axis `move_head` (`model/eval/move_head.py`)

A predictor exposes `logits(packet) -> 1968 raw floats`. Per row, in the one scorecard axis
`axes.move_head` (schema `knightlm.scorecard/move_head`, `composite_score: null`):

| block | measure |
|---|---|
| `masked` | top-1 and top-3 match with the human move over the legal set; mean NLL; calibration (ECE and reliability bins over top-1 confidence) |
| `raw` | unmasked argmax legal rate, match with the human move, mean illegal probability mass (diagnostic) |
| `centipawn_loss` | masked and raw moves scored through the existing `attach_cpl` / Stockfish path, after the decision; raw rows with an illegal argmax are unscored and counted |

A row whose predictor raised or returned the wrong length stays in every denominator as a failure.
Human match is not strength: report it beside CPL, and read both beside the Run 1 control (CPL 409,
random floor 417) and the language-move arms. `model.eval.run --move-head` (off by default) adds the axis to a play card next to the completion axes
and writes each row's head result into `predictions.jsonl` under `move_head`; the predictor needs
`logits(packet)` (`model.eval.playmode:head_from_env`). With `--prompt-kind play-head` the card also has
`axes.head_agreement`: per mode, how many completions pass every check with the head's move, the head's
move against the human move, and the refusal codes. `move_head.evaluate(rows, predictor, scorer)` stays the
library entry point. `model.eval.compare` compares the new axis like any other (ECE, illegal mass and NLL
read as lower is better); a `play-head` card does not compare with a `play` card without
`--allow-prompt-mismatch`.

## Serving (`model/serve/head.py`)

`HeadPredictor(base_dir, adapter_dir, device, head_dir=...)` is a serve `Predictor` plus a head saved by
`policy_head.head.save_head` (`head.pt`, `head.json`; the width is checked against the host). It offers

- `logits(packet)`: the 1968 raw logits (the eval axis input);
- `pick(packet)`: the masked pick, the masked logits and probabilities over the legal set, the raw argmax,
  whether it is legal and the probability mass the unmasked softmax puts on illegal moves;
- `predict_play(packet, mode)`: pick, then the conditioned prompt, then the language completion, checked with
  `policy_head.agree.interpret_with_head`. raw returns the completion as written (a move field or stated move
  other than m* is `stated_move_mismatch`). constrained sets the move field to m* when the model's reasoning is
  recoverable (path `constrained_move`) or writes the reasoning after a supplied call head
  (`constrained_forced_reasoning`); reasoning that names a different move stays a mismatch. No fallback move.
  The `PlayPrediction` carries `head_move`.

`python -m model.serve.server --base-dir DIR --adapter-dir DIR --head-dir DIR` serves it: `/v1/play` adds
`head_move` to its record, `POST /v1/head` returns the head alone, and the manifest gains `head_sha256`.
`resolve_head` holds the decision logic without torch.

## Tests

`python3 -m unittest model.policy_head.test_policy_head model.eval.test_move_head model.serve.test_head
model.eval.test_playhead` (torch tests skip without torch): on a tiny random-weight CPU host the loss falls and the masked argmax and top-k are
always legal while the unmasked argmax of an untrained head is not; planted metrics, calibration,
agreement codes, refusals, and the eval axis with fake predictors (perfect, illegal-favoured, raising,
wrong length) and a fake scorer.

## Out of scope

Sampling, search, engine-derived labels (a separate teacher arm), the joint training run, combining the head
with the board encoder, and any GPU measurement. Scoring commands are in `training-gate.md`.
