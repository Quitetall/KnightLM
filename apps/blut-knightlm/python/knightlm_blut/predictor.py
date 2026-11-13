"""Predictor factory for `python -m model.eval.run --predictor knightlm_blut.predictor:factory`.

Reads KNIGHTLM_BASE_DIR and KNIGHTLM_ADAPTER_DIR (the adapter the run produced)
and returns the serving Predictor, so the evaluated model loads exactly as it
is served: base weights plus a PEFT adapter.
"""

import os


def factory():
    from model.serve.inference import Predictor

    adapter = os.environ.get("KNIGHTLM_ADAPTER_DIR") or None
    return Predictor(os.environ["KNIGHTLM_BASE_DIR"], adapter)
