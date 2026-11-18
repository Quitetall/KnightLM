---
format: openwarrant/1.0
profile: ticket
alias: t-4eb3
uuid: 01a1a54a-30ea-710d-80d5-ff9fc15961ba
priority: 0
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Add play/draw/annotate/note/create/copy tools with revision and mutability checks."
---
# Board write tools, revisions, lock/copy

## Checklist

- [x] play_move with SAN/UCI validation (i-c050, after t-4836, t-312c) — done by claude, 2026-11-04
- [x] draw_arrow/draw_circle/clear_shapes (i-2eb9) — done by claude, 2026-11-04
- [x] annotate/write_note (i-c027) — done by claude, 2026-11-04
- [x] create_board/copy_board with provenance (i-f322) — done by claude, 2026-11-04
- [x] Conflict and copy_required tests (i-93f1) — done by claude, 2026-11-04

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-ai --all-features write_tools
```

Result: 17 of 17 integration tests pass (write_tools binary) over a real in-memory repository. The --all-features flag is required because the board host lives behind the host cargo feature.
Covers i-c050, i-2eb9, i-c027, i-f322, i-93f1.
