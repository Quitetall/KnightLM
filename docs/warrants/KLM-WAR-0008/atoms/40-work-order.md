---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-e8aa-763c-85bd-1aecc0ffb5f7
role: work_order
jurisdiction: authored
order: 40
classification: internal
---

# Work Order

## Deliverables

1. `code/model/train/` (python, uv-managed): dataset loader over
   `corpus-shards`; SmolVLM-500M chat-template packet formatting (the
   model-facing text contract); LoRA config and the training loop; logging
   through one interface with an MLflow implementation (system of record)
   and an optional W&B implementation.
2. Run-scripts for RunPod A100 (single GPU, 12-18h), with cost caps checked
   against the account's actual balance metric and a checkpoint-per-epoch
   resume.
3. Eval harness + report: on the 10K holdout, compiler-checked legality
   (raw + masked + assisted), token-cost measured against the tokenizer,
   strength proxy via fixed-depth SF19 from the W-0005 label harness, all
   as separate axes. Report generator refuses any composite axis.
4. The lane's process artifact: the converged harness after member drafts +
   discussion, logged as one internal note; run-1 receipts (cost breakdown,
   run ids, wall time) in the Warrant's evidence directory.

## Seams and who depends on them

1. **SEAM `model-io`** — the chat-template packet format: exactly how a
   decision-point packet goes in and a move comes out. Consumers:
   KLM-WAR-0009 (serving) loads it from the run manifest; the eval harness;
   KLM-WAR-0011 (Track B parity targets the same format).
2. **SEAM `run-manifest`** — manifest: run id, MLflow ref, adapter+base
   checkpoint hashes, chat template, packet format version, corpus manifest
   ref, config. Consumers: W-0009 serving, sponsor lineage, reruns.
3. **Contract `cost-cap`** — $40/run as a hard input; a run that busts the
   cap is stopped by script, not by hope: costs are printed at checkpoints.
4. Frozen without change: the corpus shard and packet formats (0007 seams).

## Owner's space

Hyperparameters within the cap, LoRA rank/alpha, learning rate, epochs, batch
composition, sampler and shard-order strategy, eval-report presentation. The
axes' definitions are the sponsor's; their measurement is ours.

## Premade instructions

- Every axis is reported per axis, with its evidence bundle; a report with a
  composite or a missing axis fails review by construction (the generator
  refuses both).
- Drafts-then-converge: the ML lane members write their own short harness
  draft first, then one converges; the process is recorded, not performed
  from memory.

## Autonomy and escalation

Tier T2 within the cap and the pinned models; busting the cap, changing exit
gates, or adding composite axes escalates to the sponsor.

## Rollback

Down-run scripts are resumable checkpoints; killing a run costs the elapsed
GPU-minutes only. The harness code stays on the Lane's branch until the run
verdict is in.
