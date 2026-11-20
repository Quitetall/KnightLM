---
format: openwarrant/1.0
profile: ticket
alias: t-e777
uuid: 01a1a54a-314a-717d-a559-68e4bc741ab3
priority: 0
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Show exactly what the model saw and replay it."
---
# Context inspector, pin/live and replay diff

## Checklist

- [x] Section list with tokens/truncation (i-6971, after t-d0bf, t-2883) — done by claude, 2026-11-04
- [x] Canonical prompt and hash (i-d4fb) — done by claude, 2026-11-04
- [x] Pin/live toggle and board-changed warning (i-c1ce) — done by claude, 2026-11-04
- [x] Replay match/diff (i-553a) — done by claude, 2026-11-04
- [x] CDP: context inspector (i-90a9) — done by claude, 2026-11-04

## Verification

`war done --check` cannot run in this layout (`source-basis-unavailable` on the
`code` symlink). Checked by hand:

- `cargo check --target wasm32-unknown-unknown` passes.
- `frontend/tests/cdp/cdp_ai_context.js` (self-contained SSE provider) ->
  `AI_CONTEXT_PASS`: the panel opens with 4 sections, the header reads
  `<sha8> · 227 tokens · 4 sections`, the canonical prompt renders, and Replay
  reports `replay · identical`.
- The same script checks the Sent/Live toggle and the board-changed warning:
  live rebuilt `c22df219` (226 tokens) against sent `59b2c451` (227 tokens) and
  the `.context-drift` line appeared exactly when the hashes differed. With the
  same position the two agree (`POST /api/ai/context/compile` returns the same
  `59b2c451` the stored turn recorded), so the warning is not a false positive:
  in that run the app's position had changed after the send while the session
  restored.
Covers i-6971, i-d4fb, i-c1ce, i-553a, i-90a9.
