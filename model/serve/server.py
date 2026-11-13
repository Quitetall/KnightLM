"""HTTP serving port (KLM-WAR-0009) for knightlm-uci / knightlm-match.

Contract (crates/cli/src/players.rs, ModelPlayer):
  POST /v1/move  {packet: <ContextPacket object>, packet_format_version: int}
                 -> {move, manifest_hash}
  GET  /v1/health, GET /v1/manifest

Engine-free by construction (KLM-WAR-0005): no engine is imported or reached.
Failures are HTTP errors; there is no fallback move of any kind.

  python -m model.serve.server --base-dir DIR [--adapter-dir DIR] \
      [--mode raw|constrained] [--port 8000]
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


class MoveResponse(BaseModel):
    move: str
    manifest_hash: str


def build_manifest(predictor, mode):
    base = predictor.base_dir
    manifest = {
        "packet_format_version": PACKET_FORMAT_VERSION,
        "mode": mode,
        "engine_access": False,
        "base_sha256": predictor.base_sha256,
        "adapter_sha256": predictor.adapter_sha256 or "none",
        "prompt_sha256": sha256_file(ROOT / "prompt.py"),
        "tokenizer_sha256": sha256_file(base / "tokenizer.json"),
        "chat_template_sha256": sha256_file(base / "chat_template.json"),
    }
    canonical = json.dumps(manifest, sort_keys=True, separators=(",", ":"))
    manifest["manifest_hash"] = hashlib.sha256(canonical.encode()).hexdigest()
    return manifest


def create_app(predictor, mode="constrained"):
    if mode not in MODES:
        raise ValueError(f"mode must be one of {MODES}")
    manifest = build_manifest(predictor, mode)
    app = FastAPI(title="knightlm-serve")

    @app.get("/v1/health")
    def health():
        return {"status": "ok", "engine_access": False, "mode": mode}

    @app.get("/v1/manifest")
    def get_manifest():
        return manifest

    @app.post("/v1/move", response_model=MoveResponse)
    def move_endpoint(req: MoveRequest):
        if req.packet_format_version != manifest["packet_format_version"]:
            raise HTTPException(400, "packet format version mismatch")
        if not req.packet.get("legal_moves"):
            raise HTTPException(422, "packet has no legal_moves")
        try:
            prediction = predictor.predict(req.packet, mode)
        except Exception as error:  # surfaced, never replaced by a guess
            raise HTTPException(500, f"inference failed: {error}") from error
        if not prediction.legal:
            raise HTTPException(502, f"model produced illegal move: {prediction.raw_reply!r}")
        return MoveResponse(move=prediction.move, manifest_hash=manifest["manifest_hash"])

    return app


def main():
    import uvicorn

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-dir", type=Path, required=True)
    parser.add_argument("--adapter-dir", type=Path)
    parser.add_argument("--mode", choices=MODES, default="constrained")
    parser.add_argument("--port", type=int, default=8000)
    parser.add_argument("--device", default="cuda")
    args = parser.parse_args()
    predictor = Predictor(args.base_dir, args.adapter_dir, args.device)
    uvicorn.run(create_app(predictor, args.mode), host="127.0.0.1", port=args.port)


if __name__ == "__main__":
    main()
