# Using RustyBrickChess over MCP

RustyBrickChess is an **MCP server**: an agent (Claude Desktop, opencode, an IDE
agent, a script) calls its chess and study tools, reads its boards as resources,
and asks for its prompts. This page is how to connect and what the surface is.

Nothing here is a separate build. The same binary serves the app and MCP; an MCP
client shares the database the app uses, so an agent and a person working at the
same desk see the same boards.

## Connecting

### stdio (Claude Desktop, opencode)

The server speaks JSON-RPC on stdin and stdout, one message per line. stdio
**inherits process trust** — the parent process already runs as you with your
database — so it needs no token.

Claude Desktop, in `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "rusty-brick-chess": {
      "command": "/path/to/rusty-brick-chess",
      "args": ["mcp"],
      "env": { "RBC_DB": "/home/you/.local/share/rusty-brick-chess/rbc.db" }
    }
  }
}
```

opencode, in `opencode.json`:

```json
{
  "mcp": {
    "rusty-brick-chess": {
      "type": "local",
      "command": ["/path/to/rusty-brick-chess", "mcp"],
      "enabled": true
    }
  }
}
```

`RBC_DB` is optional; without it the server opens the default database. Two MCP
processes on the same file are two writers to one SQLite database — fine for
reads, and writes are revision-checked, but prefer one connection.

### Streamable HTTP (local agents, several clients)

```bash
rusty-brick-chess            # serve; the app and POST /mcp share one process
curl -s http://127.0.0.1:8090/api/mcp/config
```

```json
{
  "http": "http://127.0.0.1:8090/mcp",
  "stdio": "rusty-brick-chess mcp",
  "token": "mcp-…",
  "protocol": "2025-06-18",
  "max_message_bytes": 1048576,
  "requests_per_minute": 240
}
```

Every HTTP request carries the token:

```bash
curl -s -X POST http://127.0.0.1:8090/mcp \
  -H "authorization: Bearer $TOKEN" \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

Without it the answer is `401` and nothing else. `POST /api/mcp/rotate` issues a
new token and invalidates the old one immediately. The listener binds loopback;
there is no `0.0.0.0` option, which is deliberate.

A connection gets 240 requests a minute; past that it gets `429`. A message over
1 MiB is refused.

## The surface

### Tools

The same registry the in-app assistant uses, with the same schemas: read tools
(`get_position`, `get_history`, `legal_moves`, `analyze`, `evaluate`,
`opening_explorer`, `search_database`, `tablebase`, `list_boards`, …), board
writes (`play_move`, `draw_arrow`, `annotate`, `write_note`, `create_board`,
`copy_board`), study writes (`create_study`, `add_chapter`,
`add_board_to_chapter`, `reorder_chapters`, `publish_study`, `write_study_text`)
and `approval_status` for polling. `tools/list` is the authority; ask it rather
than hardcoding names.

There is deliberately **no** chat, no Lua, no settings, no provider keys and no
access to AI sessions over MCP.

### Writes and approvals

A write that changes stored content is not performed on the spot. The call
returns:

```json
{
  "status": "approval_required",
  "approval_id": "…",
  "changeset_summary": "play_move changes stored content and needs approval before it runs",
  "poll": "approval_status"
}
```

A person sees it in the app's approval queue. Your client polls:

```json
{"name": "approval_status", "arguments": {"approval_id": "…"}}
```

which answers `pending`, `approved`, `denied` or `expired`. Once approved, the
write has been applied by the app; read the board back to see the result rather
than repeating the call. Nothing runs while it is pending — a refusal is not a
half-write.

A person may grant a scope so MCP stops asking, in bulk: `board-write`,
`study-content` or `destructive`, stored in the `mcp_auto_approve` setting.
Empty by default.

### Resources

Read-only snapshots, revision-stamped in the payload:

```text
rbc://workspaces
rbc://workspace/{id}             the workspace and its boards
rbc://board/{id}
rbc://board/{id}/history
rbc://study/{id}
rbc://study/{id}/chapter/{chapter_id}
rbc://match/{id}
rbc://match/{id}/pgn
```

Any other uri resolves to nothing. Subscriptions are not offered in this
release; poll or re-read.

### Prompts

`analyze_position` (`board_id`) and `review_game` (`game_id`): fixed templates,
identical output for identical arguments.

## Checking a connection

```bash
./scripts/check_mcp.sh
```

Builds the binary, seeds a scratch database and runs both clients (stdio and
HTTP) through initialization, listing, a read, a write that must ask for
approval, polling and a rotation. It prints `MCP_STDIO_PASS` and `MCP_HTTP_PASS`
or the reason it stopped.

## Limits worth knowing

- MCP cannot see or change app settings, AI sessions or provider keys.
- Writes are revision-checked; a board that is locked or published refuses and
  offers a copy, over MCP as in the app.
- The approval queue is shared between the app and MCP: one queue, one audit log.
  Every approval transition is written to `ai_audit`.
- Errors are results, not exceptions: an unknown tool or bad arguments come back
  as JSON-RPC errors, and a tool that refuses says so in its result.
