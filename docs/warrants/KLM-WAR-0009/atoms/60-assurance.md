---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e8ba-73df-97ed-ca7d7a99d0c9
role: assurance
jurisdiction: authored
order: 60
classification: internal
---

# Assurance

## Acceptance Obligations

### OBL-001 — serving matches the offline eval, packet in -> move out
- **scope:** the 10K-holdout sample through /v1/move at serving scale (a subset replayed)
- **evidence:** per-position legality rendered at serving equals the offline eval within
reporting margins; the manifest hash chain proves the same checkpoint as
run 1
### OBL-002 — model failures surface, never auto-repair
- **scope:** planted garbage server responses (empty, malformed SAN, invented move)
- **evidence:** each case is logged and surfaced as a failed driver turn; zero silent
fallback to RandomLegal (RQ-010)
### OBL-003 — engine-zero: the serving stack has no engine path
- **scope:** code/model/serve manifests + source; scripts
- **evidence:** a repo test asserting zero imports/dependencies from serve to crates/engine;
the check is part of the cargo test battery
### OBL-004 — the model plays a full session vs SF19 through the stack
- **scope:** the scripted session log with driver transcript
- **evidence:** driver transcript recorded: model color played to completion against SF19 at
its fixed depth; failures worth noting kept visible in the log
### OBL-005 — the consumer plays the model without the author
- **scope:** an evaluation lane member plays one session through knightlm-uci against
the server
- **evidence:** evaluation lane's recorded session; no queries to this Warrant's author

## Gate Adequacy

Required at `basic`.

**Adversarial question:** Could the server keep a legal-move fallback 'for demo stability' and erase the refusal trace?

- **outcome:** no — planted garbage responses must surface; the transcript shows every failure and a fallback would be a refusal finding.

**Adversarial question:** Could an engine call creep into serving through a 'sanity check'?

- **outcome:** no — OBL-003 is a structural check; no import path exists to reach one.


## Residual Risk

Serving throughput on a single GPU is adequate for demos but not for batched
rating tournaments; report the number, do not promise it. FastAPI latency vs
driver timeouts is calibrated at M1. The 'demo' part of a session-vs-SF19 is
narrative, not a strength claim — the strength axis stays in W-0008.
