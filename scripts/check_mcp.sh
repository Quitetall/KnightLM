#!/usr/bin/env bash
# MCP end-to-end: build, seed a scratch database, drive both transports.
#
# The clients live in apps/rusty-brick-chess/tests/mcp/. Each prints a result
# line; the suite passes only when both do.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP="$ROOT/apps/rusty-brick-chess"
SCRATCH="${RBC_MCP_SCRATCH:-/mnt/4tb/tmp/rbc-mcp-check}"
BIN="$APP/target/debug/rusty-brick-chess"
PORT="${RBC_MCP_PORT:-8123}"

rm -rf "$SCRATCH" && mkdir -p "$SCRATCH"
cd "$ROOT" || exit 1

step() { printf '\n== %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

step "build"
(cd "$APP" && cargo build) || fail "build"

step "seed a workspace and a board"
DB="$SCRATCH/mcp.db"

step "serve on 127.0.0.1:$PORT"
RBC_DB="$DB" RBC_ADDR="127.0.0.1:$PORT" "$BIN" >"$SCRATCH/server.log" 2>&1 &
SERVER=$!
trap 'kill $SERVER 2>/dev/null; wait $SERVER 2>/dev/null' EXIT
for _ in $(seq 1 40); do
  curl -s -o /dev/null "http://127.0.0.1:$PORT/api/health" && break
  sleep 0.25
done
curl -s -o /dev/null "http://127.0.0.1:$PORT/api/health" || fail "server did not start"

step "workspace seeded"
WS=$(curl -s -X POST "http://127.0.0.1:$PORT/api/workspaces" -H 'content-type: application/json' \
  -d '{"title":"MCP scratch","kind":"analysis"}' | python3 -c 'import json,sys; print(json.load(sys.stdin).get("id",""))')
[ -n "$WS" ] || fail "could not create a workspace"

step "stdio client"
RBC_BIN="$BIN" RBC_DB="$DB" timeout 120 node "$APP/tests/mcp/mcp_stdio_client.js" || fail "stdio client"

step "http client"
RBC_URL="http://127.0.0.1:$PORT" timeout 120 node "$APP/tests/mcp/mcp_http_client.js" || fail "http client"

printf '\nMCP_SUITE_PASS\n'
