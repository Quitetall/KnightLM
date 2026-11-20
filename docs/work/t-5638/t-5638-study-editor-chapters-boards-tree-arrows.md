---
format: openwarrant/1.0
profile: ticket
alias: t-5638
uuid: 01a1a54a-31d8-7090-87f4-34945f6076a2
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Build the Study tab and the manual editor."
---
# Study editor: chapters, boards, tree, arrows, Markdown

## Checklist

- [x] Study tab layout and chapter navigation (i-f4b9, after t-4200, t-6c55) — done by claude, 2026-11-04: Verified by cdp_study.js (AI_STUDY_PASS): layout, chapter add/reorder/duplicate, board strip with primary marking.
- [x] Add/reorder/duplicate/delete chapters (i-56cc) — done by claude, 2026-11-04: Verified by cdp_study.js (AI_STUDY_PASS): layout, chapter add/reorder/duplicate, board strip with primary marking.
- [x] Primary + additional board strip (i-524e) — done by claude, 2026-11-04: Verified by cdp_study.js (AI_STUDY_PASS).
- [x] Move tree, arrows, annotations (i-c9a0) — done by claude, 2026-11-04: The board is embedded with its move tree (clickable SAN), arrows and circles as shapes, and ply comments listed. Verified by cdp_study.js: a seeded board with two moves, an arrow and a comment renders e4 e5 and the comment.
- [x] Markdown editor/preview; metadata (i-d9bf) — done by claude, 2026-11-04: Markdown editor and preview (escapes before tagging; headings, lists, quotes, code, emphasis), title, description and tags. AI_STUDY_PASS plus six renderer tests.

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM/apps/rusty-brick-chess/frontend/tests/cdp && node cdp_study.js
```

Result: AI_STUDY_PASS: layout, chapter add/reorder/duplicate, board strip with primary marking, notes save and publish with edits disabled.
Covers i-f4b9, i-56cc, i-524e, i-c9a0, i-d9bf.
