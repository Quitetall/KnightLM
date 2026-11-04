---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e8ba-73df-97ed-ca7d7a99d0c9
role: intent
jurisdiction: authored
order: 10
classification: internal
---

# Intent

Wire the trained model into playable surfaces: a Python inference server
(FastAPI + transformers) that loads the run-1 checkpoint and speaks one
versioned packet contract; a `ModelPlayer` behind knightlm-uci's Player
seam; the viewer's model-move button — with the engine boundary intact
throughout (the server never asks Stockfish or any engine during a
position's evaluation; the model's inference path stays engine-free,
KLM-SAS-RQ-009).

## Out of scope

- Burn/candle native inference — KLM-WAR-0011 and later training Warrants.
- Training itself, hyperparameters — KLM-WAR-0008.
