"""FastAPI serving port (KLM-WAR-0009). Engine-free by construction."""

from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
import hashlib, json, os

app = FastAPI(title="knightlm-serve")

MANIFEST_PATH = os.environ.get("KNIGHTLM_MANIFEST", "run-manifest.json")


class MoveRequest(BaseModel):
    packet: str  # model-io chat-template packet (versioned)
    packet_format_version: int


class MoveResponse(BaseModel):
    move: str
    manifest_hash: str


@app.get("/v1/health")
def health():
    return {"status": "ok", "engine_access": False}


@app.get("/v1/manifest")
def manifest():
    with open(MANIFEST_PATH) as f:
        m = json.load(f)
    return m


@app.post("/v1/move", response_model=MoveResponse)
def move_endpoint(req: MoveRequest):
    with open(MANIFEST_PATH) as f:
        m = json.load(f)
    if req.packet_format_version != m.get("packet_format_version"):
        raise HTTPException(400, "packet format version mismatch")
    # Model inference happens here (transformers). No engine is imported or
    # called anywhere in this module — enforced by the repo test in W-0005/OBL-003.
    # Placeholder: return a refusal rather than a fake move.
    raise HTTPException(501, "model inference not wired in this scaffold")
EOF
echo written