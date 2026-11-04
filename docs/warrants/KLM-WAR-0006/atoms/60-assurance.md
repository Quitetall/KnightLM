---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e889-750b-9aea-e2b104ca6e3f
role: assurance
jurisdiction: authored
order: 60
classification: internal
---

# Assurance

## Acceptance Obligations

### OBL-001 — a full game completes through the UCI side
- **scope:** self-play versus the scripted driver using fixture openings
- **evidence:** driver transcript recording: handshake, every bestmove, clean quit;
game log ends with a real result
### OBL-002 — malformed/terminal UCI input is refused explicitly, not crashed
- **scope:** planted fuzz-ish driver sessions: unknown commands, nonsense go arguments,
go on a mated position, stop outside of go
- **evidence:** each case produces an explicit error line and the engine continues; the
terminal `go` case specifically refuses with a 'position is terminal' error
### OBL-003 — the random-legal player never emits an illegal move
- **scope:** 10,000 seeded games against the compiler's legal-move sets
- **evidence:** cargo test property run: zero illegal moves; each game's move rotations all
pass the compiler's legality set
### OBL-004 — the consumer plays a game without the author
- **scope:** an external UCI driver (evaluation lane) starts knightlm-uci and plays
- **evidence:** the session log from the evaluator lane; game completed unaided

## Gate Adequacy

Required at `basic`.

**Adversarial question:** Could a future model player quietly 'repair' an illegal move inside the binary and erase the trace?

- **outcome:** no — OBL-003 + RQ-010 failure semantics make any illegal internal move a surface event; the driver transcript is the evidence.


## Residual Risk

Real GUIs vary in UCI tolerance; the scripted-driver subset is the contract,
and one external driver session (OBL-004) is the real probe. Infinite `go`
handling is kept simple: finite sweep plus stop-wakeups.
