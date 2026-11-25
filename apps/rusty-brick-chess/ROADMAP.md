# RustyBrickChess and KnightLM roadmap

RBC is the chess app. KnightLM is the model path: corpus, tokenizer, training, evaluation
and serving. They advance in parallel and meet at the OpenAI-compatible
`/v1/chat/completions` endpoint ([the seam](../../docs/model/seam.md)). Last reviewed against
`main` at the merge of PR #76 (2026-11-24). Open work is tracked as tickets: see
[work orders](../../docs/contributors/work/README.md) and `war view prime`.

## What ships today

| Area | State | Where to read |
| --- | --- | --- |
| RBC on Linux (browser app, source installer) | Works. Import PGNs, search, Stockfish analysis, studies, arena, MCP with an approval queue | [Running RBC](../../docs/app/running.md) |
| RBC desktop app (Tauri) | Pre-release installers on GitHub (latest `v0.1.0-prealpha.4`): Linux AppImage, deb, rpm and Windows setup.exe, msi. Unsigned. Only Linux hand-tested; no macOS build (GitHub gave no runner). Stockfish 19 bundled. CI lints, builds and smoke-runs the shell on Linux | [Install](../../docs/app/running.md#install-a-release), [releasing](../../docs/app/releasing.md) |
| Play AI | Built-in test bot works at once; the real KnightLM model is opt-in (**Get the model**, about 1.4 GB). The published Run 1 short adapter is about random strength and does not chat | [Play AI](../../docs/app/models.md#play-ai-one-click-opponent) |
| UI redesign | Draft PR #14, blocked on the human check RBCUI-LIB-09. Not on `main`, which still has the 13-tab interface | none |
| Model providers in RBC | Any OpenAI-compatible server; provider settings, match seats, a chat toggle and coach marks on the board; a **KnightLM server** preset | [Using a model](../../docs/app/models.md) |
| KnightLM serving | `model/serve`: `/v1/chat/completions` (play and coach turns, raw or constrained decoding), `/v1/move`, `/v1/play`; `knightlm-uci` for UCI GUIs. Checked by tests and a game gate over a fake predictor | [Seam](../../docs/model/seam.md), [scorecard](../../docs/model/scorecard.md#the-knightlm-uci-game-gate) |
| KnightLM evaluation | Scorecard with separate axes and no composite score; Stockfish, lc0 and Maia are post-decision oracles | [Scorecard](../../docs/model/scorecard.md) |
| KnightLM training | Only Run 1 short has trained (625 steps, 20,000 rows, SmolVLM-500M LoRA). Every other arm is prepared with a CPU preflight and waits for the owner's go. No strength claim | [Training gate](../../docs/model/training-gate.md) |

The Linux evidence is the [release candidate record](../../docs/releases/release-candidate.md)
(commit `ec50e82`, before the provider and KnightLM work above) and the
[benchmarks](../../docs/releases/benchmarks.md). Windows is unverified by a person and macOS has
no build. Strength
numbers exist only for the short run and describe wiring, not rating.

## RBC app and release

| Priority | Work remaining | Done when | Ticket |
| --- | --- | --- | --- |
| 1 | Rebuild and verify the release bundle with the approval refresh fix on `main`, with an isolated browser gate. | A clean run of all browser checks passes against the packaged build. | [t-5b6e](../../docs/work/t-5b6e/t-5b6e-qa-engineer-isolate-the-rbc-browser-release-gate.md) |
| 2 | Test the Windows installers on a native host and obtain a macOS build (needs a runner or a Mac). | An installer launches RBC with its frontend, opens or creates the per-user database, and passes a PGN import and analysis smoke. Sign them if signing is wanted. | [t-1ce9](../../docs/work/t-1ce9/t-1ce9-release-engineer-qualify-rbc-on-native-windows.md), [t-a226](../../docs/work/t-a226/t-a226-windows-tester-try-the-rbc-source-launcher-on.md) |
| 3 | Make **Stop** responsive when a model provider stops sending data. | A browser check cancels a stalled provider within a bounded time and the composer becomes usable again. | [t-c168](../../docs/work/t-c168/t-c168-frontend-engineer-make-stop-responsive-during-a.md) |
| 4 | Improve opening-explorer latency. | The same million-game benchmark shows a faster query and the returned continuations match. The recorded reference is about 1.3 seconds. | none yet |

## KnightLM model

1. **Finish the Run 1 corpus.** The 200,000-game training set and 10,000-game temporal holdout
   with source provenance and leakage audits; three of four items are done
   ([t-3458](../../docs/tickets/t-3458.md)). Inventory the sources
   ([t-a475](../../docs/work/t-a475/t-a475-data-engineer-inventory-sources-for-run-1.md)) and build a
   repeatable ETL loop ([t-f724](../../docs/work/t-f724/t-f724-data-engineer-build-a-repeatable-chess-etl-loop.md)).
   Teacher labels stay separate from the packet a player receives.
2. **Agree the corpus-to-trainer contract and test the joined loop**
   ([t-5cd8](../../docs/work/t-5cd8/t-5cd8-representation-evaluation-engineer-agree-on-the.md),
   [t-ab7d](../../docs/work/t-ab7d/t-ab7d-mlops-cv-engineer-test-the-joined-source-to.md)).
3. **Implement the held-out scorecard run** that compares history, history plus state and fresh
   state ([t-c199](../../docs/work/t-c199/t-c199-representation-evaluation-engineer-implement-the.md)),
   under the [strength baseline protocol](../../docs/model/strength-baseline.md). Report legality,
   play strength and token cost separately.
4. **Train the arms in the [training gate](../../docs/model/training-gate.md) order**, each GPU
   launch on the owner's explicit go: S0 and graph-text controls, the `ctok/1` and board-encoder
   arms (T1, T3, T4, T5), the move head, reasoning v2, the perceiver. Qualify larger Qwen
   checkpoints separately; only a 5-step smoke has run.
5. **Play a first game in RBC** against a served checkpoint and record what happened
   ([t-8021](../../docs/work/t-8021/t-8021-playtester-try-the-first-knightlm-game-in-rbc.md)).
6. **Extend the model.** Prove the Burn backend against the reference
   ([Track B study](../../docs/model/trackb-smolvlm-study.md)) and add more sources
   ([corpus v3](../../docs/model/corpus-v3.md)).

[Model docs index](../../docs/model/README.md) · [Model tooling](../../model/README.md) ·
[Research plan](../../docs/model/plan.md) · [Architecture spec](../../docs/sas/KnightLM_SAS.md#98-implementation-phases)
