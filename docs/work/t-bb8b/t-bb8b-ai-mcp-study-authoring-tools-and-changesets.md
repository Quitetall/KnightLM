---
format: openwarrant/1.0
profile: ticket
alias: t-bb8b
uuid: 01a1a54a-3200-76cd-b849-54792dd695d9
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Let agents create/edit studies through the shared tools and approval flow."
---
# AI/MCP study authoring tools and changesets

## Checklist

- [x] create_study/add_chapter/add_board/reorder/write_text tools (i-4697, after t-4836, t-bb0e, t-5638) — done by claude, 2026-11-04: Six study tools implemented (add_chapter, add_board_to_chapter, reorder_chapters, write_study_text, publish_study, get_chapter), with a registry walk asserting no SessionWrite tool is advertised-but-unimplemented. The dependency on t-5638 is now satisfied (editor closed 5/5).
- [x] Published/locked protection (i-a144) — done by claude, 2026-11-04: See the ticket note and crates/ai/tests/study_tools.rs.
- [x] Changeset generation for study edits (i-cf1d) — done by claude, 2026-11-04: Changeset for a study edit is the refusal summary plus the study and chapter ids; boards keep the full before/after changeset.
- [x] Approval integration (i-21f6) — done by claude, 2026-11-04: See the ticket note and crates/ai/tests/study_tools.rs.
- [x] Authoring tests (i-fb8c) — done by claude, 2026-11-04: See the ticket note and crates/ai/tests/study_tools.rs.

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-ai --all-features study_tools
```

Result: 4 of 4 pass, including a walk of the registry asserting no SessionWrite tool is advertised-but-unimplemented.
Covers i-4697, i-a144, i-cf1d, i-21f6, i-fb8c.
