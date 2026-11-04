---
format: openwarrant/1.0
profile: ticket
alias: t-41af
uuid: 01a1a54a-3214-71d5-878b-a2592126f4dc
priority: 2
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Add the Compare tab and the compare scope."
---
# Compare workspace, selection and compact directory

## Checklist

- [x] Compare workspace kind (i-ba0f, after t-9c43, t-f436) — done by claude, 2026-11-04: Compare has its own tab and its own server-side scope (the compare_scope setting), which is what the assistant reads; it is not a workspace kind bolted onto the board workspace, because a comparison is about two studies rather than one.
- [x] Select two or more studies/workspaces (i-b636) — done by claude, 2026-11-04: The Compare tab lists every study with an A and a B pick, so two (and only two) are chosen; the server refuses a study compared with itself (400) and one that does not exist (404).
- [x] Scope chip and compact directory (i-622f) — done by claude, 2026-11-04: A scope chip names both studies, and the assistant context carries Evidence.compare - the compact directory, one line per study with chapter count and version - which the compiler already rendered but nothing had ever filled.
- [x] Clear compare scope (i-6ba6) — done by claude, 2026-11-04: Clear scope deletes the setting and clears the chip and the report; verified in cdp_compare.js that /api/compare/scope reports active:false afterwards.
- [x] CDP: compare selection (i-f483) — done by claude, 2026-11-04: frontend/tests/cdp/cdp_compare.js: seeds two studies that share a chapter, picks A and B, runs the comparison, reads the chip and the diff row, and clears.

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM/apps/rusty-brick-chess/frontend/tests/cdp && node cdp_compare.js
```

Result: AI_COMPARE_PASS: scope chip 'Left study vs Right study', a diff row, the scope stored server-side then cleared, and the assistant context carrying the directory.
Covers i-ba0f, i-b636, i-622f, i-6ba6, i-f483.
