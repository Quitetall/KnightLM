---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e889-750b-9aea-e2b104ca6e3f
role: work_order
jurisdiction: authored
order: 40
classification: internal
---

# Work Order

## Deliverables

1. `code/crates/cli` gains (or a new crate provides) the `knightlm-uci`
   binary: full UCI handshake over stdio — `uci` (id reply incl.
   `knightlm`/author), `isready`, `ucinewgame`, `position startpos ... |
   fen ...`, `go depth|nodes|movetime` (finite kinds implemented; `go
   infinite` interrupted by `stop`), `bestmove` out, `stop`, `quit`.
2. `Player` trait: `select(state) -> Result<Move, PlayerError>`, implemented
   by `RandomLegal` (uniform over compiler-verified legal moves). This is the
   seam the model player plugs into in KLM-WAR-0009.
3. Terminal/illegal semantics: `go` on a terminal position is refused with an
   explicit error line, never a spin; any internal illegal move (from a
   future player) is a reported failure — logged and surfaced — never
   silently replaced (RQ-010).
4. A scripted-driver test harness playing both colors into game-end or check
   or a move-count stop over fixture openings.

## Seams and who depends on them

1. **SEAM `player`** — the trait and its RandomLegal implementation.
   Consumers: KLM-WAR-0009 (serving, the model-served side),
   KLM-WAR-0008 (game-play harness races any two Players).
2. **Contract `uci-wire`** — per the UCI subset above, one newline-frame
   grammar; consumers: any GUI (Arena, Banksia) and the sponsor's own demos.

## Owner's space

Timers, internal state machine, logging format and destination, engine-id
strings, and any reordering of the wire handlers that preserves the subset
and complaint order. Extending the response vocabulary for future options
inside the subset is interior work; adding dependency-free options is not a
seam change until a consumer wants them.

## Premade instructions

- Every game the driver plays must be compiler-checked: a property test
  asserts the RandomLegal player never emits an illegal move.
- The UCI binary must degrade gracefully: malformed input yields an error
  output line and continues; no crash, no silent discard.

## Autonomy and escalation

Tier T2. Escalation is for adding protocol features a consumer wants (that
amends the wire seam).

## Rollback

Remove the UCI binary and the Player module from the CLI crate; crates/core
is untouched.
