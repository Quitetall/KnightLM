---
format: openwarrant/1.0
profile: ticket
alias: t-c168
uuid: 01a1b7dc-2937-71ff-b530-c77f3cfee7d2
priority: 2
created_at: "2026-11-07T21:13:41Z"
created_by: "codex-contributor-setup"
summary: "Cancel a stalled Assistant turn promptly and return the composer to a usable state."
labels:
  - "frontend-engineer"
paths:
  - "crates/ai/src/provider/**"
  - "crates/ai/tests/provider*"
  - "apps/rusty-brick-chess/src/domain_api.rs"
  - "apps/rusty-brick-chess/frontend/src/views/assistant.rs"
  - "apps/rusty-brick-chess/frontend/tests/cdp/cdp_ai_stream.js"
---
# Frontend Engineer — Make Stop responsive during a provider stall

Start with `crates/ai/src/provider/openai.rs` and `cdp_ai_stream.js`. Build a reproduction where a mock provider emits one content chunk and then holds the connection open. Preserve the approval boundary and the existing active-stream behavior.

## Done when

- In the browser regression, a mock provider sends one content chunk and then stays silent with the connection open. After clicking **Stop**, the composer returns to a sendable state within two seconds.
- A subsequent turn succeeds. The cancelled turn makes no further board writes, verified against the board revision or audit log.
- The existing active-stream scenario still passes. Include the browser output for both cases in the PR.

## Checklist


- [ ] Make Stop release the composer within two seconds of clicking while the provider is silent. (i-21d5)
- [ ] Verify a subsequent turn succeeds and the cancelled turn produces no further board writes. (i-96e4)
- [ ] Keep a browser regression for the silent-provider case alongside the active-stream check. (i-3a6a)

## M-i-42e3 · milestone — Signed off by a person
min: signed

- [ ] Signed off by a person (i-42e3)
