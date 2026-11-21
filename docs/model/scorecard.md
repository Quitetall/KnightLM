# KnightLM scorecard: axes, comparability and limits

Status: reference. This lists every axis the evaluator (`model/eval/`) can write into a
`scorecard.json`, what it measures, when two scorecards may be compared, and what it cannot
tell you. **There is no composite score**: `composite_score` is `null` in every scorecard, by
design (KLM-WAR-0008 OBL-002). Do not add one, and do not rank checkpoints by a sum or an
average of axes. The protocol that fixes the settings is in [strength-baseline.md](strength-baseline.md).

## Rules that apply to every axis

- **Every requested row is in the denominator.** A missing, timed-out, empty, unparseable or
  illegal reply is a failure on its row, never a dropped one. An axis that is defined only for
  some rows (centipawn loss: legal moves only) reports how many rows it covers beside it.
- **Two scorecards are comparable only when their `inputs.json` agree** on: corpus manifest and
  shard hashes, row count and order, mode, prompt kind, scoring engine binary hash and depth,
  tactical-set hash, game openings hash and opponent depth. A checkpoint comparison changes
  only the predictor manifest. `model/eval/compare.py` prints a `NOT COMPARABLE` note for a pair that differs in rows, Stockfish depth or binary, tactical set, modes, or lc0/Maia settings and weights, and exits 3 for different prompt kinds; it does not check every item listed here, so read `inputs.json` yourself.
- **Post-decision oracles only.** Stockfish, lc0 and Maia run after the model has committed to
  a move. Nothing in `model/serve`, `model/prompt.py` or a crate reachable from inference
  imports them (tested), so the model never sees an engine.
- **Raw and constrained are separate.** Constrained decoding cannot be illegal, so its legality
  is a check of the decoder, not of the model. Never compare a raw number with a constrained one.
- **Prompt kinds are separate scorecards.** `move` (packet in, UCI move out), `play` (FEN packet,
  reasoning and move) and `play-cgt` / `play-cgt-full` (Chess Graph Text) carry their own axis
  names, so a play scorecard is never silently read as a move-only one.
- **A skipped oracle is `{"status": "skipped", "reason": ...}`**, never a zero.

## Axes

| Axis (scorecard key) | Measures | Comparable when | Known limits |
| --- | --- | --- | --- |
| `raw_legality` (play: `play_raw_legality`) | Share of rows where unconstrained generation names a legal move. Reported against the 90% threshold of KLM-WAR-0008 (`gate.met`); the threshold never filters rows. | Same rows, prompt kind and mode. | A legal move is not a good move. In play kinds the completion must also parse and agree with its stated reasoning, so play legality is stricter than move legality. |
| `constrained_legality` (play: `play_constrained_legality`) | Legality with decoding limited to the legal list. | As above. | Near 100% by construction; it tests the decoder. |
| `play_completion` | Play kinds only: how each completion ended (code and path), including a legal move that disagrees with its reasoning (`inconsistent`). | Same prompt kind. | Describes the reply format, not chess quality. |
| `played_move_match` | Share of rows where the model's move equals the human move in the corpus (`target`). | Same rows. | The human move is one sample of human play, not the best move; a perfect engine scores well under 100% here. Measures imitation, not strength. |
| `centipawn_loss` | Stockfish (fixed depth 12, 1 thread) centipawn loss of the chosen move against the best move, mean and percentiles, for legal moves only. `engine` records binary hash and depth. | Same Stockfish binary and depth, same rows. | Undefined for illegal moves (reported as `rows_unscored`, not dropped); so a model with low legality can show a low CPL on few rows. Read it beside legality. Fixed depth, not strength in games. |
| `tactical` | Share of a derived tactical subset (best move beats the second by at least 150 cp at derivation depth, side to move not already lost) where the model plays the engine's best move. | Same tactical-set sha256. | The set is derived by Stockfish, not curated; the "right" answer is Stockfish's. Small subset, so wide confidence intervals; check `rows_in_set`. |
| `token_cost` | Input and output tokens per decision, per mode. | Same tokenizer and prompt kind. | Different tokenizers count differently; compare only within one tokenizer. |
| `latency_ms` | Wall time per decision, p50 and tail. | Same hardware and load only. | Depends on the machine, batch size and what else is running; never a model property on its own. |
| `failures` | Count of each failure kind (illegal, unparseable, empty, exception, timeout, missing; play adds inconsistent). | Same rows and mode. | Counts, not a rate; use beside the legality denominators. |
| `games` | Result of a short match against Stockfish at a fixed shallow depth (default 1), colors swapped over fixed openings, by termination. An illegal move or player error loses. | Same opponent depth, openings hash, max plies and game count. | Few games: one game moves the score by `1/games`. Depth-1 Stockfish is a floor, not a strength rating. Draws by move limit are counted as played. Also the only axis that exercises `knightlm-uci` end to end (see below). |
| `lc0_strength` (optional) | lc0 at a fixed node count: centipawn loss of each legal move from the net's value head. | Same lc0 binary, weights hash, nodes, backend. | Needs an lc0 binary and a network supplied by the owner; none ships or is downloaded. lc0 and Stockfish centipawns are not comparable, so this axis stays separate from `centipawn_loss`. Backend changes (blas vs GPU) can move results slightly; the backend is recorded. |
| `maia_human_likeness` (optional) | Share of rows where the model's move equals Maia's top move at one node, at a chosen rating (default 1500), beside how often the human move equals it. | Same Maia rating, weights hash, lc0 binary. | Maia is a model of human play at a rating band; a match is agreement with that model, not strength. The beside-figure (human vs Maia) is the ceiling to read it against. `MAIA_WEIGHTS_DIR` takes either a flat directory of `maia-<rating>.pb.gz` or the stored `<rating>/maia-<rating>.pb.gz` layout. |
| `explanation` | Mechanical check of claims in the model's free-text reasoning against the board: piece on square, square state, move legal, capture, check, capture target, material. Counts checkable and failed claims per type, plus claims seen but not checkable, each with a reason. | Same claim extractor version and rows. | Not an understanding check: an explanation can make zero checkable claims, and that count is reported. Sentences about replies or hypothetical lines ("then Black plays", "if") are counted unchecked, not true or false. Mate is checked as check only. This scorecard is kept stable; do not change its keys. |
| `s0_rules_qa` | Exact-answer match per question type for S0 rules questions (answered in the play-call `reasoning` field), against compiler-derived answers. Claim-failure rate is 1 minus the exact rate. | Same S0 corpus and question types. | One rate per question type, no total. Exact character match is strict (a correct answer in other words fails). Reported beside, never merged with, `explanation`. |
| `move_head` | Move-head arm (no language decoding): masked top-1 and top-3 match with the human move, masked NLL, calibration (ECE), CPL of the masked top-1, and the raw diagnostic (legality, probability mass on illegal moves). | Same rows and 1968-move space order. | Needs a predictor exposing `logits`; the raw view is diagnostic and not the product. Its arm label keeps it apart from language-model scorecards. |

## The knightlm-uci game gate

`scripts/knightlm_uci_game_gate.sh` (test: `model/serve/test_uci_gate.py`) is a plumbing gate,
not an axis. It plays a short match of the `knightlm-uci` binary against Stockfish through the
real serving app (`model.serve.fake_server`, a fake predictor that answers the first legal move)
using the `knightlm-match` driver, and fails on any illegal move, forfeit or error. It shows
that the packet, serve contract, UCI layer and match driver agree. It says nothing about model
strength or about any real checkpoint. Stockfish is limited by depth (default 1), because the
driver has no node limit.

## What this scorecard cannot say

- Anything about strength in rating terms. No axis is an Elo estimate.
- Anything about untested positions: the holdout is Run 1 only (lichess games from a later time
  window than training).
- Whether a change in one axis is real, without the denominators and the intervals: with a few
  hundred rows, differences of a couple of points are noise.
