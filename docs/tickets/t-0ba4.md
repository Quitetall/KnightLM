---
format: openwarrant/1.0
profile: ticket
alias: t-0ba4
uuid: 01a1a54a-321f-73a3-a873-02692fb44f7e
priority: 2
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Implement deterministic alignment and change detection."
---
# Structural diff: chapters, boards, trees, notes, arrows

## Checklist

- [x] Chapter/board alignment (i-f614, after t-376e, t-41af) — done by claude, 2026-11-04: align_chapters matches by id, then ordinal, then title; align_boards by id, then title, then normalized FEN (without move counters). Both tested, including that two chapters at the same ordinal align with the rename reported and that a chapter only on one side is Removed. The dependency on t-41af is now satisfied.
- [x] Move-tree diff (i-0b0d) — done by claude, 2026-11-04: Move trees diff by common prefix: first_divergence is the ply, moves_removed/added the tails. Tested with e4 e5 against e4 e5 Nf3 -> divergence at ply 2, Nf3 added.
- [x] Annotation/shape/note/metadata diff (i-5585) — done by claude, 2026-11-04: Arrows and circles diff as sets (arrow g1-f3 / circle square), annotations as ply: comment lines, board and chapter notes by line, and metadata as field changes (title, description, lifecycle, tags, start position). Tested in crates/ai/tests/diff.rs.
- [x] Progress overlay (i-ceb0) — done by claude, 2026-11-04: The progress overlay is per user and optional: each ChapterDiff carries left/right completed and score when a user is given.
- [x] Golden diff fixtures (i-a9df) — done by claude, 2026-11-04: crates/ai/tests/diff.rs is the fixture set: identical studies, a changed board, one-sided chapters, line diffs, FEN normalization and a test that comparing writes nothing.

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-ai --all-features --test diff
```

Result: 8 diff tests pass; GET /api/compare returns a report for two real studies and an error (not a panic) for unknown ids.
Covers i-f614, i-0b0d, i-5585, i-ceb0, i-a9df.
