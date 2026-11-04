---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e87b-701d-bd00-f083e15c85ff
role: intent
jurisdiction: authored
order: 10
classification: internal
---

# Intent

Put the engine boundary in the system as tooling, not as folklore: a tokio
UCI client for Stockfish 19, a labeling harness that stamps engine version
and depth on every label it produces, and the repo's first cargo gate
definitions — so the corpus joins labels, the eval harness scores against a
fixed opponent, and the viewer gets an analysis panel, all through one iron
rule: the model's inference path never reaches an engine (KLM-SAS-RQ-009).

## Out of scope

- knightlm-uci and the model player — KLM-WAR-0006/0009.
- Which positions get labeled and how splits are audited — KLM-WAR-0007.
