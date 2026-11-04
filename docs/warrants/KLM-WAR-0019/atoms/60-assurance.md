---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a609-71e2-b292-3cbf99b4cb8d
role: assurance
jurisdiction: authored
order: 60
classification: internal
---

# Assurance



## Gate Adequacy

Required at `basic`. **Adversarial question:** how could this look done and not be?
Each obligation pairs a positive claim with a refusal: a stale revision, a locked board, a
missing hash, an unapproved write, or a dropped required section. The package test in
`openwarrant/packages.json` is the executable gate; the obligation text is the contract.

## Obligations

### OBL-001: comparing two studies never mutates either

- **scope:** the diff engine over the repository.
- **evidence:** `comparing_writes_nothing` in `crates/ai/tests/diff.rs` asserts neither updated stamp nor version moved.
- **gate:** gate://software.repo.war-check@1.0.0

### OBL-002: alignment follows id, then ordinal, then title, and a FEN matches without its counters

- **scope:** chapters and boards aligned by the documented rules.
- **evidence:** `a_chapter_that_kept_its_title_lines_up_and_reports_its_board_change` and `a_fen_is_the_same_position_without_its_counters` in `crates/ai/tests/diff.rs`.
- **gate:** gate://software.repo.war-check@1.0.0

### OBL-003: two boards follow one ply in the browser

- **scope:** the Compare tab. 
- **evidence:** `cdp_compare.js` (`AI_COMPARE_PASS`) moves the ply and asserts both boards render.
- **gate:** gate://software.repo.war-check@1.0.0
## Residual Risk

See `90-risks.md` in the planning bundle. Residual risk is accepted by the program owner
until the release gate records otherwise.
