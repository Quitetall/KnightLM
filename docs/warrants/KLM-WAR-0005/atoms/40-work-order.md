---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e87b-701d-bd00-f083e15c85ff
role: work_order
jurisdiction: authored
order: 40
classification: internal
---

# Work Order

## Deliverables

1. New crate `code/crates/engine/` (`knightlm-engine`): an async (tokio) UCI
   client — spawn SF by `STOCKFISH_PATH`, handshake (uci/uciok,
   isready/readyok), `position`, `go depth N` / `go nodes`, MultiPV, and a
   stop/restart-safe teardown. Analysis results carry the engine version and
   requested depth in type, not by comment.
2. Label schema (`Label`): normalized position key, best move, cp or mate
   value, pv, depth, engine name+version, tie policy (highest depth wins).
   Serialization versioned.
3. Labeling harness: takes an eval-set position file, produces label JSONL in
   one resumable batch run; long jobs shard across processes.
4. Gate definitions under `docs/gates/`: a cargo test/fmt/clippy gate for the
   new and changed crates, with its plant battery, as a qualified local
   candidate like the existing war-check gate.

## Seams and who depends on them

1. **SEAM `engine-analysis`** — `knightlm_engine::Engine::analyse(pos, depth)
   -> Result<Analysis>`. Consumers: KLM-WAR-0007 (accuracy joins),
   KLM-WAR-0008 (strength axis), KLM-WAR-0010 (viewer analysis panel:
   Sf19 d15 on any position shown; entirely outside any model path).
2. **SEAM `label-row`** — the Label row shape as written by the harness and
   read by W-0007's joins and W-0008's eval. Consumers: 0007, 0008.
3. **Contract `STOCKFISH_PATH`** — documented env contract; no binary search,
   no download. Consumers: MLOps (KLM-WAR-0009 deployment).
4. **Seam `engine-zero`** — crates/engine exposes no symbol reachable from
   knightlm-model, the inference server or packet compilation. The boundary
   is structural and checked, not sworn.

## Owner's space

Process-pool design, restart policy, MultiPV bookkeeping, batch scheduling,
gate battery authorship, error naming. Consumers see only Analysis values,
Label rows and the boundary's absence.

## Premade instructions

- Every label row must name engine + version + depth; a label without them is
  a defect, not a row.
- The engine sets quotes and analysis output for humans is the viewer's job —
  the client returns data, the panel formats it.

## Autonomy and escalation

Tier T2. Escalate any dependency that would place the engine inside a model
path (that is a sponsor-level architecture question, by construction this
one cannot arise inside the contracts, so escalation is a backstop).

## Rollback

Delete `code/crates/engine/`, the gate YAMLs, and any docs referencing the
seams; no other crate imported it by contract.
