"""Predictor loading. Code against the serve interface, test with fakes.

A predictor is any object with `predict(packet: dict, mode: str) -> Prediction`
(mode "raw" or "constrained"). Specs:

  pkg.module:factory     imports pkg.module and calls factory() for the predictor
  http://host:port[/v1/move]   POST the packet to a server speaking the move contract
"""

import importlib
import json
import time
import urllib.error
import urllib.request

try:  # the serve lane's real type
    from model.serve.inference import Prediction, Predictor  # noqa: F401
except ImportError:  # until it lands, an interface-identical stand-in
    from dataclasses import dataclass

    @dataclass
    class Prediction:
        move: str
        raw_reply: str
        legal: bool
        input_tokens: int
        output_tokens: int
        latency_ms: float

    class Predictor:  # documentation of the interface only
        def predict(self, packet: dict, mode: str) -> "Prediction":
            raise NotImplementedError

MODES = ("raw", "constrained")


class HttpPredictor:
    """POST {packet, packet_format_version, mode} to /v1/move; failures raise."""

    def __init__(self, url, timeout_s=120.0):
        base = url.rstrip("/")
        self.move_url = base if base.endswith("/v1/move") else base + "/v1/move"
        self.base = self.move_url[: -len("/v1/move")]
        self.timeout_s = timeout_s

    def predict(self, packet, mode):
        body = json.dumps({"packet": packet, "packet_format_version": packet["schema_version"],
                           "mode": mode}).encode()
        request = urllib.request.Request(self.move_url, data=body,
                                         headers={"Content-Type": "application/json"})
        started = time.monotonic()
        with urllib.request.urlopen(request, timeout=self.timeout_s) as response:
            data = json.loads(response.read())
        wall = (time.monotonic() - started) * 1000
        move = data.get("move")
        return Prediction(
            move=move, raw_reply=data.get("raw_reply", move), legal=bool(data.get("legal", False)),
            input_tokens=data.get("input_tokens"), output_tokens=data.get("output_tokens"),
            latency_ms=data.get("latency_ms", wall),
        )

    def manifest(self):
        try:
            with urllib.request.urlopen(self.base + "/v1/manifest", timeout=10) as response:
                return json.loads(response.read())
        except (urllib.error.URLError, OSError, ValueError) as error:
            return {"unavailable": str(error)}


def load_predictor(spec, prompt_kind="move"):
    """Return (predictor, description). A play kind (play, play-cgt, play-cgt-full) speaks that play prompt
    (model.eval.playmode); a python predictor is told which one through its `prompt_kind` attribute."""
    from model.prompt import PLAY_KINDS
    play = prompt_kind in PLAY_KINDS
    if play:
        from model.eval.playmode import HttpPlayPredictor, PlayAdapter
    if spec.startswith(("http://", "https://")):
        return (HttpPlayPredictor(spec) if play else HttpPredictor(spec)), {"kind": "http", "url": spec}
    module_name, _, attr = spec.partition(":")
    if not attr:
        raise ValueError("python predictor spec must be 'module:factory'")
    factory = getattr(importlib.import_module(module_name), attr)
    predictor = factory()
    if play and hasattr(predictor, "predict_play"):
        if prompt_kind != "play" and not hasattr(predictor, "prompt_kind"):
            raise ValueError(f"predictor {spec!r} cannot speak prompt kind {prompt_kind!r} (no prompt_kind attribute)")
        if hasattr(predictor, "prompt_kind"):
            predictor.prompt_kind = prompt_kind
        predictor = PlayAdapter(predictor)
    return predictor, {"kind": "python", "spec": spec}


def predictor_manifest(predictor):
    getter = getattr(predictor, "manifest", None)
    if getter is None:
        return None
    value = getter() if callable(getter) else getter
    return value
