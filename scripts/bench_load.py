#!/usr/bin/env python3
"""Measure the two things that only show up under load: a 16-model tournament,
and many conversations streaming at once.

    python3 scripts/bench_load.py --base http://127.0.0.1:8090 --tournament --streams 8

The server needs MODEL_URL pointing at a provider (scripts/mock_provider.py is
enough for the streaming half; the tournament plays real games, so a model that
answers with legal moves makes it meaningful).
"""
import argparse
import json
import statistics
import threading
import time
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor


def call(base, method, path, body=None, timeout=120):
    data = json.dumps(body).encode() if body is not None else None
    request = urllib.request.Request(
        base + path, data=data, method=method,
        headers={"content-type": "application/json"} if data else {},
    )
    with urllib.request.urlopen(request, timeout=timeout) as response:
        return json.loads(response.read() or b"{}")


def tournament(base, models=16, rounds=1):
    """A 16-model event: how long a round takes, and how many pairing batches."""
    names = [f"bench-model-{index}" for index in range(models)]
    created = call(base, "POST", "/api/ai/tournaments", {
        "name": "bench",
        "format": "swiss",
        "rounds": rounds,
        "competitors": names,
        "concurrency": 4,
    })
    identifier = created.get("id")
    started = time.time()
    batches = 0
    plies = 0
    while time.time() - started < 900:
        stepped = call(base, "POST", f"/api/ai/tournaments/{identifier}/step", {})
        batches += 1
        started_now = stepped.get("started") or []
        plies += len(started_now) if isinstance(started_now, list) else int(started_now)
        if stepped.get("status") in {"finished", "complete", "done"}:
            break
        if not started_now:
            break
    elapsed = time.time() - started
    final = call(base, "GET", f"/api/ai/tournaments/{identifier}")
    return {
        "models": models,
        "rounds": rounds,
        "batches": batches,
        "games_started": plies,
        "status": final.get("status"),
        "wall_seconds": round(elapsed, 2),
    }


def one_stream(base, index):
    """One conversation: open its stream, send a turn, and time what arrives.

    The turn and the stream are separate requests - the stream is what the UI
    listens to - so the interesting number is how long after sending the first
    event reaches a reader that was already waiting.
    """
    session = call(base, "POST", "/api/ai/sessions", {
        "title": f"load-{index}", "mode": "chat",
    })
    identifier = session.get("id")
    first = [None]
    events = [0]
    sent = [None]
    stop = [None]

    def listen():
        # Read line by line: iterating the response waits for the whole body,
        # which never ends on a stream. The subscription also needs a moment to
        # register before the turn starts broadcasting.
        try:
            with urllib.request.urlopen(
                base + f"/api/ai/sessions/{identifier}/stream", timeout=180
            ) as response:
                while True:
                    if stop[0] is not None and time.time() > stop[0]:
                        break
                    raw = response.readline()
                    if not raw:
                        break
                    if raw.startswith(b"data:"):
                        # Latency is what a person waits for after pressing send,
                        # so it is measured from the send and not from subscribe.
                        if first[0] is None and sent[0] is not None:
                            first[0] = time.time() - sent[0]
                        events[0] += 1
        except Exception:
            pass

    listener = threading.Thread(target=listen, daemon=True)
    listener.start()
    time.sleep(0.5)
    sent[0] = time.time()
    call(base, "POST", f"/api/ai/sessions/{identifier}/send",
         {"message": "hello", "fen": None}, timeout=180)
    total = time.time() - sent[0]
    # Give the reader a moment to drain what the turn already broadcast.
    stop[0] = time.time() + 1.0
    listener.join(timeout=5)
    return {"first_event_s": first[0], "total_s": total, "events": events[0]}


def streams(base, count):
    started = time.time()
    with ThreadPoolExecutor(max_workers=count) as pool:
        results = list(pool.map(lambda index: one_stream(base, index), range(count)))
    wall = time.time() - started
    firsts = [r["first_event_s"] for r in results if r["first_event_s"] is not None]
    totals = [r["total_s"] for r in results]
    return {
        "concurrent": count,
        "wall_seconds": round(wall, 2),
        "first_event_median_s": round(statistics.median(firsts), 3) if firsts else None,
        "first_event_max_s": round(max(firsts), 3) if firsts else None,
        "total_median_s": round(statistics.median(totals), 3),
        "total_max_s": round(max(totals), 3),
        "turns_completed": sum(1 for r in results if r["events"] > 0),
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--base", default="http://127.0.0.1:8090")
    parser.add_argument("--tournament", action="store_true")
    parser.add_argument("--models", type=int, default=16)
    parser.add_argument("--rounds", type=int, default=1)
    parser.add_argument("--streams", type=int, default=0)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    report = {}
    if args.tournament:
        report["tournament"] = tournament(args.base, args.models, args.rounds)
    if args.streams:
        report["streams"] = streams(args.base, args.streams)

    if args.json:
        print(json.dumps(report, indent=2))
    else:
        for name, block in report.items():
            print(name + ":")
            for key, value in block.items():
                print(f"  {key}: {value}")


if __name__ == "__main__":
    main()
