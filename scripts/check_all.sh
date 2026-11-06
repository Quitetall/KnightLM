#!/usr/bin/env bash
# Run everything: formatting, lints, both test profiles, and every browser check.
#
# One command, because the alternative is remembering which suites need
# `--all-features`, which need a model behind the server, and which ports are
# free. Each browser suite gets its own Chrome on its own debugging port, and
# the server for them runs on its own database so a check cannot inherit state
# from a previous run.
#
#   scripts/check_all.sh              # everything
#   scripts/check_all.sh --rust       # just fmt, clippy and cargo test
#   scripts/check_all.sh --cdp        # just the browser checks
#   scripts/check_all.sh --repeat 3   # run each browser check three times
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP="$ROOT/apps/rusty-brick-chess"
CDP="$APP/frontend/tests/cdp"
SCRATCH="${RBC_CHECK_SCRATCH:-/mnt/4tb/tmp/knightlm-check}"
PORT="${RBC_CHECK_PORT:-8090}"
# Deliberately not 8100: cdp_ai_stream, cdp_ai_context and cdp_ai_scopes host
# their own provider there, and two servers on one port is a silent standoff.
MOCK_PORT="${RBC_CHECK_MOCK_PORT:-8110}"
REPEAT=1
DO_RUST=1
DO_CDP=1

while [ $# -gt 0 ]; do
  case "$1" in
    --rust) DO_CDP=0; shift ;;
    --cdp) DO_RUST=0; shift ;;
    --repeat) REPEAT="$2"; shift 2 ;;
    -h|--help) sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

mkdir -p "$SCRATCH"
FAILURES=0
step() { printf '\n=== %s ===\n' "$*"; }
fail() { FAILURES=$((FAILURES + 1)); echo "FAIL: $*"; }

# A port held by something old is the single most confusing failure here: the
# new server silently does not start and every request reaches the wrong one.
free_port() {
  local port="$1"
  local pid
  pid=$(ss -ltnp 2>/dev/null | grep ":$port " | grep -oP 'pid=\K[0-9]+' | head -1)
  if [ -n "${pid:-}" ]; then
    echo "stopping pid $pid on port $port"
    kill "$pid" 2>/dev/null
    for _ in $(seq 1 20); do
      ss -ltn 2>/dev/null | grep -q ":$port " || return 0
      sleep 0.2
    done
    echo "port $port is still held" >&2
    return 1
  fi
}

wait_for_health() {
  local url="$1"
  for _ in $(seq 1 40); do
    if curl -sf -m 2 "$url/api/health" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.25
  done
  return 1
}

cleanup() {
  for pid in "${SERVER_PID:-}" "${MOCK_PID:-}"; do
    [ -n "$pid" ] && kill "$pid" 2>/dev/null
  done
  pkill -9 -x chrome 2>/dev/null
}
trap cleanup EXIT

# --- rust ------------------------------------------------------------------

if [ "$DO_RUST" = 1 ]; then
  step "cargo fmt --check"
  (cd "$ROOT" && cargo fmt --all -- --check) || fail "fmt"

  step "clippy (workspace and all features), warnings are errors"
  (cd "$ROOT" && cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings) || fail "clippy"
  (cd "$ROOT" && cargo clippy --workspace --all-targets --all-features --locked -j 2 -- -D warnings) \
    || fail "clippy --all-features"

  # Two runs on purpose: default features skip the provider and host suites, and
  # a run that only compiles them is how they broke before.
  step "cargo test --workspace"
  (cd "$ROOT" && cargo test --workspace --locked -j 2) || fail "cargo test"

  step "cargo test --workspace --all-features"
  (cd "$ROOT" && cargo test --workspace --all-features --locked -j 2) || fail "cargo test --all-features"
fi

# --- browser ---------------------------------------------------------------

step "check_compare.sh (compare tests in one place)"
(cd "$ROOT" && timeout 600 ./scripts/check_compare.sh) || fail "check_compare.sh"

step "check_studies.sh (study tests in one place)"
(cd "$ROOT" && timeout 600 ./scripts/check_studies.sh) || fail "check_studies.sh"

step "check_mcp.sh (MCP transports end to end)"
(cd "$ROOT" && timeout 300 ./scripts/check_mcp.sh) || fail "check_mcp.sh"

if [ "$DO_CDP" = 1 ]; then
  step "build the app"
  (cd "$APP" && cargo build --locked) || fail "cargo build"
  (cd "$APP/frontend" && ./deploy.sh) || fail "frontend deploy"

  free_port "$PORT" || fail "port $PORT"
  free_port "$MOCK_PORT" || fail "port $MOCK_PORT"

  step "mock provider and server"
  # MOCK_REASONING is what cdp_ai_watch greps for, so the fixture names it.
  (cd "$ROOT" && setsid env MOCK_REASONING="controls the centre" \
      python3 scripts/mock_provider.py --port "$MOCK_PORT" \
      >"$SCRATCH/mock.log" 2>&1 < /dev/null &) 
  sleep 0.7
  rm -f "$SCRATCH/check.db"
  (cd "$APP" && setsid env \
      STOCKFISH_PATH="${STOCKFISH_PATH:-$HOME/.local/bin/stockfish}" \
      MODEL_URL="http://127.0.0.1:$MOCK_PORT/v1/chat" \
      RBC_DB="$SCRATCH/check.db" \
      RBC_ADDR="127.0.0.1:$PORT" \
      "$APP/target/debug/rusty-brick-chess" >"$SCRATCH/server.log" 2>&1 < /dev/null &)
  if ! wait_for_health "http://127.0.0.1:$PORT"; then
    fail "the server did not come up; see $SCRATCH/server.log"
    tail -5 "$SCRATCH/server.log" >&2
  else
    curl -s "http://127.0.0.1:$PORT/api/health"; echo
  fi
  # Keep the trap able to stop what this script started.
  SERVER_PID=$(ss -ltnp 2>/dev/null | grep ":$PORT " | grep -oP 'pid=\K[0-9]+' | head -1)
  MOCK_PID=$(ss -ltnp 2>/dev/null | grep ":$MOCK_PORT " | grep -oP 'pid=\K[0-9]+' | head -1)

  start_server() {
    local model_url="$1"
    free_port "$PORT" || return 1
    (cd "$APP" && setsid env \
        STOCKFISH_PATH="${STOCKFISH_PATH:-$HOME/.local/bin/stockfish}" \
        MODEL_URL="$model_url" \
        RBC_DB="$SCRATCH/check.db" \
        RBC_ADDR="127.0.0.1:$PORT" \
        "$APP/target/debug/rusty-brick-chess" >"$SCRATCH/server.log" 2>&1 < /dev/null &)
    wait_for_health "http://127.0.0.1:$PORT"
  }

  # Some suites bring their own provider on 8100 and so need the server pointed
  # there; the rest use this script's mock. Two fixed groups, one server each:
  # restarting per suite left the port churning and the run hanging.
  SELF_HOSTED="cdp_ai_stream.js cdp_ai_context.js cdp_ai_scopes.js"
  # A suite that queues approvals must not inherit another's queue, or it reads
  # the wrong first row. Clearing between suites keeps the shared database from
  # turning "passes alone" into "fails in a run".
  reset_queue() {
    command -v sqlite3 >/dev/null 2>&1 || return 0
    sqlite3 "$SCRATCH/check.db" "DELETE FROM ai_approvals;" 2>/dev/null || true
  }

  run_suite() {
    local name="ci_basename_placeholder"
    name="$(basename "$1")"
    reset_queue
    pkill -9 -x chrome 2>/dev/null
    sleep 0.2
    local out marker
    out=$(cd "$CDP" && timeout 240 node "$name" 2>&1)
    marker=$(printf '%s' "$out" | grep -oE '[A-Z0-9_]+_(PASS|FAIL)' | tail -1)
    case "$marker" in
      *_PASS) printf '  ok   %-24s %s\n' "$name" "$marker" ;;
      *_FAIL) printf '  FAIL %-24s %s\n' "$name" "$marker"; FAILURES=$((FAILURES + 1)) ;;
      *)      printf '  ???  %-24s (no PASS/FAIL marker)\n' "$name"
              printf '%s\n' "$out" | tail -4
              FAILURES=$((FAILURES + 1)) ;;
    esac
  }

  for run in $(seq 1 "$REPEAT"); do
    [ "$REPEAT" -gt 1 ] && step "browser checks, run $run of $REPEAT"

    step "suites with their own provider (server -> 8100)"
    if start_server "http://127.0.0.1:8100/v1/chat"; then
      for name in $SELF_HOSTED; do run_suite "$CDP/$name"; done
      SERVER_PID=$(ss -ltnp 2>/dev/null | grep ":$PORT " | grep -oP 'pid=\K[0-9]+' | head -1)
    else
      fail "server for the self-hosted suites"
    fi

    step "suites against the shared mock (server -> $MOCK_PORT)"
    if start_server "http://127.0.0.1:$MOCK_PORT/v1/chat"; then
      # cdp_session checks the one-time reset notice, which only appears when the
      # reset left something behind; give it one row to find.
      if command -v sqlite3 >/dev/null 2>&1; then
        sqlite3 "$SCRATCH/check.db" \
          "INSERT OR REPLACE INTO legacy_sessions (client, payload, updated) VALUES ('check-legacy','{}',1);" \
          2>/dev/null || true
      fi
      # cdp_composer talks to the mock provider, so it belongs to the shared group;
  # the loop below picks up every cdp_*.js, so nothing to add here.
  for script in "$CDP"/cdp_*.js; do
        base="$(basename "$script")"
        case " $SELF_HOSTED " in *" $base "*) continue ;; esac
        run_suite "$script"
      done
      SERVER_PID=$(ss -ltnp 2>/dev/null | grep ":$PORT " | grep -oP 'pid=\K[0-9]+' | head -1)
    else
      fail "server for the shared-mock suites"
    fi
  done
fi

step "summary"
if [ "$FAILURES" -eq 0 ]; then
  echo "ALL CHECKS PASSED"
  exit 0
fi
echo "$FAILURES check(s) failed"
exit 1
