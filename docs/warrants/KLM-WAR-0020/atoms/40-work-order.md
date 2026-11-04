---
schema: oh.war/atom/v1
warrant_uuid: 01a1a566-a622-7461-b929-a2e9a1941af0
role: work_order
jurisdiction: authored
order: 40
classification: internal
---

# Work Order

Deliverables (packages PKG-080..PKG-083):

1. MCP server lifecycle: stdio and Streamable HTTP on `127.0.0.1`; token generation/rotation;
   connection registry; rate limits; max request size.
2. Tool registry mapping: the read and write tools from `60-mcp.md`, stable schemas and
   versions; unknown arguments refused; no Lua/app-lifecycle/chat tools.
3. Resources: `rbc://workspaces`, workspace/board/study/chapter/match snapshots,
   revision-stamped; prompts: analyze, review, build study, compare, play move.
4. Approval jobs: async `approval_required`, polling, approve/deny/expire, auto-approve
   scopes, audit; revision conflict/copy_required surfaced to the client.
5. Audit: connection, tool, arguments (redacted), target/revision, approval, result, duration.
6. Docs + mock client tests for stdio and HTTP.

Seams:

- Same tool registry/policy as W-03; no parallel implementation.
- Board/study writes go through the repository APIs and revision checks.
- The in-app approval queue renders MCP jobs identically.

Autonomy and escalation:

Tier T2. Escalate changes to the tool list, approval semantics, or transport/auth. The
performer owns protocol plumbing and docs.

Rollback:

MCP is additive. Disabling it leaves in-app AI, studies and matches unaffected. The token can
be revoked without touching data.
