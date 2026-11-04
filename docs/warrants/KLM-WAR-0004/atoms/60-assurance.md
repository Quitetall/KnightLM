---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e86b-77b5-ba65-8451473b13b3
role: assurance
jurisdiction: authored
order: 60
classification: internal
---

# Assurance

## Acceptance Obligations

### OBL-001 — every included format round-trips on its fixtures
- **scope:** the per-format fixture batteries added by this Warrant
- **evidence:** `cargo test -p knightlm-notation`: parse/emit identical canonical state
(KLM-SAS-RQ-005)
### OBL-002 — malformed non-FEN formats are refused with specific errors
- **scope:** planted bad ICCF digits, impossible descriptive castling, EPD opcodes, off-roster FEN pieces
- **evidence:** each plant is named and refused on call; zero silent repairs
### OBL-003 — FRC/DFRC first-class in both UCI flavors
- **scope:** Scharnagl 960 starts; X-FEN K-set vs Shredder-file castling encodings; UCI
e1g1 and e1h1 games; perft spot-checks on FRC positions; four-field-FEN source
games never history_complete (RQ-006 not regressed)
- **evidence:** castling-flavor translation table tests + perft outputs recorded in the repo;
FRC corpus games replay to identical states
### OBL-004 — the differential battery shows zero unexplained disagreements
- **scope:** the whole fixture corpus, both directions, against pinned python-chess
- **gate:** gate://software.repo.war-check@1.0.0
- **evidence:** parity report: every disagreement is either fixed or carries a disposition
(known probes: four-field FEN completeness, castling flavor assumptions)
### OBL-005 — the consumer exports FRC and legacy formats unaided
- **scope:** the viewer lane (KLM-WAR-0010) and corpus lane (KLM-WAR-0007) exercise the amended seams
- **evidence:** recorded cross-lane checks: an FRC game exported to Shredder-FEN; corpus
shards re-read with old and new records mixed

## Gate Adequacy

Required at `basic`.

**Adversarial question:** Can the corpus lane be broken by the additive variant field while round-trips stay green?

- **outcome:** no — OBL-005 has both lanes re-read old and new shards mixed together.

**Adversarial question:** Could FRC legality be wrong while all FEN strings round-trip?

- **outcome:** no — OBL-003 includes perft spot-checks, which count nodes, not just strings.


## Residual Risk

Descriptive and ICCF nuances (promotion on castling contexts, en-passant edge
case row labels) are the likeliest quiet mismatch sources; the python-chess
battery is the witness. The GameRecord amendment is additive by design so
consumer risk is zero.
