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

- [ ] Composer layout and keybinds (i-5366, after t-2883)
- [ ] Slash command palette (i-7851)
- [ ] @-mentions for boards/studies/games/players (i-bf7f)
- [ ] PGN/FEN/image attachments and context chips (i-d43a)
- [ ] Model/temperature controls; stop/regenerate (i-ca44)

## CHK-ai-composer · check — ai-composer
run: cd /mnt/4tb/development/tmp/opencode && node cdp_ai_window.js
on: done
evaluates i-5366, i-7851, i-bf7f, i-d43a, i-ca44
