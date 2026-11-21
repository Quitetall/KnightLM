# Move-policy head (arm: human-only move head)

Status: specification and Python reference (`model/policy_head/`, `model/eval/move_head.py`).
Nothing trains; no prompt, serve or Rust path uses it. `model/prompt.py`, `model/tokenizer`,
the encoder and the coach are untouched. This is goal item 5 of the approved plan ("move-policy
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
   move field being constrained to m* on the existing `constrained_move` path. That needs a new prompt
   kind (provisionally `play-head`) in `model/prompt.py` with parity goldens: owed, a separate PR,
   because other work is editing that area.
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
random floor 417) and the language-move arms. The axis is not wired into `model.eval.run` yet
(it needs a serve-side logits predictor); `move_head.evaluate(rows, predictor, scorer)` is the entry point.

## Tests

`python3 -m unittest model.policy_head.test_policy_head model.eval.test_move_head` (torch tests skip
without torch): on a tiny random-weight CPU host the loss falls and the masked argmax and top-k are
always legal while the unmasked argmax of an untrained head is not; planted metrics, calibration,
agreement codes, refusals, and the eval axis with fake predictors (perfect, illegal-favoured, raising,
wrong length) and a fake scorer.

## Out of scope

Sampling, search, engine-derived labels (a separate teacher arm), serving the head over HTTP, and the
`play-head` prompt kind.
