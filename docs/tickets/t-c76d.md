---
format: openwarrant/1.0
profile: ticket
alias: t-c76d
uuid: 01a1a54a-3235-74c4-9518-5c7174543e28
priority: 2
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Export Markdown/JSON and reserve R2 fields."
---
# Diff export and forward-compatible Lua/progress fields

## Checklist

- [x] Markdown/JSON export (i-0911, after t-0ba4) — done by claude, 2026-11-04: to_markdown writes the comparison as Markdown (study fields, chapters, boards, moves, shapes, annotations, notes); GET /api/compare?format=markdown serves it; the Compare tab has an Export Markdown button.
- [x] Reserved Lua/progress diff fields (i-909f) — done by claude, 2026-11-04: DiffReport carries a reserved block (present, null) so R2 fills it without changing the shape; the progress overlay is already per user in every ChapterDiff.
- [x] Round-trip export tests (i-f502) — done by claude, 2026-11-04: the report derives Serialize AND Deserialize, and a test round-trips it through JSON and asserts the re-serialisation is byte-identical.
- [x] No mutation (i-0314) — done by claude, 2026-11-04: a test exports both ways and asserts neither study updated stamp nor version moved.

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-ai --all-features --test diff
```

Result: 12 diff tests pass, including the JSON round-trip, the Markdown export, and that exporting mutates nothing.
Covers i-0911, i-909f, i-f502, i-0314.
