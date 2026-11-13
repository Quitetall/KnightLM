"""Game play against a fixed Stockfish setting, via the existing `knightlm-match` driver.

The model side is reached over HTTP (`ModelPlayer` posts the packet to /v1/move).
A python predictor is exposed to the driver through a small local shim that
contains no engine. Colors swap on every opening, each of the fixed openings is
played from both sides, claimable draws are adjudicated, a player error or an
illegal move is a loss, and outcomes are aggregated by termination.
"""

import json
import shutil
import subprocess
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

from model.eval.engine import REPO, find_binary, find_stockfish, sha256_path

OPENINGS = Path(__file__).with_name("openings.txt")
DEFAULTS = {"opponent_depth": 1, "max_plies": 200, "model_timeout_s": 120}


def serve_predictor(predictor):
    """Expose predictor.predict as POST /v1/move?mode=raw|constrained. Returns (server, url)."""

    class Handler(BaseHTTPRequestHandler):
        def do_POST(self):  # noqa: N802
            query = parse_qs(urlparse(self.path).query)
            length = int(self.headers.get("Content-Length", 0))
            body = json.loads(self.rfile.read(length))
            mode = query.get("mode", [body.get("mode", "raw")])[0]
            try:
                prediction = predictor.predict(body["packet"], mode)
                payload = {"move": prediction.move, "raw_reply": prediction.raw_reply}
                code = 200
            except Exception as error:  # noqa: BLE001 - surfaced to the driver as a forfeit
                payload, code = {"error": f"{type(error).__name__}: {error}"}, 500
            data = json.dumps(payload).encode()
            self.send_response(code)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Connection", "close")  # no pooled-connection races with the Rust client
            self.end_headers()
            self.wfile.write(data)

        def log_message(self, *args):
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, f"http://127.0.0.1:{server.server_address[1]}/v1/move"


def run_games(move_url, games, mode="raw", opponent_depth=DEFAULTS["opponent_depth"],
              max_plies=DEFAULTS["max_plies"], model_timeout_s=DEFAULTS["model_timeout_s"],
              binary=None, stockfish=None, timeout_s=7200):
    """Play `games` games (even number: every opening both colors). Returns the match summary."""
    if games % 2:
        raise ValueError("games must be even so every opening is played with both colors")
    match = find_binary(binary).with_name("knightlm-match")
    if not match.is_file():
        raise FileNotFoundError(f"{match} not found; build -p knightlm-cli")
    stockfish_path = find_stockfish(stockfish)
    scratch = Path(tempfile.mkdtemp(prefix="eval-games-", dir="/mnt/4tb/tmp"))
    try:
        summary_path = scratch / "summary.json"
        command = [
            str(match), "--white", "model", "--black", "engine", "--model", f"{move_url}?mode={mode}",
            "--engine", str(stockfish_path), "--depth", str(opponent_depth), "--games", str(games),
            "--openings", str(OPENINGS), "--swap-colors", "--claim-draws", "--forfeit-on-error",
            "--max-plies", str(max_plies), "--model-timeout", str(model_timeout_s),
            "--summary-json", str(summary_path),
        ]
        done = subprocess.run(command, capture_output=True, text=True, timeout=timeout_s)
        if done.returncode != 0:
            raise RuntimeError(f"knightlm-match failed: {done.stderr.strip()[:500]}")
        summary = json.loads(summary_path.read_text())
    finally:
        shutil.rmtree(scratch, ignore_errors=True)
    summary["protocol"] = {
        "opponent": f"Stockfish fixed depth {opponent_depth}, 1 thread, default hash, fresh process per game",
        "openings_sha256": sha256_path(OPENINGS), "model_mode": mode, "colors": "swapped per opening",
        "claim_draws": True, "max_plies": max_plies, "failure_rule": "illegal move or player error loses",
    }
    return summary


def games_axis(summary):
    """The scorecard view: counts and termination table, without per-game move lists."""
    keep = ("games", "wins", "draws", "losses", "score", "by_termination", "protocol", "openings")
    return {key: summary[key] for key in keep}
