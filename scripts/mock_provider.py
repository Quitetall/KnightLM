#!/usr/bin/env python3
"""A tiny OpenAI-compatible endpoint for the browser checks.

The Rust suites have `tests/common` for this; the CDP scripts need a real
process on a port, so this is the same idea in a hundred lines of stdlib.

It answers a chat completion with:
  - the first legal move listed in the prompt, when the prompt lists moves
    (so a match or a watch run stays legal as the game advances);
  - `MOCK_TEXT` (default: a short sentence) otherwise.

`MOCK_REASONING` attaches reasoning when the prompt asks for it, `MOCK_MOVE`
forces a specific move, `MOCK_STATUS` answers that HTTP status instead (429 or
500 are the useful ones), and `MOCK_DELAY` sleeps that many seconds per chunk
so a test can observe a turn mid-flight.

    python3 scripts/mock_provider.py --port 8100
"""
import argparse
import json
import os
import re
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def legal_moves(system: str):
    match = re.search(r"Legal moves:([^\n]*)", system)
    if not match:
        return []
    return match.group(1).split()


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        pass

    def _payload(self, body):
        messages = body.get("messages") or []
        system = " ".join(m.get("content") or "" for m in messages if m.get("role") == "system")
        user = " ".join(m.get("content") or "" for m in messages if m.get("role") == "user")

        status = int(os.environ.get("MOCK_STATUS", "0") or 0)
        if status:
            return status, {
                "error": {
                    "message": f"mock status {status}",
                    "type": "rate_limit_exceeded" if status == 429 else "server_error",
                }
            }

        reasoning = None
        if "casual chess game" in system:
            text = "Try Nf3: it develops a piece toward the centre."
        elif "Legal moves:" in system:
            forced = os.environ.get("MOCK_MOVE", "").strip()
            moves = legal_moves(system)
            # e4 / e5 rather than the first legal move, so a check can assert a
            # specific opening; anything else falls back to the first legal move.
            side = "black" if "Your side: black" in system else "white"
            opening = "e5" if side == "black" else "e4"
            text = forced or (opening if opening in moves else (moves[0] if moves else "e4"))
            if "Think briefly" in system:
                reasoning = os.environ.get("MOCK_REASONING", "a reasonable move")
        elif "reply with" in user.lower() or "hi" in user.lower():
            text = os.environ.get("MOCK_TEXT", "hello from the mock")
        else:
            text = os.environ.get("MOCK_TEXT", "hello from the mock")

        # Tool calls: the approval and scope checks need the model to ask for a
        # board before it asks to write to one.
        tool_names = set()
        for tool in (body.get("tools") or []):
            name = (tool.get("function") or {}).get("name")
            if name:
                tool_names.add(name)
        wants_write = "play" in user.lower() and "play_move" in tool_names
        if wants_write:
            already_listed = any(
                (m.get("role") == "tool") and "board_id" in str(m.get("content"))
                for m in messages
            )
            if not already_listed and "list_boards" in tool_names:
                return 200, {
                    "id": "mock",
                    "choices": [{
                        "index": 0,
                        "message": {
                            "content": None,
                            "tool_calls": [{
                                "id": "call_1",
                                "type": "function",
                                "function": {"name": "list_boards", "arguments": "{}"},
                            }],
                        },
                        "finish_reason": "tool_calls",
                    }],
                    "usage": {"prompt_tokens": 40, "completion_tokens": 3},
                }
            if already_listed:
                # The database accumulates boards across suites, so pick the one
                # the check just made rather than whichever came first.
                content = str([m for m in messages if m.get("role") == "tool"][-1].get("content"))
                try:
                    decoded = json.loads(content)
                except (json.JSONDecodeError, TypeError):
                    decoded = {}
                boards = decoded.get("data") if isinstance(decoded, dict) else None
                target = None
                if isinstance(boards, list):
                    named = [
                        board for board in boards
                        if isinstance(board, dict) and "Approval" in str(board.get("title", ""))
                    ]
                    # Several runs leave several boards with that name; the one
                    # still at revision 1 is the one this run just made.
                    fresh = [b for b in named if b.get("revision") == 1]
                    pool = fresh or named
                    if pool:
                        target = pool[-1]
                    elif boards and isinstance(boards[0], dict):
                        target = boards[0]
                board_id = (target or {}).get("board_id") or (target or {}).get("id")
                if not board_id:
                    found = re.search(r'\\?"board_id\\?"\s*:\s*\\?"([^"\\]+)', content)
                    board_id = found.group(1) if found else None
                if board_id:
                    revision = (target or {}).get("revision", 1)
                    return 200, {
                        "id": "mock",
                        "choices": [{
                            "index": 0,
                            "message": {
                                "content": None,
                                "tool_calls": [{
                                    "id": "call_2",
                                    "type": "function",
                                    "function": {
                                        "name": "play_move",
                                        "arguments": json.dumps({
                                            "board_id": board_id,
                                            "move": "e4",
                                            "expected_revision": revision,
                                        }),
                                    },
                                }],
                            },
                            "finish_reason": "tool_calls",
                        }],
                        "usage": {"prompt_tokens": 50, "completion_tokens": 4},
                    }

        message = {"content": text}
        if reasoning:
            message["reasoning_content"] = reasoning
        return 200, {
            "id": "mock",
            "object": "chat.completion",
            "choices": [{"index": 0, "message": message, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 40, "completion_tokens": 4},
        }

    def do_POST(self):
        length = int(self.headers.get("content-length") or 0)
        raw = self.rfile.read(length) if length else b"{}"
        try:
            body = json.loads(raw or b"{}")
        except json.JSONDecodeError:
            body = {}
        delay = float(os.environ.get("MOCK_DELAY", "0") or 0)
        if delay:
            time.sleep(delay)
        status, payload = self._payload(body)
        out = json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(out)))
        self.end_headers()
        self.wfile.write(out)

    def do_GET(self):
        out = b'{"ok":true,"mock":true}'
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(out)))
        self.end_headers()
        self.wfile.write(out)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=8100)
    args = parser.parse_args()
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    print(f"mock provider on http://127.0.0.1:{args.port}/v1/chat", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
