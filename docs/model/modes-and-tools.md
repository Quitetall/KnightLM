# Modes and tools contract

Status: proposed (ADR `KLM-ADR-modes-and-tools`, not signed). Extends
`docs/model/plan.md` and `docs/model/contracts.md`; where this file is silent,
those govern. The implemented parts are marked **Implemented**; the rest is
**Spec only**.

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
are not used to build any current training row.

## 2. Play mode (competition, chat off)

The model sees the play instruction, the play tool schema and the packet, and
nothing else. It writes a short reasoning field (at most 400 characters) and then
calls `submit_move` with the key order `reasoning`, `move`. The harness shows only
that reasoning and the move. No chat context, no coach tools.

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
check and the `submit_move` retry gate. **Spec only:** the executors that answer
the read queries (they belong on `knightlm_core::RuleSession`, which already
simulates one ply on a clone and charges every call against its budget; the
eight-call fixture budget applies unless a later arm changes it).

## 3. Coach mode (chat on)

Same fixed prefix structure, with the coach instruction and coach schema, then
bounded chat context, then the packet.

Chat context is a rolling summary (at most 1200 characters), or the last
6 messages (each at most 800 characters), or both. Older messages are dropped;
a summary or message over its bound is an error, not silent truncation. The
summarizer is a separate step outside the model and is spec only.

The model plays first, then replies: its output begins with the line
`Move: <uci>`, followed by the reply. The harness validates and plays that move
through the same legality gate as section 5, then shows the reply. Coach has no
`submit_move`, so a coach tool can never submit a real move.

Coach tools (scratch board and presentation only):

| tool | effect | purpose |
|---|---|---|
| `scratch_set` {fen} | read | set a scratch position; the real game is untouched |
| `scratch_step` {moves[1..8]} | read | play moves on the scratch board |
| `show_game_move` {ply} | read | point at a ply of the game |
| `mark_arrow` {from, to, color?} | board-write | draw an arrow |
| `mark_square` {square, color?} | board-write | highlight a square |
| `clear_marks` {} | board-write | remove the model's marks |

A long chat shows both schemas: the harness may present coach and play schemas in
one prompt for a long chat, but each call is still checked against the schema of
the mode the harness assigned to that turn. The combined prefix is its own fixed
prefix (not built yet; spec only) and keeps the same ordering.

## 4. Tool schemas

Defined once in `crates/ai/src/tools.rs` (`model_tool_specs`). The registry is
separate from the app registry, so the model cannot name `analyze`, `evaluate`,
`play_move` or any other app tool. `crates/ai/schemas/model_tools.json` is
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
| attempts spent (default: the match `max_retries`, at least one) | existing constrained-decoding fallback, which is recorded as an intervention, not as unaided play |
| coach `Move:` line missing or illegal | same gate and fallback as play |
| chat context over its bound | error before the prompt is built |

The fallback keeps contracts.md's rule: a masked result needs a separate unmasked
diagnostic, and assisted legality is never equated with model knowledge. The
retry count and fallback use are recorded per move. **Implemented:**
`SubmitMoveGate`. **Spec only:** wiring it into `crates/ai/src/play.rs` (current
KnightLM play still uses the grammar-constrained single call, unchanged).

## 6. Packet fields for repetition and the 50-move rule

The packet already carries, in `facts`: `repetition_count`, `threefold_claim_now`,
`fivefold_automatic`, `fifty_move_claim_now`, `seventy_five_move_automatic`, plus
`repetition_ledger` (readable four-field positions with counts). The halfmove
clock is the fifth FEN field of `fen`. So the current-state facts are present;
a test pins their presence.

Missing, and proposed here rather than changed: a per-legal-move consequence
flag, for example `legal_move_effects: {"g1f3": ["repeats:2"], "e2e4": ["resets_clock"]}`
giving the repetition count the move would reach and whether it resets the
halfmove clock (pawn move or capture). Adding it changes the packet shape, which
changes schema_version 1 and every row built from it. Proposal: a packet
`schema_version` 2 behind a `PacketOptions` field (off by default, so v1 rows and
evals are unaffected), introduced only as a labelled experiment arm
("state plus consequences"), trained and scored against the control.
**Not implemented.**

## 7. Test list

Implemented:

- `model/tests/test_prompt.py`: `user_text`/`user_chat` byte pins; play prompt
  never contains a coach tool name and the reverse; tool sets disjoint and
  engine-free; packet last and prefix identical across packets; chat bounds and
  overflow errors; schema file shape and exact embedding in the prefix.
- `crates/ai/src/tools.rs` tests: model tool sets valid, disjoint, only
  read/board-write effects, no engine-like names; a call outside the visible
  schema is rejected in both directions (including app and engine tool names);
  call text parsing (one block only); illegal `submit_move` returns the error,
  retries, then falls back, and stays fallen back; the checked-in JSON equals the
  Rust output (this is the Rust/Python parity test; Python then reads the same file).
- Packet facts presence (repetition count, claim flags, halfmove clock in FEN):
  `crates/core/tests/contracts.rs` (existing) covers the facts; no change.

Spec only (to write with the executors):

- executor answers match `shakmaty` for attackers, defenders, check and what-if;
  every query consumes `RuleSession` budget; a result carries no score field;
- a coach tool can never reach a real-move write;
- coach `Move:` line goes through the gate; reply is withheld until it is legal;
- long-chat combined prefix is stable and contains both schemas;
- `legal_move_effects` agrees with a replay of each legal move (when the arm exists).

## 8. Open questions

1. Coach move submission: this draft has the coach write `Move: <uci>` on its
   first line because the coach schema has no submit tool. Alternative: a
   restricted `commit_move` in the coach schema. Owner decision.
2. Whether the `reasoning` field is trained on (needs synthetic teacher data) or
   only permitted; it is optional in the schema for that reason.
3. Retry budget default and whether it equals `MatchConfig::max_retries`.
4. Whether `what_if` and the queries share one `RuleSession` budget with the
   eight-call fixture limit.
5. Packet v2 with `legal_move_effects`, as an arm (section 6).
