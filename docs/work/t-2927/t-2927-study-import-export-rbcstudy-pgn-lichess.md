---
format: openwarrant/1.0
profile: ticket
alias: t-2927
uuid: 01a1a54a-31eb-7305-8b3a-d39b8710f340
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Portable bundles and compatibility adapters."
---
# Study import/export: .rbcstudy, PGN, Lichess

## Checklist

- [x] .rbcstudy schema and serializer (i-3498, after t-5638, t-376e) — done by claude, 2026-11-04: crates/ai/src/study_bundle.rs writes the rbc-study/1 document the design fixes (schema, title, description, tags, lineage, chapters with boards, lua: null).
- [x] PGN export per board (i-6bca) — done by claude, 2026-11-04: board_pgn emits one game per board with the study and chapter in its headers, a FEN header when the position is not the start, and the movetext.
- [x] Lichess PGN/URL import adapter (i-256c) — done by claude, 2026-11-04: POST /api/studies/import-pgn: a Lichess study export is PGN with numbered chapters, so one game becomes one chapter with its Event as the title and its movetext as the board. Verified with a two-game PGN -> two chapters.
- [x] Unknown-field preservation (i-c1c9) — done by claude, 2026-11-04: Unknown top-level, chapter and board fields are kept in Bundle.other and written back by to_json; a test proves a future_field survives a parse/write cycle.
- [x] Round-trip tests (i-e7f2) — done by claude, 2026-11-04: 6 tests in crates/ai/tests/study_bundle.rs: lossless round trip, unknown-field survival, refusal of a non-bundle, lineage in the bundle, and a full import into a different database.

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-ai --all-features study_bundle
```

Result: 6 bundle tests pass; verified live that GET /bundle downloads with content-disposition, POST /import reports chapters/boards, and a two-game PGN becomes two chapters with their moves.
Covers i-3498, i-6bca, i-256c, i-c1c9, i-e7f2.
