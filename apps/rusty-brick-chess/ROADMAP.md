# RustyBrickChess roadmap

RBC is the app. KnightLM is the chess model being trained for it. The app and
model can advance in parallel through the same player interface.

## RBC app and release

| Priority | Work remaining | Done when |
| --- | --- | --- |
| 1 | Rebuild and verify the release bundle with the approval refresh fix on `main`. | A clean run of all browser checks passes against the packaged build. |
| 2 | Package Windows and macOS and test on native hosts. | An installer launches RBC with its frontend, opens or creates the per-user database, and passes a PGN import and analysis smoke. Publish the tested packages. |
| 3 | Make **Stop** responsive when a model provider stops sending data. | A browser check cancels a stalled provider within a bounded time and the composer becomes usable again. |
| 4 | Improve opening-explorer latency. | The same million-game benchmark shows a faster query and the returned continuations match. The recorded reference is about 1.3 seconds. |

The Linux browser app already has a source installer. The Windows source
quickstart and desktop packaging status are in the [running guide](../../docs/running.md).
See the [release candidate record](../../docs/release-candidate.md) for the
previous build's evidence and [benchmarks](../../docs/benchmarks.md) for the
opening-explorer measurement.

## KnightLM model

1. **Serve the pilot adapter.** Load the pinned SmolVLM base and LoRA adapter
   from the run manifest. Register KnightLM as a player in RBC and
   `knightlm-uci`, and surface invalid moves as failures.
2. **Build the full Run 1 corpus.** Produce the 200,000-game training set and
   10,000-game temporal holdout with source provenance and leakage audits.
   Keep teacher labels separate from the packet a player receives.
3. **Train and evaluate.** Compare game history, history plus state, and fresh
   state on held-out games. Report legality, play strength and tokenizer cost
   separately. The existing 1,000-game pilot used text packets with legal
   choices and verified that the adapter reloads.
4. **Extend the model.** Evaluate board vision, prove the Burn backend against
   the reference, and qualify larger Qwen checkpoints separately.

[Training quickstart](../../ports/README.md#lichess-lora-pilot) ·
[Research plan](../../docs/plan.md) · [Architecture spec](../../docs/sas/KnightLM_SAS.md#98-implementation-phases)
