# KnightLM strength baseline protocol

Status: proposed for review (ticket t-e135, implemented by t-c199). Nothing here
is a strength claim. No trained checkpoint exists yet; the harness exists so the
untrained base model can be scored now and the change from training measured
later under identical rules.

The protocol keeps five things apart, as the [model plan](plan.md) requires:

| Axis | Question it answers |
| --- | --- |
| Raw legality | Does the model, unaided, name a legal move? |
| Constrained legality | Does it name a legal move when decoding is constrained to the legal list? |
| Tactical accuracy | On positions with one clearly best move, does it find it? |
| Game results | How does it fare against a fixed Stockfish setting, by how each game ended? |
| Latency and token cost | What does a decision cost? |

Played-move match (the human move) and Stockfish centipawn loss are reported
beside legality. **There is no composite score**, no weighting across axes, and
no strength gate. The only threshold is the one KLM-WAR-0008 fixes: raw legality
of at least 90% is *reported* as met or not met. It never filters rows.

## What is fixed

A second person should be able to repeat a run from this section alone.

| Setting | Value |
| --- | --- |
| Positions | Run 1 holdout, `/mnt/4tb/artifacts/KnightLM/expire/corpus/run1-2026-09-a`, first N rows in manifest shard order. N = 10,000 for a headline run, 1,000 for a smoke run. |
| Corpus identity | `corpus_manifest_sha256` and every holdout shard sha256, verified against the manifest before use and recorded in `inputs.json`. A mismatch aborts. |
| Model input | The row's compiled `packet` (FEN, `legal_moves`, rule facts, repetition ledger), exactly as the corpus stores it. |
| Model settings | Whatever the predictor's manifest records (base revision, adapter sha256, prompt template, greedy decoding). The harness stores the predictor manifest and never overrides it. A run without a manifest is labelled as such. |
| Modes | `raw` (unconstrained generation) and `constrained` (decoding limited to `legal_moves`). Both are always run. |
| Per-prediction timeout | 60 s (`--timeout-s`). A reply later than this is a `timeout` failure. |
| Scoring engine | Stockfish 19, fixed depth 12 (`--depth`), 1 thread, 64 MB hash, `ucinewgame` before every search. A fixed-depth score then depends on nothing but the position, the move and the binary; the same inputs score identically for any worker count or order (tested). Binary sha256 and depth are recorded. |
| Tactical set | Derived below; its sha256 is recorded. |
| Game opponent | Stockfish 19, fixed depth 1, 1 thread, a fresh engine process per game. |
| Game openings | `model/eval/openings.txt` (12 six-ply openings), sha256 recorded. |

Stockfish is used only after the model has committed to a move: scoring the
chosen move, and as the opponent in games. Nothing in `model/serve` calls an
engine (WAR-0005 engine-zero). The harness reaches Stockfish only through the
Rust tool (`knightlm score-moves`, `knightlm rank-positions`, `knightlm-match`),
which wraps `crates/engine`; Python never speaks UCI.

## Held-out positions and independence from training

The Run 1 corpus keeps one decision at ply 24 from each selected rated blitz or
rapid Lichess game, both players rated 1600 or above. Its holdout is strictly
later in time than every training game, with disjoint Lichess game IDs, a
reserved ECO two-character group (B0) and disjoint scored positions. The corpus
builder audits all of this, and the manifest records it. The target in the
corpus is the human-played move, not an engine move.

Everything here uses only those holdout rows, so the evaluation inherits that
independence. Evaluation inputs and outputs live in
`/mnt/4tb/artifacts/KnightLM/keep/eval-<label>-<UTC>/`, separate from training
data. One caveat to carry into any claim: the holdout positions come from the
same distribution as training (human ply-24 positions), so they measure
generalization to new games, not out-of-distribution strength.

## Tactical set (derived, not curated)

The repository has no puzzle collection and the plan forbids inventing
annotations, so the set is **derived from the holdout** by Stockfish MultiPV
margin:

1. Analyse every holdout position with Stockfish 19 at depth 16, MultiPV 2.
2. Keep a position when the best move beats the second-best by at least 150
   centipawns (scores clipped to +-1000, so a forced mate counts as 1000) and
   the side to move is not already worse than -300.
3. The expected answer is the engine's best move, not the human move.

Result on the full 10,000-row holdout: 1,324 positions (13.2%).
File: `/mnt/4tb/artifacts/KnightLM/keep/eval-tactical-set-run1/tactical-set.json`,
sha256 `50fee2c3b8c3f1ee5beedb4909b7827f4e0c1359e69f298ee374fc41a9dedd81`. Regenerate with:

```sh
python3 -m model.eval.tactics --corpus $CORPUS --depth 16 --margin 150 --out tactical-set.json
```

Each item stores its holdout row index, game ID and FEN; loading checks them
against the corpus, so a set built for another corpus is refused. Limits: the
labels are engine opinions at one depth, and "tactical" means "one move is
clearly better", which also admits quiet only-moves.

## Scoring rules

Let N be the number of requested rows. Every rate below uses N (or the tactical
set size) as its denominator. Nothing is dropped.

**Reply status**, decided by the harness and never by the predictor's own
`legal` flag (disagreements are counted):

| Status | Meaning |
| --- | --- |
| `ok` | `move` is exactly one of the row's `legal_moves`. |
| `illegal` | Looks like UCI (`e2e4`, `e7e8q`) but is not legal here. |
| `unparseable` | Not UCI-shaped: prose, wrong case, extra text, out-of-range squares. |
| `empty` | No move or an empty string. |
| `exception` | The predictor or server raised, or returned an HTTP error. |
| `timeout` | No reply within the timeout. |
| `missing` | Reserved: a requested row with no prediction. The scorecard builder refuses to produce a card if any requested row has no record. |

Axes:

- **Raw / constrained legality** = `ok` rows / N, per mode. Raw also reports
  whether the 90% gate is met.
- **Played-move match** = rows where status is `ok` and `move == target` / N.
- **Centipawn loss (CPL)** of an `ok` move: the engine's best score for the
  mover at depth 12, minus the mover's score after the move (searched at the
  same depth from the child position), clipped to +-1000 and floored at 0. A
  move equal to the engine's best has CPL 0. A move that mates scores +1000. CPL
  is undefined for failures, so its block always prints `rows_scored` and
  `rows_unscored` beside the mean and median. A model that fails often can show a
  low CPL on the few moves it plays; read CPL only together with legality.
- **Tactical accuracy** = rows where status is `ok` and the move equals the
  set's best move / tactical set size, per mode. Failures are misses. The block
  also gives the share of tactical rows with CPL <= 50.
- **Token cost**: input and output token counts per prediction as reported by
  the predictor from its real tokenizer, with mean, p50, p90, p95, p99 and max,
  and the number of rows that reported. Passing `--tokenizer tokenizer.json`
  also recounts output tokens from the raw reply and counts disagreements.
  Input tokens are not recounted, because the prompt template belongs to the
  serving lane.
- **Latency**: p50, p90, p95, p99, max of `latency_ms` over rows that returned,
  with nearest-rank percentiles. If the predictor does not report latency the
  harness uses its own wall clock. Timed-out rows have no latency and are not
  invented. Latency depends on hardware; it is measured, not reproducible
  bit for bit. Record the machine in the run notes.
- **Failure accounting**: counts per status per mode, plus `predictor_legal_claim_disagrees`.

## Game protocol

Games use the existing `knightlm-match` driver (`ModelPlayer` over HTTP against
`EnginePlayer`), extended with the options below.

- Opponent: Stockfish 19, fixed depth 1. Depth 1 is chosen because an untrained
  model needs an opponent it can plausibly score against; a trained model that
  saturates it should be rerun at depth 4 under a pre-registered amendment, not
  by quietly changing the setting.
- Games: an even number. Game g uses opening `g / 2` of the fixed list and the
  colors swap on odd g, so each opening is played from both sides. The
  default run is 24 games (12 openings x 2 colors). **Do not run a larger series
  before this protocol has been reviewed** (t-e135).
- Openings: 6 plies, recorded in `model/eval/openings.txt`. A bad opening is a
  configuration error, never a game result.
- Draws: a claimable threefold repetition or fifty-move draw is adjudicated as
  a draw at once. Fivefold, 75-move, stalemate and proven dead positions end the
  game by the compiler's rules. A game still running at 200 plies is a draw
  (`move limit`).
- Failure: an illegal move from the model loses the game (`illegal move by
  subject`). Any other player error (HTTP failure, no `move` in the response,
  timeout of 120 s) also loses it (`player failure by subject`), with the error
  text recorded. Nothing is retried and no legal move is substituted.
- Mode: games use the `raw` mode by default (`--game-mode`). The mode is sent
  to the server as `?mode=`.
- Aggregation: wins, draws, losses, score = (W + D/2) / games, and a table of
  outcome by termination (`checkmate`, `illegal move by subject`, `player failure
  by subject`, `threefold repetition claimed`, `move limit`, ...). "Subject" is
  the model, whichever color it held. Per-game moves are kept in `games.json`.

With 24 games the score has a wide uncertainty (a single game is 4 points of
score). Treat game results as a sanity check on playing at all, not as an Elo
estimate.

## Worked scoring example

Fixture: six real Run 1 holdout rows in `model/eval/fixtures/worked-corpus`
and a scripted predictor, `model.eval.fakes:worked_example`, that plants one of
every outcome:

| Row | Human move | Raw reply | Raw status | Constrained move | CPL (constrained) |
| --- | --- | --- | --- | --- | --- |
| 0 | g4h5 | `g4h5` | ok, matches | g4h5 | 0 |
| 1 | d1f3 | `a1b1` | ok, differs; engine best f2f4, CPL 375 | a1b1 | 375 |
| 2 | c1f4 | `e1e8` | illegal | a1b1 | 111 |
| 3 | d2d4 | `Knight to f3` | unparseable | a1a2 | 53 |
| 4 | e3f4 | (no reply in 1 s) | timeout | a2a3 | 17 |
| 5 | f4g5 | server error | exception | a1b1 | 115 |

Scoring by hand, N = 6:

- Raw legality: rows 0 and 1 are legal, so 2 / 6 = 0.3333, **gate not met**.
  Four failures: 1 illegal, 1 unparseable, 1 exception, 1 timeout.
- Constrained legality: 6 / 6 = 1.
- Played-move match: raw 1 / 6 = 0.1667 (row 0). Constrained 1 / 6 = 0.1667.
- Raw CPL: only rows 0 and 1 are scored: (0 + 375) / 2 = 187.5, with
  `rows_scored` 2 and `rows_unscored` 4. The mean looks middling only because the
  four failed rows are excluded from it; the four unscored rows are the larger
  fact, and the card prints them beside it.
- Constrained CPL: (0 + 375 + 111 + 53 + 17 + 115) / 6 = 111.8333, median 53 (nearest-rank).
- Latency (raw): four rows returned (10, 12, 14, 16 ms), so n = 4, p50 12 ms;
  the timeout and exception rows have none.
- Tokens (raw): output tokens 4, 4, 4, 8 over the four rows that reported.

Reproduce (about 20 seconds, CPL via Stockfish):

```sh
python3 -m model.eval.run --predictor model.eval.fakes:worked_example \
  --label worked-example --rows 6 --corpus model/eval/fixtures/worked-corpus \
  --timeout-s 1 --no-tactics --out-root /path/on/ssd
```

`python3 -m unittest model.eval.test_eval` asserts the same numbers with a
scripted scorer, and the Stockfish integration test checks that the engine's
scores do not change with worker count.

## Running it

```sh
# build the Rust tool once (release binaries land in target/release)
oneoff cargo build --release -p knightlm-cli

# score a predictor (Python spec or a running server)
python3 -m model.eval.run --predictor <module:factory returning a serve Predictor> \
    --label base-untrained --rows 10000 --games 24
python3 -m model.eval.run --predictor http://127.0.0.1:8000 --label run1 --rows 10000 --games 24

# per-axis deltas, B minus A, with a comparability check
python3 -m model.eval.compare /mnt/4tb/artifacts/KnightLM/keep/eval-base-untrained-... \
                              /mnt/4tb/artifacts/KnightLM/keep/eval-run1-...
```

Outputs per run: `scorecard.json`, `scorecard.md`, `predictions.jsonl` (one
record per row with both modes, reply, status, tokens, latency, CPL),
`inputs.json` (corpus manifest sha256 and shard hashes, predictor manifest,
Stockfish version, sha256, depth and settings, git commit and dirty flag,
tactical-set sha256), and `games.json` when games were played.

`compare` flags `NOT COMPARABLE` when the corpus manifest, row count, Stockfish
depth or binary, tactical set, or modes differ. Only compare runs it does not
flag.

Repeatability: given the same corpus, predictor outputs, Stockfish binary and
settings, `scorecard.json` (minus its `run` block), `predictions.jsonl` and the
tactical set are byte-identical (tested with a deterministic predictor, with
different Stockfish worker counts). Model decoding and latency are the
predictor's responsibility.

## Reference floor (not a model)

`model.eval.baselines:random_legal` picks a uniformly random legal move, seeded
from the position. Scored on the first 1,000 holdout rows with the 24-game
protocol it gives the numbers any model scorecard should be read against:
legality 1.0 by construction, played-move match 0.037, mean CPL 408.7 (median
305), tactical accuracy 4/130 = 0.031 (130 of the 1,324 tactical positions fall in the first 1,000 rows), and 24 games against depth-1 Stockfish
scoring 0.042 (0W 2D 22L; 22 checkmates, 2 claimed threefold repetitions). Its card is kept at
`/mnt/4tb/artifacts/KnightLM/keep/eval-random-legal-floor-*/`. It has no raw-output
failure mode, which is exactly what the raw-legality axis exists to expose in a
language model.

## Not covered

- Strength in Elo. Depth-limited Stockfish at depth 1 is not a calibrated rating.
- Perception (the image pass), explanations, causal accuracy and language
  retention from the plan: separate axes, not built here.
- Pilot scorecard of a trained model (t-c199 second item): needs a checkpoint.
- Syzygy: the local tables are 3-piece only and are not used as an oracle.
- A per-game wall-clock budget. Only per-move timeouts exist.
