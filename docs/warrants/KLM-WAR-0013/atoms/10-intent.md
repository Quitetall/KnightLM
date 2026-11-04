---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a580-74f0-be4c-a5ef4787ae41
role: intent
jurisdiction: authored
order: 10
classification: internal
---

# Intent

Build `rbc-context`, the pure compiler that produces the exact input an agent sees. The same
position, history, config and evidence must produce the same canonical bytes and SHA-256. The
compiler is reusable by chat, play, evaluation and future training; it must not know about
HTTP, SQLite, engines or the UI.

Out of scope: provider calls, tool execution, dataset splits/training loop, UI rendering.
