"""Play-mode predictors for model.eval: the prompt a reasoning-corpus adapter is trained on.

A play predictor answers `predict(packet, mode)` with a `model.serve.inference.PlayPrediction`
(move, reasoning, completion code, path, tokens, latency). Nothing here reaches an engine.

  --predictor model.eval.playmode:from_env     serve Predictor from KNIGHTLM_BASE_DIR
                                               (+ KNIGHTLM_ADAPTER_DIR, KNIGHTLM_DEVICE; with
                                               KNIGHTLM_ENCODER_DIR and KNIGHTLM_PROJECTOR_DIR the board
                                               arms, prompt kinds play-enc and play-enc-cgt)
  --predictor model.eval.playmode:head_from_env   the same plus a move head from KNIGHTLM_HEAD_DIR
                                               (prompt kind play-head; also the --move-head axis)
  --predictor http://host:port                 POST /v1/play (with --prompt-kind play)
"""

import json
import os
import time
import urllib.request

from model.eval import explain as explain_module
from model.serve.inference import PlayPrediction


class PlayAdapter:
    """Wrap an object with predict_play(packet, mode) as a play-kind predictor."""

    def __init__(self, inner):
        self.inner = inner

    def predict(self, packet, mode):
        return self.inner.predict_play(packet, mode)

    def manifest(self):
        getter = getattr(self.inner, "manifest", None)
        return getter() if callable(getter) else None

    def __getattr__(self, name):
        # `logits` (the move_head axis) is the inner head predictor's, exposed only when it has one.
        if name == "logits" and callable(getattr(self.__dict__.get("inner"), "logits", None)):
            return self.inner.logits
        raise AttributeError(name)


class HttpPlayPredictor:
    """POST {packet, packet_format_version, mode} to /v1/play. A refused completion asked for
    in raw mode is a 200 record (code says why); any HTTP error raises and is scored as one."""

    def __init__(self, url, timeout_s=300.0):
        base = url.rstrip("/")
        for tail in ("/v1/play", "/v1/move"):
            if base.endswith(tail):
                base = base[: -len(tail)]
        self.base = base
        self.play_url = base + "/v1/play"
        self.move_url = base + "/v1/move"  # the match driver; the server must run with --prompt play
        self.timeout_s = timeout_s

    def predict(self, packet, mode):
        body = json.dumps({"packet": packet, "packet_format_version": packet["schema_version"],
                           "mode": mode}).encode()
        request = urllib.request.Request(self.play_url, data=body, headers={"Content-Type": "application/json"})
        started = time.monotonic()
        with urllib.request.urlopen(request, timeout=self.timeout_s) as response:
            data = json.loads(response.read())
        wall = (time.monotonic() - started) * 1000
        return PlayPrediction(
            move=data.get("move"), reasoning=data.get("reasoning"), raw_reply=data.get("raw_reply", ""),
            legal=bool(data.get("legal", False)), code=data["code"], path=data["path"],
            raw_code=data.get("raw_code", data["code"]), stated_move=data.get("stated_move"),
            input_tokens=data.get("input_tokens"), output_tokens=data.get("output_tokens"),
            latency_ms=data.get("latency_ms", wall), head_move=data.get("head_move"))

    def manifest(self):
        try:
            with urllib.request.urlopen(self.base + "/v1/manifest", timeout=10) as response:
                return json.loads(response.read())
        except (OSError, ValueError) as error:
            return {"unavailable": str(error)}


def from_env():
    """A serve Predictor (base, optionally plus an adapter and a board encoder + projector) wrapped for play mode."""
    from model.serve.inference import Predictor
    adapter = os.environ.get("KNIGHTLM_ADAPTER_DIR") or None
    return PlayAdapter(Predictor(os.environ["KNIGHTLM_BASE_DIR"], adapter, os.environ.get("KNIGHTLM_DEVICE", "cuda"),
                                 encoder_dir=os.environ.get("KNIGHTLM_ENCODER_DIR") or None,
                                 projector_dir=os.environ.get("KNIGHTLM_PROJECTOR_DIR") or None))


def head_from_env():
    """A serve HeadPredictor (base + adapter + move head from KNIGHTLM_HEAD_DIR) wrapped for play mode."""
    from model.serve.head import HeadPredictor
    adapter = os.environ.get("KNIGHTLM_ADAPTER_DIR") or None
    return PlayAdapter(HeadPredictor(os.environ["KNIGHTLM_BASE_DIR"], adapter, os.environ.get("KNIGHTLM_DEVICE", "cuda"),
                                     head_dir=os.environ["KNIGHTLM_HEAD_DIR"]))


def head_agreement_axis(records, modes):
    """Per mode: how often the completion agrees with the move head's pick (play-head kind).

    `agreed` is a completion that passes every check with the head's move (code ok, move == head move); every
    row is in the denominator, a row with no head pick counts as not agreed. No repair is counted as agreement."""
    axis = {"label": "human-only move head"}
    for mode in modes:
        n = len(records)
        entries = [r[mode] for r in records]
        agreed = sum(e.get("play_code") == "ok" and e.get("move") is not None and e.get("move") == e.get("head_move")
                     for e in entries)
        by_code = {}
        for e in entries:
            if e.get("head_move") is not None:
                by_code[e.get("play_code")] = by_code.get(e.get("play_code"), 0) + 1
        hits = sum(e.get("head_move") == r["target"] for e, r in zip(entries, records, strict=True))
        axis[mode] = {"n": n, "rows_with_head_move": sum(e.get("head_move") is not None for e in entries),
                      "agreed": {"count": agreed, "n": n, "rate": round(agreed / n, 4) if n else None},
                      "head_move_matches_target": {"count": hits, "n": n, "rate": round(hits / n, 4) if n else None},
                      "by_code": dict(sorted(by_code.items()))}
    return axis


def explanation_axis(rows, records, modes):
    """Check the generated reasoning against the board, per mode; fills entry["explanation"].

    A completion with no recoverable reasoning is `missing` and stays in the denominator. The
    move handed to the checker (for "gives check" sentences) is the committed move when legal."""
    axes = {}
    for mode in modes:
        per_row = []
        for row, record in zip(rows, records, strict=True):
            entry, packet = record[mode], row["packet"]
            text = entry.get("reasoning")
            move = entry["move"] if entry["status"] == "ok" else None
            result = None if not isinstance(text, str) else explain_module.check_explanation(
                packet["fen"], packet["legal_moves"], text, move)
            entry["explanation_check"] = result
            per_row.append(result)
        axes[mode] = explain_module.aggregate(per_row) | {"source": "play reasoning field"}
    return axes
