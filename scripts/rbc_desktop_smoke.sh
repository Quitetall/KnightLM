#!/usr/bin/env bash
# Launch the built RustyBrickChess desktop binary under a virtual X display and
# check what makes it a desktop app: a window appears, the per-launch token gates
# the API, a PGN handed to a second launch reaches the first, and quitting stops
# the server and its engine.
#
#   scripts/rbc_desktop_smoke.sh [path/to/rusty-brick-chess-desktop] [screenshot.png]
#
# Needs Xvfb, xdotool, curl, ImageMagick `import`, and the web assets in
# apps/rusty-brick-chess/frontend/public. Linux only.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${1:-$ROOT/apps/rusty-brick-chess/src-tauri/target/debug/rusty-brick-chess-desktop}"
SHOT="${2:-}"
T="$(mktemp -d "${TMPDIR:-/tmp}/rbc-desktop-smoke.XXXXXX")"
DISP=":$((90 + RANDOM % 9))"
TOKEN="$(head -c 24 /dev/urandom | od -An -tx1 | tr -d ' \n')"
APP_PID=""; XVFB_PID=""
fail() { echo "FAIL: $*" >&2; cat "$T/run.log" >&2 2>/dev/null; cleanup; exit 1; }
cleanup() {
  [ -n "$APP_PID" ] && kill -9 "$APP_PID" 2>/dev/null
  [ -n "$XVFB_PID" ] && kill "$XVFB_PID" 2>/dev/null
  /usr/bin/rm -rf "$T"
}
Xvfb "$DISP" -screen 0 1600x1000x24 >/dev/null 2>&1 & XVFB_PID=$!
sleep 2
export DISPLAY="$DISP" GDK_BACKEND=x11 WEBKIT_DISABLE_COMPOSITING_MODE=1
unset WAYLAND_DISPLAY RBC_DB STOCKFISH_PATH
export XDG_DATA_HOME="$T/data" XDG_CONFIG_HOME="$T/cfg" RBC_DESKTOP_TOKEN="$TOKEN"
mkdir -p "$T/data" "$T/cfg"

"$BIN" >"$T/run.log" 2>&1 & APP_PID=$!
PORT=""
for _ in $(seq 100); do
  PORT="$(sed -n 's|^rbc-desktop: server on http://127.0.0.1:\([0-9]*\)/.*|\1|p' "$T/run.log")"
  [ -n "$PORT" ] && break; sleep 0.2
done
[ -n "$PORT" ] || fail "no server port reported"
for _ in $(seq 50); do xdotool search --name RustyBrickChess >/dev/null 2>&1 && break; sleep 0.2; done
xdotool search --name RustyBrickChess >/dev/null 2>&1 || fail "no RustyBrickChess window"
echo "ok   window exists, server on 127.0.0.1:$PORT"

code() { curl -s -o /dev/null -m5 -w '%{http_code}' "$@"; }
[ "$(code "127.0.0.1:$PORT/api/health")" = 401 ] || fail "API answered without the token"
[ "$(code -H 'x-rbc-token: wrong' "127.0.0.1:$PORT/api/health")" = 401 ] || fail "API accepted a wrong token"
[ "$(code -H "x-rbc-token: $TOKEN" "127.0.0.1:$PORT/api/health")" = 200 ] || fail "API refused the token"
echo "ok   API refuses no/wrong token, accepts the launch token"
case "$(curl -s -m5 -H "x-rbc-token: $TOKEN" "127.0.0.1:$PORT/api/health")" in
  *'"ok":true'*) echo "ok   health body ok:true" ;; *) fail "health body" ;; esac
[ -n "$SHOT" ] && { sleep 5; import -window root "$SHOT"; echo "ok   screenshot $SHOT"; }

printf '[Event "smoke"]\n[White "A"]\n[Black "B"]\n[Result "1-0"]\n\n1. e4 e5 2. Nf3 Nc6 1-0\n' >"$T/one.pgn"
"$BIN" "$T/one.pgn" >"$T/second.log" 2>&1
for _ in $(seq 30); do
  games="$(curl -s -m5 -H "x-rbc-token: $TOKEN" "127.0.0.1:$PORT/api/health" | sed -n 's/.*"games":\([0-9]*\).*/\1/p')"
  [ "$games" = 1 ] && break; sleep 0.2
done
[ "$games" = 1 ] || fail "second launch did not hand its PGN to the running app (games=$games)"
echo "ok   second launch focused the first and imported its PGN"
[ -f "$T/data/org.knightlm.rustybrickchess/rusty-brick-chess.db" ] || fail "database not in the app data dir"
echo "ok   database lives in the app data dir"

# A long analysis spawns a engine child (when stockfish is on PATH); quitting must end it.
ENGINE=""
if command -v stockfish >/dev/null 2>&1; then
  curl -s -m60 -H "x-rbc-token: $TOKEN" \
    "127.0.0.1:$PORT/api/analyze/lines?fen=rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR%20w%20KQkq%20-%200%201&depth=30" >/dev/null 2>&1 &
  sleep 2
  ENGINE="$(pgrep -P "$APP_PID" -x stockfish | head -1)"
  [ -n "$ENGINE" ] && echo "ok   engine child $ENGINE running under the app"
fi
kill -TERM "$APP_PID"
for _ in $(seq 60); do kill -0 "$APP_PID" 2>/dev/null || break; sleep 0.2; done
kill -0 "$APP_PID" 2>/dev/null && fail "app did not exit on SIGTERM"
APP_PID=""
[ "$(code -m2 "127.0.0.1:$PORT/api/health")" = 000 ] || fail "server still listening after quit"
# The engine exits when its stdin closes with the app; allow it a moment, but
# a leaked engine would live forever, so the wait is bounded.
for _ in $(seq 25); do [ -z "$ENGINE" ] || kill -0 "$ENGINE" 2>/dev/null || break; sleep 0.2; done
[ -z "$ENGINE" ] || ! kill -0 "$ENGINE" 2>/dev/null || fail "engine $ENGINE outlived the app"
echo "ok   quit stopped the server${ENGINE:+ and the engine}"
cleanup
echo "RBC_DESKTOP_SMOKE_PASS"
