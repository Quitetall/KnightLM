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

from fastapi import FastAPI, HTTPException
from pydantic import BaseModel

from model.corpus.pipeline import sha256_file
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
