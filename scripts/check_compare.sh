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

step "the read-only guarantee"
# Name the tests that exist, and fail if the filter matches none: a filtered run
# that selects zero tests exits 0 and would report a guarantee nobody checked.
for name in comparing_touches_no_table_it_does_not_read a_report_is_a_snapshot_not_a_live_handle; do
  out=$(cargo test -p rbc-ai --all-features --test diff_fixtures "$name" 2>&1) || fail "read-only: $name"
  echo "$out" | grep -qE "test result: ok\. [1-9][0-9]* passed" \
    || fail "read-only: $name selected no test ($(echo "$out" | grep 'test result' | head -1))"
done

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
