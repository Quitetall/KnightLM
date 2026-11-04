---
format: openwarrant/1.0
profile: ticket
alias: t-6096
uuid: 01a1a54a-3137-76f2-8de5-d62bae8786e4
priority: 0
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Render messages, reasoning, tools, actions, retries and usage."
---
# Transcript, streaming, tool and retry cards

## Checklist

- [x] Message list and streaming content (i-fa9b, after t-d0bf, t-2883) — done by claude, 2026-11-04
- [x] Collapsible reasoning (i-00ae) — done by claude, 2026-11-04
- [x] Tool/board-action/retry cards (i-618a) — done by claude, 2026-11-04
- [x] Usage and context chips (i-502e) — done by claude, 2026-11-04
- [x] CDP: stream and stop (i-3275) — done by claude, 2026-11-04

## Verification

`war done --check` cannot run in this layout (`source-basis-unavailable` on the
`code` symlink). Checked by hand instead:

- `cargo check --target wasm32-unknown-unknown` passes.
- The cancel route was checked directly: with a slow provider, `POST
  /api/ai/sessions/{id}/cancel` returned `{"cancelled":true}` while a turn ran,
  the turn ended with `provider_cancelled`, and a second cancel returned
  `{"cancelled":false}`.
- `curl` against the stream after a turn returned its 10 events in order.
- `frontend/tests/cdp/cdp_ai_stream.js` now serves its own SSE provider and
  passes reproducibly (two consecutive runs): partial text ("the") renders
  mid-turn, the button reads Stop while the turn runs, Stop ends it with "the
  turn was cancelled", the button returns to Send, a tool card renders and the
  usage chip is present. `cdp_ai_rail.js` and `cdp_ai_pin.js` still pass.

The earlier "flakiness" was not the app: the dev server had never actually
restarted, because the old process still held port 8090, so every request went
to a server with no `MODEL_URL` and the turn failed before emitting anything.
Checked with `ss -ltnp` and a clean restart.
Covers i-fa9b, i-00ae, i-618a, i-502e, i-3275.
