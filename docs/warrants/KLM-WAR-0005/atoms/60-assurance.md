---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e87b-701d-bd00-f083e15c85ff
role: assurance
jurisdiction: authored
order: 60
classification: internal
---

# Assurance

## Acceptance Obligations

### OBL-001 — the client analyzes fixture positions at fixed depth with stamped output
- **scope:** a fixture eval battery (hand-chosen FENs): handshake, `go depth N`, MultiPV,
unresponsive-engine kill/restart
- **evidence:** test log: per-position best move + value + pv + depth + engine name+version;
restart exercised by the battery (kill mid-search, re-handshake)
### OBL-002 — a missing STOCKFISH_PATH is an explicit operator error
- **scope:** no SF binary in PATH and no env for the client
- **evidence:** the client refuses to start with a message naming STOCKFISH_PATH; no
download attempt leaves a trace
### OBL-003 — harness labels are stamped and resumable
- **scope:** an eval-set sample through the harness
- **evidence:** every row carries engine+version+depth; a run resumed mid-batch produces the
same rows as an uninterrupted one
### OBL-004 — engine-zero: no path from model inference to the engine
- **scope:** `knightlm-model`, the inference server and packet compilation must not
import crates/engine or link UCI tooling
- **evidence:** dependency scan over workspace manifests (cargo tree --no-default-features
evidence in repo docs) naming zero paths; the check itself is a repo test
### OBL-005 — consumer check: the viewer lane's analysis panel runs
- **scope:** the KLM-WAR-0010 integration stage uses engine-analysis unaided
- **evidence:** cross-lane consumer check recorded from the viewer stage

## Gate Adequacy

Required at `basic`.

**Adversarial question:** Could a model's inference call an engine through a label or PV?

- **outcome:** no — OBL-004 proves crates/engine is unreachable from the model path; the seam lives outside inference by construction, not honor.

**Adversarial question:** Could labels emerge unversioned or inconsistent and still flow into joins?

- **outcome:** no — OBL-003 makes stamping contract-level, with resume-equality proving consistency.


## Residual Risk

Stockfish behavior can differ across builds/hardware; pinned SF 19 and a
fixture battery keep surface drift visible. Async teardown races are the
crash risk the battery exercises. Gate-battery authorship quality is a
known limitation the admins' own known_blind_spots list is honest about —
detection claims are scoped to the mutations planted.
