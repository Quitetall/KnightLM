# Modes and tools contract

Status: proposed (ADR `KLM-ADR-modes-and-tools`, not signed). Extends
`docs/model/plan.md` and `docs/model/contracts.md`; where this file is silent,
those govern. The implemented parts are marked **Implemented**; the rest is
**Spec only**. The owner's decisions on the former open questions are recorded
in section 8 and applied below.

## 1. Context rule

The chess state is always one fresh `ContextPacket` (contracts.md). There is no
game replay in the prompt. The packet is compiled outside the model from the full
history, so repetition and clock facts are in it already. Chat, when on, adds
only bounded text (section 3); it never carries board state.

Prompt order is fixed for prefix caching:

1. mode instruction and tool-call format (fixed per mode);
2. the mode's tool schema JSON (fixed per mode and schema version);
3. coach only: bounded chat context;
4. the packet, last.

Items 1 and 2 are byte-identical across all turns of a mode. **Implemented:**
`model.prompt.prefix`, `play_text`, `coach_text`.

The existing move prompt (`INSTRUCTION` + packet, `user_text`, `user_chat`) is
unchanged. Existing corpora, the baseline eval and the served model use it;
`model/tests/test_prompt.py` pins its bytes. The mode prompts are additive and
are not used to build any current training row. Play mode is served (`POST /v1/play`) and
scored (`model.eval.run --prompt-kind play`) by `model.serve` and `model.eval`; see
`docs/model/strength-baseline.md`.

## 2. Play mode (competition, chat off)

The model sees the play instruction, the play tool schema and the packet, and
nothing else. It writes a short reasoning field (at most 400 characters) and then
calls `submit_move` with the key order `reasoning`, `move`. The harness shows only
that reasoning and the move. No chat context, no coach tools.

**Completion format (trained from the start).** The reasoning field is trained on
in play mode, from rules-derived text (another workstream builds that data). The
completion a play row is trained to write is exactly one call and nothing else:

```
<tool_call>{"name":"submit_move","arguments":{"reasoning":"<1 to 400 characters>","move":"<uci>"}}</tool_call>
```

`reasoning` is required and non-blank, comes before `move`, and `move` has UCI
shape (`[a-h][1-8][a-h][1-8][qrbn]?`); legality is the gate's job, not the
parser's. Text outside the block, a second block, another tool, swapped or extra
keys, or an over-long reasoning are refused with a stable code: `not_one_call`,
`extra_text`, `malformed`, `wrong_tool`, `key_order`, `reasoning_missing`,
`reasoning_too_long`, `bad_move`. (A model that was not trained on this format is
still read leniently by `parse_model_call`, which ignores text outside the block.)
**Implemented:** `parse_play_completion` / `format_play_completion` in
`crates/ai/src/model_turn.rs` and `model/prompt.py`, pinned to each other by
`crates/contract/schemas/play_completion_cases.json`.

Play tools (all act on the packet position only; the model never reaches an
engine, tablebase, database or opening explorer):

| tool | effect | purpose |
|---|---|---|
| `submit_move` {reasoning?, move} | board-write | the turn's move, UCI; ends the turn when legal |
| `list_legal_moves` {} | read | all legal moves, UCI |
| `what_if` {moves[1..8]} | read | play moves on a scratch copy; returns FEN, check, legal-move count |
| `attackers_of` {square} | read | squares of attacking pieces |
| `defenders_of` {square} | read | squares of defending pieces |
| `gives_check` {move} | read | whether a legal move gives check |

Results are rules facts. They carry no evaluation, score, ranking or
recommendation (contracts.md: "No evaluation score, recommendation, automatic
search or tablebase"). **Implemented:** the schemas, call parsing, visibility
check and the `submit_move` retry gate, and the read executors
(`RuleSession::{list_legal_moves, what_if, attackers_of, defenders_of,
gives_check}`, dispatched by `model_turn::execute_read`). Read queries share the
one eight-call `RuleSession` budget (`READ_BUDGET`) with each other; every call,
including a bad request, spends it, and the ninth is refused with a result the
model sees. `attackers_of` lists the pieces of the colour opposing the piece on
the square and `defenders_of` the pieces of its own colour, both ignoring pins
like `DerivedFeatures::attacks`; an empty square is an error (use `what_if`).
Answers are rules facts with no score, evaluation or ranking field.

## 3. Coach mode (chat on)

Same fixed prefix structure, with the coach instruction and coach schema, then
bounded chat context, then the packet.

Chat context is a rolling summary (at most 1200 characters), or the last
6 messages (each at most 800 characters), or both. Older messages are dropped;
a summary or message over its bound is an error, not silent truncation. The
summarizer is a separate step outside the model and makes no model call.
**Implemented:** `summarize_chat` / `roll_chat` (Rust and Python, pinned by
`crates/contract/schemas/chat_summary_cases.json`). Each folded message becomes
`U: <excerpt>` or `A: <excerpt>` (whitespace flattened, at most 120 characters, a
cut ends in `…`), entries are joined with ` | `, the previous summary comes first,
and when the result exceeds 1200 characters the oldest whole entries are dropped
and the summary starts with `…`. Same input, same output.

The model plays first, then replies. **Commit rule (owner decision):** the model
emits a provisional move first, as a line `Provisional: <uci>`, and may revise it
by emitting a later `Provisional:` line while it thinks. The harness validates
every such line through the same gate as `submit_move` (section 5), so each line
spends one attempt of the shared retry budget. The last legal provisional move is
the committed move when the model stops, runs out of tokens or time, or exhausts
its retries. An illegal later line never erases a legal earlier one. A stream cut
off mid-line ignores the unterminated last line (it may be a move cut short). If
no legal provisional exists, the existing constrained-decoding fallback picks the
move and the turn records `fallback`. A round with no usable line is answered with
the gate's error and the model may try again while attempts remain. The reply
(the text with the `Provisional:` lines removed) is shown only once the move is
committed, and not at all after a fallback. Coach has no `submit_move`, so a coach
tool can never submit a real move. **Implemented:** `ProvisionalTracker`
(`model_turn.rs`) and `MoveHarness::Coach` (`play.rs`).

Coach tools (scratch board and presentation only):

| tool | effect | purpose |
|---|---|---|
| `scratch_set` {fen} | read | set a scratch position; the real game is untouched |
| `scratch_step` {moves[1..8]} | read | play moves on the scratch board |
| `show_game_move` {ply} | read | point at a ply of the game |
| `mark_arrow` {from, to, color?} | board-write | draw an arrow |
| `mark_square` {square, color?} | board-write | highlight a square |
| `clear_marks` {} | board-write | remove the model's marks |

**Implemented:** `crates/ai/src/coach.rs`. A `CoachSession` holds the real game and a separate
scratch branch (it starts at the real position). `scratch_set` validates a FEN, `scratch_step` plays
1 to 8 moves all-or-nothing, `show_game_move` points at ply 0 to N of the real game, and the mark tools
add or clear arrows and highlights (green when no colour, at most 16 marks, an identical mark replaces
itself). Nothing here changes the real game. Marks and the ply pointer are returned as structured
`UiEffect` data (`show_ply`, `add_mark`, `clear_marks`) for RBC to draw; the model sees only rules
facts (FEN, check, legal-move count). Errors are typed with stable codes: `not_coach_tool`,
`call_budget` (32 calls a turn), `bad_arguments`, `bad_fen`, `illegal_move`, `move_count`, `bad_ply`,
`bad_square`, `bad_color`, `same_square`, `too_many_marks`. In the harness (`MoveHarness::Coach`) every
`<tool_call>` block of the coach text is checked against the coach schema and run; a refused or failing
call is shown to the model on the next round but is not a move attempt, and after a fallback the marks
and the reply are withheld together. The serving side (`model/serve/coach.py`) forwards valid coach
calls as OpenAI `tool_calls` for these executors and refuses `submit_move`, play-only tools and app tools.

A long chat shows both schemas: the harness may present coach and play schemas in
one prompt for a long chat, but each call is still checked against the schema of
the mode the harness assigned to that turn. The combined prefix is its own fixed
prefix: combined instruction, call format, `Play tools:` and `Coach tools:` JSON.
The turn's mode follows it on a `Mode: play|coach` line, then the chat context,
then the packet last, so the prefix stays cacheable. **Implemented:**
`combined_prefix` / `combined_prompt` (Rust) and `combined_prefix` /
`combined_text` (Python), byte-pinned by `crates/contract/schemas/model_prompts.json`.

## 4. Tool schemas

Defined once in `crates/ai/src/tools.rs` (`model_tool_specs`). The registry is
separate from the app registry, so the model cannot name `analyze`, `evaluate`,
`play_move` or any other app tool. `crates/contract/schemas/model_tools.json` is
generated from the Rust specs (`UPDATE_MODEL_TOOLS=1 cargo test -p rbc-ai
model_tools_json`); a Rust test fails when it drifts, and Python reads that file.
Schema version is `TOOL_SCHEMA_VERSION`. Every schema has
`additionalProperties: false` and all existing registry validation applies.

A tool call is text the harness parses, exactly one per model turn:
`<tool_call>{"name": ..., "arguments": {...}}</tool_call>`. Text outside the block
is ignored. Zero or several blocks, bad JSON and unknown fields are rejected.
**Implemented:** `parse_model_call`.

## 5. Failure cases

| case | harness response |
|---|---|
| no or several tool calls, malformed JSON | error result; counts as an attempt |
| tool not in this mode's schema (other mode, app tool, engine tool, nonexistent) | `NotVisible` error, identical for all; counts as an attempt |
| arguments break the schema | `Arguments` error; counts as an attempt |
| illegal `submit_move` | error naming the move (echo bounded to 8 characters), pointing to `list_legal_moves`; retry |
| attempts spent (the match `max_retries`, at least one) | existing constrained-decoding fallback, which is recorded as an intervention, not as unaided play |
| read query after the eight-call budget | error result the model sees; not a move attempt |
| coach `Provisional:` line illegal | same gate; an earlier legal one stands (section 3); none legal: fallback |
| chat context over its bound | error before the prompt is built |

The fallback keeps contracts.md's rule: a masked result needs a separate unmasked
diagnostic, and assisted legality is never equated with model knowledge. The
retry count and fallback use are recorded per move. **Implemented:**
`SubmitMoveGate` and its wiring in `crates/ai/src/play.rs`:
`request_move_with_harness` with `MoveHarness::{Default, Play, Coach}`. `Default`
is `request_move` unchanged, so KnightLM and every existing provider behave as
before; `Play` and `Coach` are opt-in, return a `HarnessMove` (attempts, rejected,
`fallback`, read calls, provisional revisions, reply) and fall back to the same
grammar-constrained request that KnightLM uses. `play::select_harness` picks the harness by mode where RBC asks a model
for a move (`model_match_move`): a competitive game gets `Play`, a game with chat allowed gets `Coach`,
but only for a KnightLM seat and only when the app enables it (`RBC_TOOLS_HARNESS=1`), because the model
has not been trained on the tools format; every other seat or setting keeps `Default`.

## 6. Packet fields for repetition and the 50-move rule

The packet already carries, in `facts`: `repetition_count`, `threefold_claim_now`,
`fivefold_automatic`, `fifty_move_claim_now`, `seventy_five_move_automatic`, plus
`repetition_ledger` (readable four-field positions with counts). The halfmove
clock is the fifth FEN field of `fen`. So the current-state facts are present;
a test pins their presence.

Packet v2 (owner decision: a labelled experiment arm, "state plus consequences"):
a per-legal-move consequence map, for example
`legal_move_effects: {"f6g8": ["repeats:2"], "e7e5": ["resets_clock"]}`, giving the
repetition count the move would reach (only when it is 2 or more, and only when
the history is complete, like `repetition_count`) and whether it resets the
halfmove clock (pawn move or capture). Moves with no consequence are omitted. It
is behind `PacketOptions::legal_move_effects`, off by default; when on, the packet
`schema_version` is 2. With it off the packet bytes are exactly v1 (a golden test
pins them), so existing rows and evals are unaffected. **Implemented:**
`GameState::legal_move_effects`, checked against a replay of every legal move.
No corpus or training row uses v2; the arm is trained and scored against the
control only when someone starts it.

## 7. Test list

Implemented:

- `model/tests/test_prompt.py`: `user_text`/`user_chat` byte pins; play prompt
  never contains a coach tool name and the reverse; tool sets disjoint and
  engine-free; packet last and prefix identical across packets; chat bounds and
  overflow errors; schema file shape and exact embedding in the prefix.
- `crates/ai/src/model_turn.rs` tests: the coach commit rule (revision replaces,
  illegal later line keeps the earlier, none valid commits nothing, retries
  exhausted keep the last valid, a cut stream ignores the partial line); read
  executors against known positions and the shared budget (ninth call refused,
  bad requests charged); play completion parser cases; summarizer bounds and
  determinism; chat bounds; prefixes disjoint by mode and the combined prefix
  showing both; the three checked-in fixtures equal the Rust output.
- `crates/ai/tests/play_harness.rs`: default harness equals `request_move`;
  play reads then submits, retry budget equals `max_retries`, fallback after it,
  the other mode's tool refused; coach provisional rule end to end including
  time and token cut-offs, no reply after a fallback, oversized context refused
  before any model call; a coach prompt never carries a play tool.
- `crates/core/tests/packet_v2.rs`: v1 packet bytes pinned and free of
  `legal_move_effects`; v2 effects equal a replay of each legal move.
- `model/tests/test_prompt.py` (added classes): Python prefixes, combined prefix,
  example prompts and constants equal the Rust bytes; play completion parser and
  formatter agree with every shared case; the summarizer agrees with the shared
  cases and the rolling bound.
- `crates/ai/src/tools.rs` tests: model tool sets valid, disjoint, only
  read/board-write effects, no engine-like names; a call outside the visible
  schema is rejected in both directions (including app and engine tool names);
  call text parsing (one block only); illegal `submit_move` returns the error,
  retries, then falls back, and stays fallen back; the checked-in JSON equals the
  Rust output (this is the Rust/Python parity test; Python then reads the same file).
- Packet facts presence (repetition count, claim flags, halfmove clock in FEN):
  `crates/core/tests/contracts.rs` (existing) covers the facts; no change.

Also implemented: `crates/ai/src/coach.rs` unit tests (scratch isolation, all-or-nothing steps, typed
errors, marks as UI effects, the call budget); `play_harness.rs` coach end to end with a scripted provider
(provisional lines, tool calls, marks, committed move) and `select_harness` by mode;
`crates/ai/tests/coach_seam.rs` (a served coach completion run on the Rust executors);
`provisional_cases.json` (Rust generates, `model/serve/test_coach.py` reproduces); the serve chat turn
(`model/serve/test_coach.py`).

Spec only: nothing in this list remains; a tools-harness run against a real
model and training on the play completion format are not done.

## 8. Owner decisions

1. Coach move submission: the provisional rule of section 3 (`Provisional: <uci>`
   lines, last valid wins, fallback if none). A restricted `commit_move` tool in
   the coach schema is not used.
2. The `reasoning` field is trained on from the start in play mode, from
   rules-derived text; the exact completion format is section 2. The schema keeps
   it optional so a lenient reader can still accept an untrained model's call.
3. The retry budget equals `MatchConfig::max_retries` (at least one).
4. Read queries share the eight-call `RuleSession` budget.
5. Packet v2 with `legal_move_effects` is an experiment arm behind
   `PacketOptions`, off by default (section 6).
