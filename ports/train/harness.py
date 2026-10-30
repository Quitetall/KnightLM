"""KnightLM training port (Track A, HuggingFace). Run 1 harness.

Seams consumed (see KLM-WAR-0008):
  - model-io: chat-template packet format (state packet in -> move out)
  - run-manifest: manifest schema (run id, MLflow ref, adapter+base hashes,
    chat template, packet format version, corpus manifest ref, config)
  - cost-cap: $40/run hard input
"""

from dataclasses import dataclass, field
from typing import Iterable, Iterator, Protocol
import json, hashlib, os


@dataclass
class Packet:
    """The model-facing text contract. One decision point -> one packet."""
    prompt: str
    target: str  # the played move (primary signal)
    label: dict | None = None  # joined eval label, LABEL ONLY


def format_packet(state_fen: str, legal_moves: list[str], facts: dict) -> str:
    """Chat-template packet formatting. Exact template is the seam (model-io)."""
    return (
        "<|system|>You are a chess engine. Reply with one legal UCI move.\n"
        f"<|user|>FEN: {state_fen}\nLegal: {','.join(legal_moves)}\nFacts: {json.dumps(facts)}\n"
        "<|assistant|>"
    )


class Logger(Protocol):
    def log(self, key: str, value): ...
    def log_artifact(self, path: str): ...


class MLflowLogger:
    """System of record (sponsor decision 2026-10-20)."""

    def __init__(self, tracking_uri: str, experiment: str):
        self.tracking_uri = tracking_uri
        self.experiment = experiment
        self.run_id = None

    def log(self, key, value):
        # mlflow.log_metric / log_param in the real impl
        print(f"[mlflow] {key}={value}")

    def log_artifact(self, path):
        print(f"[mlflow] artifact {path}")


class WandbLogger:
    """Optional, behind the same seam."""

    def log(self, key, value):
        print(f"[wandb] {key}={value}")

    def log_artifact(self, path):
        print(f"[wandb] artifact {path}")


def cost_guard(spent_usd: float, cap_usd: float = 40.0) -> None:
    if spent_usd > cap_usd:
        raise RuntimeError(f"cost cap exceeded: {spent_usd:.2f} > {cap_usd:.2f}")


@dataclass
class RunManifest:
    run_id: str
    mlflow_ref: str
    adapter_hash: str
    base_hash: str
    chat_template: str
    packet_format_version: int
    corpus_manifest_ref: str
    config: dict = field(default_factory=dict)

    def to_json(self) -> str:
        return json.dumps(self.__dict__, indent=2)


def sha256_file(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


# ---------------------------------------------------------------------------
# Eval report: per-axis only. The generator refuses composites.
# ---------------------------------------------------------------------------

AXES = ["raw_legality", "masked_legality", "token_cost", "assisted_legality", "strength_proxy_sf19"]


def build_report(results: dict[str, float]) -> dict:
    missing = [a for a in AXES if a not in results]
    if missing:
        raise ValueError(f"missing axes: {missing}")
    extra = [k for k in results if k not in AXES]
    if extra:
        raise ValueError(f"unknown/composite axes refused: {extra}")
    # refuse any key that looks like a composite
    for k in results:
        if any(tok in k for tok in ("total", "score", "winner", "composite", "overall")):
            raise ValueError(f"composite axis refused: {k}")
    return {"axes": {k: results[k] for k in AXES}, "composite": None}


if __name__ == "__main__":
    r = build_report({
        "raw_legality": 0.91,
        "masked_legality": 1.0,
        "token_cost": 12.3,
        "assisted_legality": 0.97,
        "strength_proxy_sf19": 0.42,
    })
    print(json.dumps(r, indent=2))
