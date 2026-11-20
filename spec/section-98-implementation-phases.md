---
format: openwarrant/1.0
profile: spec
part_of: doc:SPEC.md
summary: 98. Implementation phases, from docs/sas/KnightLM_SAS.md.
---
# 98. Implementation phases

## 98. Implementation phases

### Phase 1 — Adopt OpenWarrant

Deliver:

- this SAS at an accepted revision;
- the first Warrant, `KLM-WAR-0001`, authorized, delivered, verified and resolved;
- `docs/authority/roles.toml` and `allowed_signers`, written by a human.

Exit:

- one Warrant has gone from `war new` to a human-signed resolution with an independent verification on record.

### Phase 2 — Universal state hub

Deliver:

- the v2 contract revision of `code/docs` (README, plan, contracts) to this SAS's scope;
- `crates/notation`: auto-detecting ingest for FEN (4- and 6-field), X-FEN/Shredder-FEN, EPD, PGN (tag roster, SAN movetext, comments, NAGs, recursive variations, `SetUp`/FEN starts, `Variant` tags), UCI move lists, SAN move lists, long algebraic, ICCF numeric and English descriptive notation;
- the export mirror — any state to FEN, X-FEN, EPD, PGN, UCI, SAN, long algebraic, ICCF numeric, descriptive, JSON context packet and SVG;
- Chess960/DFRC as first-class: variant-aware `GameRecord`, Scharnagl start positions, both UCI castling flavors;
- round-trip fixtures for every format and a python-chess parity oracle in `ports/`.

Exit:

- every supported format round-trips to an identical canonical state on the fixture corpus, and ingest either normalizes or fails explicitly (RQ-004 through RQ-007).

### Phase 3 — Human viewer

Deliver:

- `apps/rusty-brick-chess` (RustyBrickChess, RBC) is the single viewer: a Leptos + axum application compiled to WASM, no JavaScript or TypeScript in the application (browser test drivers under `tests/` are not part of it); the placeholder viewers `apps/viewer` and `apps/knightlm-viewer` are retired (decision `viewer-single-rbc`, docs/adr);
- paste or upload of any ingested notation; board rendering; move navigation; rule-facts and packet panel; legal-move highlighting; export buttons;
- a Stockfish analysis panel behind the engine boundary (lands with Phase 4).

Exit:

- the viewer renders and navigates every fixture game and exports every format, Rust-only (RQ-008).

### Phase 4 — Engine tooling

Deliver:

- `crates/engine`: a UCI client driving an external Stockfish process for analysis, teacher labels and sparring;
- `knightlm-uci`: a UCI server exposing the program's player — a random-legal baseline first, the fine-tuned model later — to any UCI GUI.

Exit:

- analysis and teaching flow through the UCI client with no engine in any model inference path, and the baseline completes games under a standard UCI GUI, reporting illegal-move failures instead of repairing them (RQ-009, RQ-010).

### Phase 5 — Training data pipeline

Deliver:

- streaming bulk ingestion of Lichess standard and Chess960 PGN into normalized `GameRecord`s with provenance;
- the decision-point compiler: per experiment arm, state packets plus targets — the played move, and Stockfish policy/eval as teacher labels only;
- sharded JSONL output and split audits rejecting leakage by game, family and canonical scored position.

Exit:

- a sharded corpus on disk whose provenance and split-audit records are green (RQ-011).

### Phase 6 — Model Track A: supervised fine-tune (HuggingFace)

Deliver:

- a LoRA fine-tune of SmolVLM-500M-Instruct on the Phase 5 corpus via HuggingFace `transformers` on cloud GPUs;
- evaluation through the scoring axes on held-out splits, reported separately;
- the trained model wired into `knightlm-uci` and the viewer behind the engine boundary.

Exit:

- a checkpoint whose raw legality, assisted legality, play strength and token cost are each reported with their evidence, and no composite score (RQ-012).

### Phase 7 — Model Track B: Burn-native admission

Deliver:

- the pinned SmolVLM architecture and processor implemented in `burn`, admitted only through the gates: processor and tokenizer parity with the Python reference, reference logits, finite nonzero gradients, tiny-set overfit, reload equivalence, reset isolation;
- Burn-native fine-tuning and local inference;
- a separate Qwen 3.5-class gate, including Gated DeltaNet state — passing SmolVLM admits nothing else.

Exit:

- the Burn implementation passes its recorded gates, and inference resets all KV, recurrent and convolution state between passes (RQ-013).
