---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a63a-7567-ab29-0a9330bca269
role: work_order
jurisdiction: authored
order: 40
classification: internal
---

# Work Order

Deliverables (packages PKG-090..PKG-094):

1. Performance/load: 1M-game DB, transcript/context volume, 16-model tournament, SSE under
   load, board list rendering.
2. Security review: keyring/secrets, redaction, permissions, approval bypass, MCP auth,
   resource traversal, rate limits, destructive actions.
3. Full suite: Rust crates, server, frontend wasm check, Tauri build, every CDP suite
   (existing + new AI suites), no warnings.
4. Docs: README, ROADMAP, MCP setup, study format, context spec, migration/reset notice,
   release notes.
5. Packaging: release build, install script, desktop bundle, signed/updater checklist for the
   operator.
6. RC: tag candidate, run all gates, record evidence, hand off signing.

Seams:

- No feature work; only fixes, tests, docs and packaging.
- Any behavior change discovered here reopens the owning work order.

Autonomy and escalation:

Tier T2. Escalate any scope change, security exception, or release-blocking behavior. The
performer owns test/docs/packaging mechanics.

Rollback:

A failed RC does not ship. The previous release remains; the database migration reset is
documented and irreversible.
