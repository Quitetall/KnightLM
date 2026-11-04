---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-1bea-73e5-af55-6182429637ab
role: assurance
jurisdiction: authored
order: 60
classification: internal
---

# Assurance

## Acceptance Obligations

### OBL-001 — SAN writer and parser round-trip the fixture battery
- **scope:** every SAN fixture in `code/crates/notation/tests/`: disambiguation, captures,
promotions including under-promotions, castling, check and mate suffixes
- **evidence:** `cargo test -p knightlm-notation` round-trip tests green on the whole battery
### OBL-002 — the parser refuses a SAN that names no legal move
- **scope:** the same fixture battery, planted with illegal and ambiguous SAN tokens
- **evidence:** planted token exercises produce the specific refusal error naming position and
token; no fallback guess, no silent repair (KLM-SAS-RQ-004)
### OBL-003 — the thin reader yields provenance-carrying records or explicit failure
- **scope:** one real Lichess month-file excerpt ingested end to end; a planted malformed movetext game
- **evidence:** records carry source `lichess-bulk/<month>` and policy-correct
`history_complete`; the planted game produces a position-and-token error
### OBL-004 — the consumer ingests without the author
- **scope:** KLM-WAR-0007's ingest stage run by MLOps, not by this Warrant's author
- **gate:** gate://software.repo.war-check@1.0.0
- **evidence:** recorded cross-lane consumer check: month file to shard JSONL + manifest via
the CLI, unaided

## Gate Adequacy

Required at `basic`.

**Adversarial question:** Can this Warrant be satisfied while a four-field-FEN source is marked `history_complete: true`?

- **outcome:** no — OBL-003 states the policy and the reader tests include such a source.

**Adversarial question:** Can the corpus start even though this Warrant's reader skips annotations?

- **outcome:** no counterexample — the thin reader is exactly the path that allows it; fidelity belongs to KLM-WAR-0003.


## Residual Risk

Real-world Lichess movetext may deviate from spec in ways the thin reader
must refuse rather than repair; the refusal rate on a real excerpt is the
signal to watch. Zst streaming memory is bounded but a hostile month file
could stress it; fixtures include one large excerpt.
