#!/usr/bin/env bash
# Every compare test in one command: the engine, its fixture matrix, the export
# and the browser.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 1

step() { printf '\n== %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

step "engine and fixture matrix"
cargo test -p rbc-ai --all-features --test diff --test diff_fixtures || fail "diff tests"

step "the read-only guarantee, from the HTTP surface"
# The compare route must not change anything it reads; the test above asserts it
# against the repository, this asserts the route exists and answers.
cargo test -p rbc-ai --all-features --test diff_fixtures comparing_writes_nothing \
  || cargo test -p rbc-ai --all-features --test diff_fixtures comparing_touches_no_table_it_does_not_read \
  || fail "read-only assertion"

step "browser"
SCRATCH="${RBC_COMPARE_SCRATCH:-/mnt/4tb/tmp/rbc-compare-check}"
PORT="${RBC_COMPARE_PORT:-8090}"
BIN="$ROOT/apps/rusty-brick-chess/target/debug/rusty-brick-chess"
SERVER=""
cleanup() { [ -n "$SERVER" ] && kill "$SERVER" 2>/dev/null; }
trap cleanup EXIT

if ! curl -s -o /dev/null -m 2 "http://127.0.0.1:$PORT/api/health"; then
  rm -rf "$SCRATCH" && mkdir -p "$SCRATCH"
  (cd "$ROOT/apps/rusty-brick-chess" && cargo build) || fail "build"
  (cd "$ROOT/apps/rusty-brick-chess/frontend" && ./deploy.sh) || fail "frontend deploy"
  # A tmpfs database: this runs alongside other builds and the 4 TB disk is the
  # slowest thing in the room when they do.
  RBC_DB="/tmp/rbc-compare-check.db" RBC_ADDR="127.0.0.1:$PORT" \
    "$BIN" >"$SCRATCH/server.log" 2>&1 &
  SERVER=$!
  for _ in $(seq 1 60); do
    curl -s -o /dev/null "http://127.0.0.1:$PORT/api/health" && break
    sleep 0.5
  done
  curl -s -o /dev/null "http://127.0.0.1:$PORT/api/health" || fail "server did not start"
fi
(cd "$ROOT/apps/rusty-brick-chess/frontend/tests/cdp" && node cdp_compare.js) || fail "cdp_compare"

printf '\nCOMPARE_SUITE_PASS\n'
