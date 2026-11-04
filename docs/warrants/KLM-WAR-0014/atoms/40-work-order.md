---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a597-70da-9816-c5fd5c93349d
role: work_order
jurisdiction: authored
order: 40
classification: internal
---

# Work Order

Deliverables (packages PKG-020..PKG-028):

1. `rbc-ai` crate with `protocol`/`core`/`host` features; session/turn DTOs; SQLite store;
   create/list/rename/archive/delete; append-only turns.
2. Tool registry: names, JSON schemas, versions, effects (`read`, `board-write`,
   `session-write`, `destructive`, `external`), mode/origin policy.
3. Read tools: position, history, legal moves, analyze, eval, openings, database, tablebase,
   study read. Board tools: play, arrows, circles, annotate, notes, create/copy, revisions,
   lock/publish checks.
4. Provider trait + OpenAI-compatible adapter: chat completions, tools, streaming SSE, usage,
   normalized errors, timeout/cancellation, retries.
5. Agent loop: compile context, call provider, execute tools, feed results, retry illegal
   moves/tool errors, max rounds, cancellation, store turn/context hash.
6. SSE event stream + replay from `after`; replay endpoint rebuilds context and returns
   match/diff.
7. Approval service: per-turn changesets, pending/approved/denied/expired, auto-approve
   scopes, audit; async approval ids for MCP.
8. Mock provider + integration tests for streaming, tool loops, retries, conflicts, approvals.

Seams:

- `rbc-context` compiles; `rbc-ai` supplies evidence and stores the result.
- The server routes (W-04/W-09) are thin adapters over `rbc-ai`.
- MCP and the in-app AI share the same tool registry and policy engine.

Autonomy and escalation:

Tier T2. Escalate changes to the tool effect model, approval semantics, or provider contract.
The performer owns internal module layout and error codes.

Rollback:

Additive crate + v4 tables. If the runtime is reverted, sessions/turns remain readable but
inert; no chess data is touched.
