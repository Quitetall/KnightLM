#!/usr/bin/env python3
"""Run RBC browser suites with a fresh database, owned processes, and build receipts."""

import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import socket
import sqlite3
import subprocess
from tempfile import TemporaryDirectory
import time
from urllib.error import URLError
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "apps/rusty-brick-chess"
CDP = APP / "frontend/tests/cdp"
SELF_HOSTED = {"cdp_ai_approvals", "cdp_ai_context", "cdp_ai_scopes", "cdp_ai_stream"}
ASSETS = ("index.html", "pkg/rbc-frontend.js", "pkg/rbc-frontend_bg.wasm")


def free_ports(count):
    with ExitStack() as stack:
        sockets = [stack.enter_context(socket.socket()) for _ in range(count)]
        for sock in sockets:
            sock.bind(("127.0.0.1", 0))
        return [sock.getsockname()[1] for sock in sockets]


def stop(process):
    # Each process owns its group, including browsers spawned by the test.
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    try:
        process.wait(timeout=3)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        process.wait(timeout=3)


def start(stack, cmd, log, env):
    output = stack.enter_context(log.open("wb"))
    proc = subprocess.Popen(cmd, cwd=APP, env=env, stdout=output, stderr=subprocess.STDOUT,
                            start_new_session=True)
    stack.callback(stop, proc)
    return proc


def serve_ready(proc, base):
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        if proc.poll() is not None:
            raise RuntimeError(f"app process exited ({proc.returncode})")
        try:
            with urlopen(base + "api/health", timeout=1) as response:
                if response.status == 200:
                    return
        except (OSError, URLError):
            time.sleep(0.2)
    raise RuntimeError("app health check timed out")


def cleanup_live_model(server_pid, port, receipt):
    from urllib.request import Request

    children = set()
    for task in Path(f"/proc/{server_pid}/task").glob("*/children"):
        try:
            children.update(task.read_text().split())
        except OSError:
            continue
    owned = False
    for child in children:
        try:
            args = Path(f"/proc/{child}/cmdline").read_bytes().split(b"\0")
            owned |= b"serve" in args and str(port).encode() in args
        except OSError:
            continue
    result = {"owned_runtime": owned, "model_unloaded": None}
    if owned:
        base = f"http://127.0.0.1:{port}/v1"
        try:
            request = Request(base + "/admin/models/unload", data=json.dumps({"model": "knightlm-smolvlm-500m"}).encode(),
                              headers={"content-type": "application/json"})
            try:
                with urlopen(request, timeout=10) as response:
                    response.read()
            except OSError:
                # Already unloaded is acceptable; verify residency below.
                pass
            with urlopen(base + "/admin/vram", timeout=5) as response:
                state = json.load(response)
            result["model_unloaded"] = not any(row.get("model") == "knightlm-smolvlm-500m"
                                               and row.get("state") == "loaded" for row in state.get("loaded", []))
        except OSError as error:
            result["error"] = type(error).__name__
    receipt.write_text(json.dumps(result, indent=2) + "\n")


def check_suite(suite, args, scratch, expected_assets):
    logs = args.evidence / suite.stem
    logs.mkdir()
    app_port, provider_port, cdp_port, lamu_port = free_ports(4)
    base = f"http://127.0.0.1:{app_port}/"
    database = scratch / "check.db"
    env = dict(os.environ, TMPDIR=str(scratch), RBC_DATA=str(scratch / "data"),
               RBC_DB=str(database), RBC_STATIC=str(args.static), RBC_ADDR=f"127.0.0.1:{app_port}",
               RBC_LAMU_BIN="/usr/bin/false", RBC_LAMU_PORT=str(lamu_port),
               MODEL_URL=f"http://127.0.0.1:{provider_port}/v1/chat",
               RBC_CDP_URL=base, RBC_CDP_PORT=str(cdp_port), RBC_CDP_PROVIDER_PORT=str(provider_port),
               RBC_CDP_EVIDENCE=str(logs),
               MOCK_REASONING="controls the centre")
    if suite.stem in {"cdp_engine_preview", "cdp_game_report", "cdp_sparring", "cdp_engine_match", "cdp_analysis_engine_play", "cdp_engine_settings"} and "STOCKFISH_PATH" not in env:
        if engine := shutil.which("stockfish"):
            env["STOCKFISH_PATH"] = engine
    if suite.stem == "cdp_tournament":
        env["MOCK_FOOLS_MATE"] = "1"
        env["RBC_CDP_TOURNAMENT_FIXTURE"] = "1"
    if suite.stem == "cdp_analysis_dock":
        env["MOCK_ANALYSIS_TOOLS"] = "1"
    if suite.stem == "cdp_provider_settings":
        env["MOCK_PROVIDER_REGISTRY"] = "1"
        env["RBC_PARITY_PROVIDER_KEY"] = os.urandom(24).hex()
        env.pop("RBC_PARITY_PROVIDER_KEY_MISSING", None)
    if suite.stem == "live_model_controls":
        runtime = shutil.which("lamu")
        if not runtime:
            raise RuntimeError("LAMU is not installed")
        env["RBC_LAMU_BIN"] = runtime
    no_provider = suite.stem == "cdp_ai_i18n_a11y"
    if no_provider:
        env.pop("MODEL_URL", None)
    with ExitStack() as stack:
        if not no_provider and suite.stem not in SELF_HOSTED:
            start(stack, ["python3", ROOT / "scripts/mock_provider.py", "--port", str(provider_port)],
                  logs / "provider.log", env)
        server = start(stack, [args.server], logs / "server.log", env)
        serve_ready(server, base)
        if suite.stem == "live_model_controls":
            stack.callback(cleanup_live_model, server.pid, lamu_port, logs / "runtime-cleanup.json")
        receipt = {}
        for asset in ASSETS:
            with urlopen(base + ("" if asset == "index.html" else asset), timeout=5) as response:
                served = response.read()
            receipt[asset] = hashlib.sha256(served).hexdigest()
            if receipt[asset] != expected_assets[asset]:
                raise RuntimeError(f"served asset differs from run build: {asset}")
        (logs / "assets.json").write_text(json.dumps(receipt, indent=2) + "\n")
        if suite.stem in {"cdp_session", "cdp_workspace_restore"}:
            with sqlite3.connect(database) as db:
                db.execute("INSERT INTO legacy_sessions (client, payload, updated) VALUES ('check-legacy','{}',1)")
        if suite.stem == "cdp_leaderboard":
            env["RBC_CDP_SEEDED_LEADERBOARD"] = "1"
            with sqlite3.connect(database) as db:
                db.executemany("INSERT INTO ratings VALUES (?, ?, ?, 0.06, ?, 1, 1)",
                               [("llm:fixture:Quartz", 1701, 50, 8),
                                ("llm:fixture:Zinc", 1550, 110, 4)])
                db.executemany(
                    "INSERT INTO ai_turns (id, session_id, seq, role, model_json, tokens_in, tokens_out, created)"
                    " VALUES (?, 'leaderboard-fixture', ?, 'assistant', ?, ?, ?, 1)",
                    [("q1", 1, '{"model":"Quartz"}', 23, 7), ("q2", 2, '{"model":"Quartz"}', 17, 3)],
                )
                db.execute(
                    "INSERT INTO ai_matches (id, white_json, black_json, state_json, created)"
                    " VALUES ('leaderboard-fixture', ?, ?, ?, 1)",
                    ('{"model":"Quartz"}', '{"model":"Zinc"}', json.dumps({"moves": [
                        {"color": "white", "attempts": 1}, {"color": "black", "attempts": 1},
                        {"color": "white", "attempts": 2},
                    ]})),
                )
        test = start(stack, ["node", suite], logs / "test.log", env)
        status = test.wait(timeout=240 if suite.stem in {"cdp_vision_editor", "live_model_controls", "live_online_accounts"} else 120)
        output = (logs / "test.log").read_text(errors="replace")
        markers = re.findall(r"\b[A-Z0-9_]+_(?:PASS|FAIL)\b", output)
        if status != 0 or not markers or not markers[-1].endswith("_PASS"):
            raise RuntimeError(f"exit {status}, marker {markers[-1] if markers else 'missing'}")
        print(f"PASS {suite.stem}: {markers[-1]}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server", type=Path, required=True)
    parser.add_argument("--static", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True, help="new evidence directory")
    parser.add_argument("suites", nargs="*", help="CDP filename stems, defaults to all")
    args = parser.parse_args()
    args.server = args.server.resolve()
    args.static = args.static.resolve()
    args.evidence = args.evidence.resolve()
    available = {p.stem: p for p in sorted(CDP.glob("cdp_*.js"))}
    available.update({p.stem: p for p in sorted(CDP.glob("live_*.js"))})
    names = args.suites or [name for name in available if name.startswith("cdp_")]
    if len(names) != len(set(names)) or set(names) - available.keys():
        parser.error("suite names must be unique existing cdp_* filename stems")
    args.evidence.mkdir(parents=True)
    failures = 0
    with TemporaryDirectory(prefix="knightlm-cdp-run-", dir="/mnt/4tb/tmp") as scratch:
        run = Path(scratch)
        scripts = run / "cdp"
        shutil.copytree(CDP, scripts)
        static = run / "public"
        shutil.copytree(args.static, static)
        args.static = static
        server = run / "rbc-server"
        shutil.copy2(args.server, server)
        args.server = server
        with server.open("rb") as binary:
            server_sha256 = hashlib.file_digest(binary, "sha256").hexdigest()
        assets = {asset: hashlib.sha256((static / asset).read_bytes()).hexdigest()
                  for asset in ASSETS}
        (args.evidence / "run.json").write_text(json.dumps({
            "server_sha256": server_sha256,
            "assets": assets,
            "suites": names,
            "scripts": {path.name: hashlib.sha256(path.read_bytes()).hexdigest()
                        for path in sorted(scripts.glob("*.js"))},
        }, indent=2) + "\n")
        # Chrome creates Unix sockets under its profile. Keep the per-suite
        # path short even when a CDP suite has a descriptive filename.
        for index, name in enumerate(names):
            fixture = run / str(index)
            fixture.mkdir()
            try:
                check_suite(scripts / available[name].name, args, fixture, assets)
            except (OSError, RuntimeError, subprocess.TimeoutExpired, sqlite3.Error) as error:
                failures += 1
                print(f"FAIL {name}: {error}; see {args.evidence / name}", flush=True)
    return bool(failures)


if __name__ == "__main__":
    raise SystemExit(main())
