---
format: openwarrant/1.0
profile: ticket
alias: t-8021
uuid: 01a1b88a-dc02-76fc-8550-9043a069505f
priority: 2
created_at: "2026-11-08T00:24:30Z"
created_by: "codex-contributor-setup"
summary: "Observe whether a first-time player can select KnightLM, start a game, and understand the board feedback."
labels:
  - "qa-engineer"
---
# Playtester — Try the first KnightLM game in RBC

**Time and setup.** About 30–45 minutes. Use a local build with KnightLM
available, or ask a maintainer for a short guided session. You can do this
without writing code or downloading model weights if a session is available.

Open **Play** without coaching beyond the [running guide](../app/running.md).
Select KnightLM, start a game, play three turns, then toggle **Longer thinking**
and play another turn. Try both dragging a piece and clicking its source and
destination. Note any unclear labels, unexpected waits, or moments when you
cannot tell whose turn it is. If the model is unavailable, record where the
flow stopped and the message you saw.

**Review.** Share the build, OS, browser, steps, and what happened. One
reproducible observation is enough to review. A clean run is useful too.
Use a throwaway game and crop personal information from any screenshot.

## Checklist


- [ ] Try the Play flow and note where it was clear or confusing. (i-a43d)
- [ ] Share a short report with build identity, steps, and the result or blocker. (i-855a)
