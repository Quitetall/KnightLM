#!/usr/bin/env bash
# Game gate for the `knightlm-uci` binary: a short match against Stockfish through the
# real HTTP serving app over a fake predictor (always the first legal move), driven by
# the crates/play match driver (`knightlm-match`). Fails on any illegal move, forfeit or
# engine error. It proves the plumbing (packet, serve contract, UCI, driver), not strength.
#
#   scripts/knightlm_uci_game_gate.sh [games] [max_plies]
#
# Env: KNIGHTLM_BIN_DIR (default target/debug; needs knightlm-uci and knightlm-match),
#      STOCKFISH_PATH (default: stockfish on PATH), PYTHON (default python3; needs
#      fastapi, uvicorn, httpx), GATE_DEPTH (Stockfish search depth, default 1: the
#      shallowest limit the driver offers; it has no node limit), GATE_FAKE_ARGS
#      (e.g. --illegal: a model that answers illegal moves, which must fail the gate).
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GAMES="${1:-2}"
PLIES="${2:-60}"
BIN="${KNIGHTLM_BIN_DIR:-$ROOT/target/debug}"
SF="${STOCKFISH_PATH:-$(command -v stockfish || true)}"
PY="${PYTHON:-python3}"
DEPTH="${GATE_DEPTH:-1}"
T="$(mktemp -d "${TMPDIR:-/tmp}/knightlm-uci-gate.XXXXXX")"
SERVER_PID=""
cleanup() { [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null; /usr/bin/rm -rf "$T"; }
fail() { echo "FAIL: $*" >&2; [ -f "$T/server.log" ] && tail -20 "$T/server.log" >&2; cleanup; exit 1; }
trap cleanup EXIT
for f in "$BIN/knightlm-uci" "$BIN/knightlm-match" "$SF"; do
  [ -x "$f" ] || fail "missing executable: $f"
done
"$PY" -c 'import fastapi, uvicorn, httpx' 2>/dev/null || fail "$PY lacks fastapi/uvicorn/httpx"

PORT="$("$PY" -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')"
(cd "$ROOT" && exec "$PY" -m model.serve.fake_server --port "$PORT" ${GATE_FAKE_ARGS:-}) >"$T/server.log" 2>&1 & SERVER_PID=$!
for _ in $(seq 100); do
  curl -s -m2 "127.0.0.1:$PORT/v1/health" >/dev/null 2>&1 && break; sleep 0.2
done
curl -s -m2 "127.0.0.1:$PORT/v1/health" >/dev/null || fail "fake model server did not start"
echo "ok   fake model server on 127.0.0.1:$PORT"

# knightlm-match spawns an engine by path only, so the model URL goes in a wrapper.
cat >"$T/knightlm-uci-model" <<EOF
#!/usr/bin/env bash
exec "$BIN/knightlm-uci" --player model --model "http://127.0.0.1:$PORT/v1/move"
EOF
chmod +x "$T/knightlm-uci-model"

timeout 300 "$BIN/knightlm-match" --white engine --white-engine "$T/knightlm-uci-model" \
  --black engine --black-engine "$SF" --depth "$DEPTH" --games "$GAMES" --swap-colors \
  --max-plies "$PLIES" --forfeit-on-error --seed 7 --summary-json "$T/summary.json" --pgn "$T/games.pgn" \
  >"$T/match.log" 2>&1 || { cat "$T/match.log" >&2; fail "match driver failed"; }

"$PY" - "$T/summary.json" "$GAMES" <<'PYEOF' || fail "summary check"
import json, sys
summary = json.load(open(sys.argv[1]))
want = int(sys.argv[2])
terms = summary["by_termination"]
bad = [t for t in terms if "illegal" in t or "error" in t or "forfeit" in t or "failure" in t]
assert not bad, f"bad terminations: {bad}"
assert summary["wins"] + summary["draws"] + summary["losses"] == want, summary
for record in summary["per_game"]:
    assert record["failure"] is None, record
    assert record["plies"] > 0, record
print(f"ok   {want} games, zero illegal moves, terminations: {sorted(terms)}")
PYEOF
echo "ok   knightlm-uci game gate passed"
