# Reasoning v2: short, compiler-grounded reasoning

Status: spec and builder (`model/corpus/reasoning2.py`), rejection-sampling loop
(`model/corpus/rejection.py`), and a 20,000 / 1,000 pilot of both variants. No training has run.

## Why

Run 1 (short pass) learned the output format but not the board: 295 of 300 sampled reasonings held a false
claim, and 65% of piece-on-square claims failed. `reasoning-v1` was rules-derived but long, and some of its
sentences ("is attacked and defended", move counts) are facts nothing downstream re-checks. v2 states only
what the compiler computes and an independent oracle re-checks, and it is shorter (mean 186 characters
against 254 for v1 in variant a).

## What a v2 reasoning may say

Only these, in this order, deterministic text with no variation by hash:

1. side to move, and "X is in check" when it is;
2. material, as `reasoning.material` words that `model.eval.explain` checks;
3. 0 to 3 facts, in priority order, dropped from the end when the text would exceed its limit:
   `victim` (defenders of the piece the move captures), `hanging` (the most valuable own piece the
   opponent attacks, with its attackers and defenders), `loose` (the most valuable undefended enemy piece
   the side to move can capture);
4. the chosen move: UCI, piece and squares, capture, castling, en passant, promotion, check.

No count of legal moves, no ranking, no plan, no "good" or "best". "Attack" and "defend" are the geometric
`attackers_of` / `defenders_of` edges (pins ignored), as in docs/model/graph-text.md.

## Two variants

| | a: text | b: tool quoting |
|---|---|---|
| completion | one `submit_move` call | play-schema read calls, each followed by its result, then one `submit_move` |
| reasoning limit | 330 characters, 55 words | 400 characters (schema bound), 75 words |
| tools | none | `attackers_of`, `defenders_of`, `gives_check`, `what_if` (checking moves), `list_legal_moves` (at most 5 legal moves) |
| prompt | `model.prompt.play_prompt("play", packet)` | `play_prompt("play-reads", packet)`: the same prompt with the "exactly one tool call" sentence replaced |

Variant b transcript, results rendered by the compiler (`knightlm corpus-reads`, which runs the calls through
`RuleSession`, the harness's executors, and prints `<tool_result>{reply}</tool_result>` exactly as
`model_turn::format_tool_result` does; call numbers run 1..n):

```
<tool_call>{"name":"gives_check","arguments":{"move":"c2e4"}}</tool_call>
<tool_result>{"call":1,"ok":true,"result":{"gives_check":false,"move":"c2e4"},"error":null}</tool_result>
<tool_call>{"name":"defenders_of","arguments":{"square":"e4"}}</tool_call>
<tool_result>{"call":2,"ok":true,"result":{"square":"e4","squares":[]},"error":null}</tool_result>
<tool_call>{"name":"submit_move","arguments":{"reasoning":"White to move. ... The black pawn on e4 is undefended (defenders_of e4: none). ... (gives_check c2e4: false).","move":"c2e4"}}</tool_call>
```

The reasoning quotes each result as `(tool arg: squares)`. A row carries `segments`: the calls and the
final `submit_move` have `loss: true`, the results `loss: false` (the harness inserts them at inference).

## Checks (build and audit)

A row is refused unless all of these hold; `python -m model.corpus.pipeline audit DIR --knightlm BIN`
(and `python -m model.corpus.reasoning2 audit`) re-derives every row from the compiler:

1. `model.eval.explain` finds zero failed claims and at least one checkable claim (the pilot also has
   zero unchecked claims);
2. every attacker and defender list the compiler returned equals an independent Python recomputation
   (`attackers_py`), and `gives_check` and `list_legal_moves` equal `corpus-facts`;
3. the whole row equals a fresh derivation (a hand edit that keeps every claim true is still refused);
4. variant b: results are the compiler's and every quote matches a result in the transcript.

The manifest records the rules revision (git tree ids of `crates/core` and `crates/notation`, as `s0.py`
does), binary and generator hashes, claim counts per type, and the parent corpus. Splits are the parent's
(Run 1 reasoning corpus), so the leak audit is inherited.

## Rejection sampling (rules rewards only)

`python -m model.corpus.rejection run --corpus DIR --predictor pkg.mod:fn --n 4 --split train --limit N
--out kept.jsonl --stats stats.json`. A predictor is `fn(row, n) -> list of completions` (a real one wraps
the serving harness; for variant b it returns the full transcript the harness produced). A sample is kept
only if it parses, its results and quotes are the compiler's (b), the move is legal, `explain` finds zero
failed claims (and no unchecked claim unless `--allow-unchecked`), and the move equals the chosen move
(`row["target"]`, or a move-policy head's move via `chosen=`). The stats give reasons per rejection and
the kept count per row. Holdout samples measure pass rate only; the CLI refuses to write them.

Engine-scored rewards are not here. They are a separate labelled arm (plan S4).

## Prompt kind and loss mask for variant b

`play-reads` (`model/prompt.py`, `READS_KINDS`) is the play prompt whose instruction allows the read tools before
`submit_move`: `PLAY_READS_INSTRUCTION` replaces `PLAY_INSTRUCTION` ("exactly one tool call") for that kind only; the schema,
the call format and the packet are the same bytes, and the goldens of every older kind are unchanged
(`model.tests.test_prompt`, `model.train.test_masked`). It is a training and scoring kind: the Python server answers one call
and refuses it (`Predictor._check_play_kind`, `--play-prompt-kind`); the Rust harness runs the read calls between turns.
`model.corpus.reasoning2 build --variant b` writes `prompt_kind: play-reads` rows; the first pilot (`...-pilot-2026-11-b`,
prompt kind `play`) still validates and audits, but a b corpus that trains the intended prompt has to be rebuilt (not done).
`parse_play_completion` still accepts only the final call; a transcript is parsed by `reasoning2.parse_transcript`.

Loss mask: a row's `segments` mark the calls and the final `submit_move` `loss: true` and the tool results `loss: false`
(the harness inserts them at inference). `model/train/segments.py` turns that into labels (-100 over the prompt, padding and
result spans; refuses a span boundary that falls inside a token) and the joint trainers use it: `model.policy_head.joint`,
`model.encoder.task` and `model.train.masked` (the text-only entry for variant b: `python -m model.train.masked --corpus
<b corpus> --run r2b-masked`). Measured on the first 64 rows of the b pilot: 8,511 of 21,954 completion tokens (39%) are
results and carry no loss.

BLUT limitation: the recipe's trainer (`hf_sft_train`, `hf_image_text_data.row_texts` / `collate`) reads `{prompt, completion}`
only and puts loss on every completion token. `model.train.run` therefore refuses a corpus with result spans (`loss_segments`
check; `--allow-unmasked-results` trains them with loss, which is a different, labelled arm). The fix upstream is a
`labels`-or-`segments` field in the image-text row schema; until then variant b trains through `model.train.masked`.

## Owed (not done, not tested)

- Rebuild the variant b corpus with the `play-reads` prompt and no GPU run of either variant.
- A serving loop that runs the read calls between model turns exists in Rust (`execute_read`,
  `MoveHarness::Play`), but no model has produced a v2 transcript, so the sampler is tested with fake
  predictors only.
