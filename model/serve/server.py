"""HTTP serving port (KLM-WAR-0009) for knightlm-uci / knightlm-match.

Contract (crates/play/src/players.rs, ModelPlayer):
  POST /v1/move  {packet: <ContextPacket object>, packet_format_version: int}
                 -> {move, manifest_hash}
  GET  /v1/health, GET /v1/manifest

Engine-free by construction (KLM-WAR-0005): no engine is imported or reached.
Failures are HTTP errors; there is no fallback move of any kind.

  python -m model.serve.server --base-dir DIR [--adapter-dir DIR] \
      [--mode raw|constrained] [--prompt move|play] [--port 8000]

Play mode (the prompt a reasoning-corpus adapter is trained on, model/prompt.py play_text):
  POST /v1/play  {packet, packet_format_version, mode?}
                 -> {move, reasoning, code, path, raw_code, legal, raw_reply, manifest_hash}
`code` is "ok" or a stable refusal code (model/serve/play.py); `path` is "model" when the
model's own completion passed every check, "constrained_move" when the move field was chosen
from the legal list after the model's reasoning, "none" when no move is returned. A refusal
is an HTTP 502 in the default (constrained) path and a 200 record when raw is asked for
explicitly, exactly as /v1/move. `--prompt play` makes /v1/move answer through the play
prompt (same {move, manifest_hash, legal, raw_reply} body), so knightlm-uci can use it.
"""

import argparse
import hashlib
import json
from pathlib import Path

import time
import uuid

from fastapi import FastAPI, HTTPException
from fastapi.responses import JSONResponse, StreamingResponse
from pydantic import BaseModel, ConfigDict

from model.corpus.pipeline import sha256_file
from model.prompt import play_text, tool_names
from model.serve import coach as coach_mode
from model.serve.inference import MODES, Predictor

PACKET_FORMAT_VERSION = 1  # crates/core SCHEMA_VERSION
ROOT = Path(__file__).resolve().parents[1]


class MoveRequest(BaseModel):
    packet: dict
    packet_format_version: int
    mode: str | None = None  # "raw" | "constrained"; defaults to the server's --mode


class MoveResponse(BaseModel):
    move: str
    manifest_hash: str
    legal: bool = True
    raw_reply: str | None = None


class PlayResponse(BaseModel):
    move: str | None
    reasoning: str | None
    code: str
    path: str
    raw_code: str
    legal: bool
    raw_reply: str
    stated_move: str | None = None
    manifest_hash: str


PROMPTS = ("move", "play")

# --- OpenAI-compatible surface (docs/model/seam.md) ---------------------------------
MODEL_ID = "knightlm"
# The position packet rides in the last user message after the marker the play prompt
# (model/prompt.py play_text) already uses; the marker is derived from it, not copied.
PACKET_MARKER = play_text({})[:-len("{}")]
STREAM_CHUNK_CHARS = 24


class ChatRequest(BaseModel):
    """The OpenAI chat-completions fields we read; others are accepted and ignored."""
    model_config = ConfigDict(extra="allow")
    model: str
    messages: list[dict]
    tools: list[dict] | None = None
    tool_choice: str | dict | None = None
    temperature: float | None = None
    max_tokens: int | None = None
    stream: bool = False
    stream_options: dict | None = None
    # extension: {"mode": "raw" | "constrained", "turn": "play" | "coach", "summary": str}
    knightlm: dict | None = None


class ApiError(Exception):
    def __init__(self, status, message, code, param=None, kind="invalid_request_error"):
        self.status, self.body = status, {"error": {"message": message, "type": kind, "param": param, "code": code}}


def message_text(message):
    content = message.get("content")
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return "".join(p.get("text", "") for p in content if isinstance(p, dict) and p.get("type") == "text")
    return ""


def packet_from_messages(messages):
    """The packet in the last user message: after the play-prompt marker, bare JSON, or {"packet": ...}."""
    return packet_and_text(messages)[0]


def packet_and_text(messages):
    """(packet, text the person typed before the packet marker in the last user message)."""
    if not messages or messages[-1].get("role") != "user":
        raise ApiError(400, "the last message must have role 'user' and carry the position packet",
                       "invalid_messages", "messages")
    text = message_text(messages[-1])
    at = text.rfind(PACKET_MARKER)
    body = text[at + len(PACKET_MARKER):] if at >= 0 else text
    typed = text[:at] if at >= 0 else ""
    try:
        packet = json.loads(body.strip())
    except ValueError:
        raise ApiError(400, "the last user message has no position packet JSON", "no_packet", "messages") from None
    if isinstance(packet, dict) and isinstance(packet.get("packet"), dict) and "legal_moves" not in packet:
        packet = packet["packet"]
    if not isinstance(packet, dict) or not packet.get("legal_moves"):
        raise ApiError(422, "the position packet has no legal_moves", "bad_packet", "messages")
    return packet, typed


TURNS = ("play", "coach")


def check_tools(req, turn="play"):
    """Tools are fixed by the server: a request may omit them or name exactly these. A play turn
    allows the play tools only; a coach turn (chat on) shows both schemas. App tools never pass."""
    allowed = tool_names("play") + (tool_names("coach") if turn == "coach" else [])
    for i, tool in enumerate(req.tools or []):
        name = (tool.get("function") or {}).get("name") if tool.get("type") == "function" else None
        if name not in allowed:
            raise ApiError(400, f"tool {name!r} is not available in {turn} mode; the server fixes the tools to {allowed}",
                           "tool_not_allowed", f"tools[{i}]")
    choice = req.tool_choice
    if choice in (None, "auto", "required"):
        return
    name = (choice.get("function") or {}).get("name") if isinstance(choice, dict) else None
    if name not in allowed:
        raise ApiError(400, f"tool_choice {choice!r} is not available in {turn} mode", "tool_not_allowed", "tool_choice")


def chat_completion(req, p, manifest, now=None):
    """The chat.completion object for one play prediction (`p`)."""
    call = p.legal and p.move is not None
    message = {"role": "assistant", "content": None if call else p.raw_reply}
    if call:
        message["tool_calls"] = [{"id": "call_" + uuid.uuid4().hex[:24], "type": "function",
                                  "function": {"name": "submit_move", "arguments": json.dumps(
                                      {"reasoning": p.reasoning, "move": p.move}, ensure_ascii=False)}}]
    return {
        "id": "chatcmpl-" + uuid.uuid4().hex[:24], "object": "chat.completion", "created": int(now or time.time()),
        "model": req.model,
        "choices": [{"index": 0, "message": message, "finish_reason": "tool_calls" if call else "stop"}],
        "usage": {"prompt_tokens": p.input_tokens, "completion_tokens": p.output_tokens,
                  "total_tokens": p.input_tokens + p.output_tokens},
        "knightlm": {"path": p.path, "code": p.code, "raw_code": p.raw_code, "legal": p.legal,
                     "manifest_hash": manifest["manifest_hash"]},
    }


def coach_completion(req, turn, manifest, now=None):
    """The chat.completion object for one coach turn (`turn` is a model.serve.coach.CoachTurn).

    The move is never a tool call (coach has no submit_move): it rides in `knightlm.coach.move`.
    `content` is the reply, withheld after a fallback; `tool_calls` are validated coach tool calls
    for the app's executors (crates/ai/src/coach.rs)."""
    message = {"role": "assistant", "content": turn.reply or None}
    if turn.tool_calls:
        message["tool_calls"] = [
            {"id": "call_" + uuid.uuid4().hex[:24], "type": "function",
             "function": {"name": c["name"], "arguments": json.dumps(c["arguments"], ensure_ascii=False)}}
            for c in turn.tool_calls]
    legal = turn.move is not None
    from_model = legal and not turn.fallback
    return {
        "id": "chatcmpl-" + uuid.uuid4().hex[:24], "object": "chat.completion", "created": int(now or time.time()),
        "model": req.model,
        "choices": [{"index": 0, "message": message,
                     "finish_reason": "tool_calls" if turn.tool_calls else "stop"}],
        "usage": {"prompt_tokens": turn.input_tokens, "completion_tokens": turn.output_tokens,
                  "total_tokens": turn.input_tokens + turn.output_tokens},
        "knightlm": {"path": "model" if from_model else ("constrained_move" if legal else "none"),
                     "code": "ok" if legal else "no_provisional",
                     "raw_code": "ok" if from_model else "no_provisional",
                     "legal": legal, "manifest_hash": manifest["manifest_hash"],
                     "coach": {"move": turn.move, "provisional": turn.provisional, "rejected": turn.rejected,
                               "fallback": turn.fallback, "refused_calls": turn.refused_calls}},
    }


def sse(obj):
    return "data: " + (obj if isinstance(obj, str) else json.dumps(obj, ensure_ascii=False)) + "\n\n"


def chat_stream(completion, include_usage):
    """The same completion as chat.completion.chunk events, then [DONE]."""
    choice = completion["choices"][0]
    message = choice["message"]
    base = {"id": completion["id"], "object": "chat.completion.chunk", "created": completion["created"],
            "model": completion["model"]}

    def chunk(delta, finish=None, **extra):
        return dict(base, choices=[{"index": 0, "delta": delta, "finish_reason": finish}], **extra)

    text = message["content"] or ""
    calls = message.get("tool_calls") or []
    yield sse(chunk({"role": "assistant", "content": None if calls and not text else ""}))
    for i in range(0, len(text), STREAM_CHUNK_CHARS):
        yield sse(chunk({"content": text[i:i + STREAM_CHUNK_CHARS]}))
    for index, call in enumerate(calls):
        args = call["function"]["arguments"]
        yield sse(chunk({"tool_calls": [{"index": index, "id": call["id"], "type": "function",
                                         "function": {"name": call["function"]["name"], "arguments": ""}}]}))
        for i in range(0, len(args), STREAM_CHUNK_CHARS):
            yield sse(chunk({"tool_calls": [{"index": index,
                                             "function": {"arguments": args[i:i + STREAM_CHUNK_CHARS]}}]}))
    yield sse(chunk({}, choice["finish_reason"], knightlm=completion["knightlm"]))
    if include_usage:
        yield sse(dict(base, choices=[], usage=completion["usage"]))
    yield "data: [DONE]\n\n"


def build_manifest(predictor, mode, prompt="move"):
    manifest = {
        "packet_format_version": PACKET_FORMAT_VERSION,
        "default_mode": mode,
        "engine_access": False,
        **predictor.manifest(),
    }
    if prompt == "play":  # move-only manifests (and their hash) are unchanged
        manifest["default_prompt"] = "play"
    canonical = json.dumps(manifest, sort_keys=True, separators=(",", ":"))
    manifest["manifest_hash"] = hashlib.sha256(canonical.encode()).hexdigest()
    return manifest


def create_app(predictor, mode="constrained", prompt="move"):
    default_mode = mode
    if mode not in MODES:
        raise ValueError(f"mode must be one of {MODES}")
    if prompt not in PROMPTS:
        raise ValueError(f"prompt must be one of {PROMPTS}")
    manifest = build_manifest(predictor, mode, prompt)
    app = FastAPI(title="knightlm-serve")

    @app.get("/v1/health")
    def health():
        return {"status": "ok", "engine_access": False, "mode": mode}

    @app.get("/v1/manifest")
    def get_manifest():
        return manifest

    def play_prediction(req, mode):
        if not hasattr(predictor, "predict_play"):
            raise HTTPException(501, "this predictor has no play mode")
        chosen = mode or req.mode or default_mode
        explicit_raw = chosen == "raw" and (mode or req.mode) is not None
        if chosen not in MODES:
            raise HTTPException(422, f"mode must be one of {MODES}")
        try:
            prediction = predictor.predict_play(req.packet, chosen)
        except Exception as error:  # surfaced, never replaced by a guess
            raise HTTPException(500, f"inference failed: {error}") from error
        if not prediction.legal and not explicit_raw:
            raise HTTPException(502, f"play completion refused ({prediction.code}): {prediction.raw_reply!r}")
        return prediction

    @app.exception_handler(ApiError)
    def api_error(_request, error):
        return JSONResponse(error.body, status_code=error.status)

    @app.get("/v1/models")
    def list_models():
        return {"object": "list", "data": [{"id": MODEL_ID, "object": "model", "created": 0, "owned_by": "knightlm"}]}

    def coach_endpoint(req, packet, typed, mode, ext):
        """Chat on: rolling summary + recent messages + the fresh packet; the model plays first, then replies."""
        if not hasattr(predictor, "predict_coach"):
            raise ApiError(501, "this predictor has no coach mode", "not_implemented", None, "server_error")
        chosen = mode or ext.get("mode") or default_mode
        if chosen not in MODES:
            raise ApiError(422, f"mode must be one of {MODES}", "bad_mode", "knightlm.mode")
        earlier = [(m.get("role"), message_text(m)) for m in req.messages[:-1]]
        try:
            summary, recent = coach_mode.chat_history(earlier, typed, ext.get("summary"))
            turn_result = coach_mode.play_coach_turn(predictor, packet, summary, recent, chosen)
        except coach_mode.ChatError as error:
            raise ApiError(422, str(error), error.code, "messages") from error
        except Exception as error:  # surfaced, never replaced by a guess
            raise ApiError(500, f"inference failed: {error}", "inference_failed", None, "server_error") from error
        completion = coach_completion(req, turn_result, manifest)
        if req.stream:
            include = bool((req.stream_options or {}).get("include_usage"))
            return StreamingResponse(chat_stream(completion, include), media_type="text/event-stream")
        return completion

    @app.post("/v1/chat/completions")
    def chat_completions(req: ChatRequest, mode: str | None = None):
        if req.model != MODEL_ID:
            raise ApiError(404, f"the model {req.model!r} does not exist; this server serves {MODEL_ID!r}",
                           "model_not_found", "model")
        ext = req.knightlm or {}
        turn = ext.get("turn", "play")
        if turn not in TURNS:
            raise ApiError(422, f"turn must be one of {TURNS}", "bad_turn", "knightlm.turn")
        check_tools(req, turn)
        packet, typed = packet_and_text(req.messages)
        if turn == "coach":
            return coach_endpoint(req, packet, typed, mode, ext)
        if not hasattr(predictor, "predict_play"):
            raise ApiError(501, "this predictor has no play mode", "not_implemented", None, "server_error")
        chosen = mode or ext.get("mode") or default_mode
        if chosen not in MODES:
            raise ApiError(422, f"mode must be one of {MODES}", "bad_mode", "knightlm.mode")
        explicit_raw = chosen == "raw" and (mode or ext.get("mode")) is not None
        try:
            p = predictor.predict_play(packet, chosen)
        except Exception as error:  # surfaced, never replaced by a guess
            raise ApiError(500, f"inference failed: {error}", "inference_failed", None, "server_error") from error
        if not p.legal and not explicit_raw:
            raise ApiError(502, f"play completion refused ({p.code}): {p.raw_reply!r}", p.code, None, "server_error")
        completion = chat_completion(req, p, manifest)
        if req.stream:
            include = bool((req.stream_options or {}).get("include_usage"))
            return StreamingResponse(chat_stream(completion, include), media_type="text/event-stream")
        return completion

    def check_request(req):
        if req.packet_format_version != manifest["packet_format_version"]:
            raise HTTPException(400, "packet format version mismatch")
        if not req.packet.get("legal_moves"):
            raise HTTPException(422, "packet has no legal_moves")

    @app.post("/v1/play", response_model=PlayResponse)
    def play_endpoint(req: MoveRequest, mode: str | None = None):
        check_request(req)
        p = play_prediction(req, mode)
        return PlayResponse(move=p.move, reasoning=p.reasoning, code=p.code, path=p.path, raw_code=p.raw_code,
                            legal=p.legal, raw_reply=p.raw_reply, stated_move=p.stated_move,
                            manifest_hash=manifest["manifest_hash"])

    @app.post("/v1/move", response_model=MoveResponse)
    def move_endpoint(req: MoveRequest, mode: str | None = None):
        if prompt == "play":
            check_request(req)
            p = play_prediction(req, mode)
            return MoveResponse(move=p.move or "", manifest_hash=manifest["manifest_hash"],
                                legal=p.legal, raw_reply=p.raw_reply)
        if req.packet_format_version != manifest["packet_format_version"]:
            raise HTTPException(400, "packet format version mismatch")
        if not req.packet.get("legal_moves"):
            raise HTTPException(422, "packet has no legal_moves")
        chosen = mode or req.mode or default_mode  # query overrides body overrides --mode
        explicit_raw = chosen == "raw" and (mode or req.mode) is not None
        if chosen not in MODES:
            raise HTTPException(422, f"mode must be one of {MODES}")
        try:
            prediction = predictor.predict(req.packet, chosen)
        except Exception as error:  # surfaced, never replaced by a guess
            raise HTTPException(500, f"inference failed: {error}") from error
        if not prediction.legal and not explicit_raw:
            # Default path (knightlm-uci): an illegal move is never emitted.
            raise HTTPException(502, f"model produced illegal move: {prediction.raw_reply!r}")
        # A client that explicitly asks for mode=raw (the evaluation) gets the
        # untouched reply and legal=false in a 200, so illegal is a record, not an error.
        return MoveResponse(move=prediction.move, manifest_hash=manifest["manifest_hash"],
                            legal=prediction.legal, raw_reply=prediction.raw_reply)

    return app


def main():
    import uvicorn

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-dir", type=Path, required=True)
    parser.add_argument("--adapter-dir", type=Path)
    parser.add_argument("--mode", choices=MODES, default="constrained")
    parser.add_argument("--prompt", choices=PROMPTS, default="move",
                        help="what /v1/move asks the model; /v1/play always uses the play prompt")
    parser.add_argument("--port", type=int, default=8000)
    parser.add_argument("--device", default="cuda")
    args = parser.parse_args()
    predictor = Predictor(args.base_dir, args.adapter_dir, args.device)
    uvicorn.run(create_app(predictor, args.mode, args.prompt), host="127.0.0.1", port=args.port)


if __name__ == "__main__":
    main()
