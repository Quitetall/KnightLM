---
format: openwarrant/1.0
profile: ticket
alias: t-e4fa
uuid: 01a1a54a-3140-7530-90d6-9b6748ef6572
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Slash commands, @-mentions, attachments, context chips, model picker, stop/regenerate."
---
# Composer bundle

## Checklist

- [x] Composer layout and keybinds (i-5366, after t-2883) — done by claude, 2026-11-06 [observed]: composer layout with Enter to send, Shift+Enter for a newline, Escape to close a palette and arrows to move in it.
- [x] Slash command palette (i-7851) — done by claude, 2026-11-06 [observed]: Typing / filters the five shipped prompts; Enter or a click inserts the prompt text and closes the palette.
- [x] @-mentions for boards/studies/games/players (i-bf7f) — done by claude, 2026-11-06 [observed]: Typing @ filters boards and studies loaded from the API; picking one adds a chip and strips the half-typed mention from the draft.
- [x] PGN/FEN/image attachments and context chips (i-d43a) — done by claude, 2026-11-06 [observed]: A FEN typed into the composer (or the position on screen) and a pasted PGN become chips carried in a Context block under the message; an image goes through /api/vision/recognize into a position chip.
- [x] Model/temperature controls; stop/regenerate (i-ca44) — done by claude, 2026-11-06 [observed]: Temperature slider and per-turn model override reach the provider (SendAiTurn gained temperature and model; temperature rounded so 0.9 sends as 0.9); stop cancels the turn; regenerate re-asks the last user message.

## CHK-ai-composer · check — ai-composer
run: cd /mnt/4tb/KnightLM/apps/rusty-brick-chess/frontend/tests/cdp && node cdp_composer.js
on: done
evaluates i-5366, i-7851, i-bf7f, i-d43a, i-ca44
