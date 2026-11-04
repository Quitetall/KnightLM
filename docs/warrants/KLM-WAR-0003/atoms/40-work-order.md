---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e85d-7117-9c40-3c09a1e2f79d
role: work_order
jurisdiction: authored
order: 40
classification: internal
---

# Work Order

## Deliverables

1. `knightlm-notation::pgn` full parser and writer:
   - tag section: the Seven-Tag Roster plus supplemental tags, `SetUp`/`FEN`
     for non-standard starts;
   - movetext: comments (braces and `;` line comments), NAGs ($n), recursive
     variations, null moves, results — all preserved.
2. `AnnotatedGame`: tags + movetext tree with comments, NAGs, variations and
   the final result; conversion to `GameRecord` for play-through.
3. Writer emitting the same fidelity back out, re-parse identical.
4. CLI: `parse --annotations` over the shared fixture corpus, including
   hand-authored pathological cases (nested variations, comments containing
   braces, variation inside variation, NAG storms).

## Seams and who depends on them

1. **SEAM `nota-annot`** — `knightlm_notation::pgn::parse_annotated(&[u8]) ->
   Result<AnnotatedGame, ParseError>`. Contract: annotations preserved or a
   position-and-token error. Consumers: KLM-WAR-0010 (viewer renders
   annotations), KLM-WAR-0004 (adds formats beside it).
2. Unfrozen promise made and kept: the `nota-reader`, `nota-detect` and
   `GameRecord` seams from KLM-WAR-0002 do not change here.

## Owner's space

The performer owns the movetext AST, memory and traversal design, error
taxonomy inside the contract, writer pretty-printing policy, test structure.
Anything that would force a consumer to change code is a seam amendment, not
an interior choice.

## Premade instructions

- Round-trip is the standard: parse -> write -> parse idempotent over the
  battery, state and annotations identical.
- Malformed input never succeeds quietly (RQ-004).

## Autonomy and escalation

Tier T2. Seam-level contract changes escalate to the sponsor and consumers.

## Rollback

Delete `code/crates/notation/src/pgn/`; the thin path from W-0002 is
untouched.
