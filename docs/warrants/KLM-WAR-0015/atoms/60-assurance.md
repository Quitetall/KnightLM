---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a5ae-75f9-b5f0-2bd8ee7eb34c
role: assurance
jurisdiction: authored
order: 60
classification: internal
---

# Assurance



## Gate Adequacy

Required at `basic`. **Adversarial question:** how could this look done and not be?
Each obligation pairs a positive claim with a refusal: a stale revision, a locked board, a
missing hash, an unapproved write, or a dropped required section. The package test in
`openwarrant/packages.json` is the executable gate; the obligation text is the contract.


## Obligations

### OBL-001: a turn streams into the transcript as it happens, and can be stopped

- **scope:** the Assistant view consuming the session SSE stream, and the Stop
  control. It says nothing about a slow provider beyond the cancellation message.
- **evidence:** `frontend/tests/cdp/cdp_ai_stream.js` (`AI_STREAM_PASS`): a partial
  delta is visible while the turn runs, the button reads Stop, and after stopping the
  turn reports "the turn was cancelled".

### OBL-002: the session rail lists, renames, archives and pins conversations

- **scope:** those rail actions against stored sessions.
- **evidence:** `cdp_ai_rail.js` (`AI_RAIL_PASS`), `cdp_ai_rail2.js`, `cdp_ai_pin.js`
  (`AI_PIN_PASS`).

### OBL-003: the context inspector shows what was sent, and says when the board moved

- **scope:** the panel's section list, canonical prompt, hash, replay diff, and the
  Sent/Live toggle. It does not claim the live compile equals the sent one — that is
  exactly what the warning reports.
- **evidence:** `cdp_ai_context.js` (`AI_CONTEXT_PASS`): 4 sections, the sha/token
  header, the canonical prompt, `replay · identical`, and the drift line appearing
  only when the live hash differs from the sent one.

### OBL-004: the board being written to is visible even when it is not on screen

- **scope:** the strip marker, the off-screen badge and its click-to-focus.
- **evidence:** `cdp_cross_board.js` (`AI_CROSS_BOARD_PASS`).

### OBL-005: a conversation outlives a cleared browser

- **scope:** restore after `localStorage.clear()`, and the one-time reset notice.
- **evidence:** `cdp_session.js` (`AI_SESSION_PASS`).

## Residual Risk

See `90-risks.md` in the planning bundle. Residual risk is accepted by the program owner
until the release gate records otherwise.
