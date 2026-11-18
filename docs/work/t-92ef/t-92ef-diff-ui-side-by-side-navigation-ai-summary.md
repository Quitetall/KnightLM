---
format: openwarrant/1.0
profile: ticket
alias: t-92ef
uuid: 01a1a54a-322b-770d-9347-7768b9c54e18
priority: 2
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Render the compare view and read-only AI commentary."
---
# Diff UI: side-by-side, navigation, AI summary

## Checklist

- [x] Side-by-side boards and synchronized ply (i-fc06, after t-41af, t-0ba4) — done by claude, 2026-11-04: The Compare tab renders two Board components from the diff (left_pgn/right_pgn) with one ply control moving both; verified 3 of 3 -> back -> 2 of 3.
- [x] Change list with filters (i-28de) — done by claude, 2026-11-04: A changed/all filter over the chapter rows; the render closure tracks the filter signal (it read get_untracked before, so the list never re-rendered).
- [x] Click-to-focus (i-feb7) — done by claude, 2026-11-04: Clicking a change row focuses it and fills both boards, jumping to the first divergence ply.
- [x] Progress badges (i-7046) — done by claude, 2026-11-04: A done badge on a chapter the reader has completed; cdp_compare marks a chapter done and asserts one .progress-badge appears.
- [x] AI summary/chat (i-27b4) — done by claude, 2026-11-04: An Ask the assistant button writes a question naming both studies into the assistant composer (the draft moved into AppCtx) and switches to its tab; the assistant already carries the compare directory in its context. Verified in cdp_compare: the tab switched and the composer holds the question.

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM/apps/rusty-brick-chess/frontend/tests/cdp && node cdp_compare.js
```

Result: AI_COMPARE_PASS: two boards side by side on one ply (3 of 3 -> back -> 2 of 3), filters, focus, a progress badge on a completed chapter, and Markdown export.
Covers i-fc06, i-28de, i-feb7, i-7046, i-27b4.
