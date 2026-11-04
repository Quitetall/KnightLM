#!/usr/bin/env bash
# Every study test in one command: storage, tools, authoring, approval, bundle,
# progress, versions, and the browser editor.
#
# The point is one place to look when a study regresses; the same tests also run
# in `cargo test --workspace`, so this is a convenience rather than a second gate.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 1

step() { printf '\n== %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

step "storage: CRUD, order, revisions"
cargo test -p rbc-db --test db --test foundation --test study_versions --test study_progress \
  || fail "rbc-db study tests"

step "tools: authoring, registry, locks"
cargo test -p rbc-ai --all-features --test study_tools --test write_tools \
  || fail "study tool tests"

step "approval gate for study writes"
cargo test -p rbc-ai --all-features --test study_authoring_approval --test approvals \
  || fail "study approval tests"

step "bundle: round trip, unknown fields, lineage"
cargo test -p rbc-ai --all-features --test study_bundle || fail "bundle tests"

step "browser: the editor"
# The editor check needs a server and a deployed frontend. Reuse one that is
# already healthy; otherwise serve a scratch database for the duration.
SCRATCH="${RBC_STUDY_SCRATCH:-/mnt/4tb/tmp/rbc-study-check}"
PORT="${RBC_STUDY_PORT:-8090}"
BIN="$ROOT/apps/rusty-brick-chess/target/debug/rusty-brick-chess"
SERVER=""
cleanup() { [ -n "$SERVER" ] && kill "$SERVER" 2>/dev/null; }
trap cleanup EXIT

if ! curl -s -o /dev/null -m 2 "http://127.0.0.1:$PORT/api/health"; then
  rm -rf "$SCRATCH" && mkdir -p "$SCRATCH"
  (cd "$ROOT/apps/rusty-brick-chess" && cargo build) || fail "build"
  RBC_DB="$SCRATCH/studies.db" RBC_ADDR="127.0.0.1:$PORT" "$BIN" >"$SCRATCH/server.log" 2>&1 &
  SERVER=$!
  for _ in $(seq 1 40); do
    curl -s -o /dev/null "http://127.0.0.1:$PORT/api/health" && break
    sleep 0.25
  done
  curl -s -o /dev/null "http://127.0.0.1:$PORT/api/health" || fail "server did not start"
fi
(cd "$ROOT/apps/rusty-brick-chess/frontend/tests/cdp" && node cdp_study.js) || fail "cdp_study"

printf '\nSTUDY_SUITE_PASS\n'
