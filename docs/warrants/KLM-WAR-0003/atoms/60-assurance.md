---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e85d-7117-9c40-3c09a1e2f79d
role: assurance
jurisdiction: authored
order: 60
classification: internal
---

# Assurance

## Acceptance Obligations

### OBL-001 — annotated round-trip preserves everything
- **scope:** the shared fixture battery: seven-tag games, comments, NAGs, nested
variations, brace-in-comment pathologies, SetUp/FEN starts
- **evidence:** `cargo test -p knightlm-notation` round-trip tests: parse, write, re-parse
idempotent; the replayed state identical at every node (KLM-SAS-RQ-005)
### OBL-002 — malformed annotation input is refused, not repaired
- **scope:** planted unterminated comments, unbalanced variation parentheses, stray tokens
- **evidence:** each plant produces an error naming position and token; none is silently dropped or repaired (KLM-SAS-RQ-004)
### OBL-003 — AnnotatedGame survives conversion to GameRecord
- **scope:** every fixture's `GameRecord` conversion replays the same final state
- **evidence:** replay tests assert identical terminal state between annotated and converted paths
### OBL-004 — the consumer renders without the author
- **scope:** the viewer lane (KLM-WAR-0010) loads one annotated fixture through `nota-annot`
- **gate:** gate://software.repo.war-check@1.0.0
- **evidence:** recorded cross-lane consumer check in KLM-WAR-0010's integration stage

## Gate Adequacy

Required at `basic`.

**Adversarial question:** Can annotations be lost in the GameRecord conversion while the pure round-trip tests stay green?

- **outcome:** no — OBL-003 states the conversion replay explicitly.

**Adversarial question:** Could this Warrant be resolved with a parser that accepts the happy path only?

- **outcome:** no — the battery includes planted malformations that must fail loudly.


## Residual Risk

Pathological real-world PGNs (unterminated iterations, wildly nested
variations) can diverge from the written standard; the battery targets the
pathologies found in Lichess-style exports and hand-crafted cases. Depth
limits on recursion are an interior choice the performer documents.
