---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e8ba-73df-97ed-ca7d7a99d0c9
role: work_order
jurisdiction: authored
order: 40
classification: internal
---

# Work Order

## Deliverables

1. `code/model/serve/` — FastAPI server (uv-managed): loads base+adapter per
   the run manifest, exposes `POST /v1/move` (state packet in, move out as
   SAN + inner UCI tokens), `/v1/health`, `/v1/manifest`; requests are
   version-checked against `model-io` (packet format version from the
   manifest), and the server exposes NOTHING engine-related (no analysis
   route, no score route).
2. Deployment scripts (MLOps lane): env file listed in code/README.md; the
   model binary and adapter come from the run manifest paths, never a
   download at startup; a refuse-to-start if hashes mismatch.
3. `ModelPlayer` — registered behind knightlm-uci's Player seam: HTTP
   client to the server,ean translate the state to a packet, apply the
   replayed response; on garbage/absence of a reply, the player FAILS
   explicitly (resign by explicit resign path in the driver transcript)
   rather than falling back to any legal move.
4. The viewer panel's model-move button calls the ZERO-basket waterfall:
   ModelPlayer (sever) first; SF analysis stays a separate panel owned by
   KLM-WAR-0005's seam; the two never combine scores.
5. Demos: a session script where the model plays SF19 at fixed depth as the
   opponent and the driver records everything.

## Seams and who depends on them

1. **SEAM `serve-http`** — the request/response schema + packet format
   version header. Consumers: any Player or harness (knights to this is how
   KnightLM's model plays), KLM-WAR-0010 (panel), and later a hosted Gate
   scoreboard.
2. **SEAM `player-model`** — the ModelPlayer registration (trait from
   KLM-WAR-0006). Consumers: UCI binary (knightlm-uci), the scripted game
   driver.
3. Frozen without change: knightlm-uci's UCI subset, the packet compiler
   surfaces, the engine seams from W-0005.

## Owner's space

Server framework details (FastAPI middlewares, worker counts), batching and
request timeouts, caching of state hashes, deployment layout, logging
shapes. The contract is the schema plus fail-loud behavior; how fast it is
is quality work the owner controls.

## Premade instructions

- The server contains no engine import — the OBL check greps manifests and
  source for any path to crates/engine and refuses green on a hit.
- A model failure is a FAILURE: a wrong/garbage/ineligible response surfaces
  as an error event, logged and visible in the driver transcript; a silent
  legal-move fallback would fail review by construction.

## Autonomy and escalation

Tier T2. Escalation: anything changing the HTTP contract shape (consumers:
viewer + any future scoreboard), or placing any engine call inside the
server (forbidden by construction; escalation is the failure backstop).

## Rollback

Delete `code/model/serve/` and the ModelPlayer module; the UCI binary falls
back to RandomLegal (a Player removal is safe by contract).
