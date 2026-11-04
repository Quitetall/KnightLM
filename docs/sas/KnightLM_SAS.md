# KnightLM — Software Architecture Specification

Version 0.2.1. This document governs the program `KnightLM`; every Warrant
in `docs/warrants/` traces to it through `[[implements]]` refs into §106 and
`[[roadmap]]` refs into §98. It is a controlled document: `war sas propose
<version>` records a revision, a human accepts it with `war sign <version>
--ssh-sign`, and Warrants pin the accepted revision (OpenWarrant SAS §101).

## Program

KnightLM has two inseparable halves.

1. **A universal state hub.** Every notation a chess tool emits — FEN,
   X-FEN/Shredder-FEN, EPD, PGN, UCI move lists, SAN move lists, long
   algebraic, ICCF numeric, English descriptive — normalizes into one
   canonical game state, and every state exports back into any of them.
   Standard chess and Chess960/DFRC are both first-class. The hub serves a
   human viewer, engine tooling and a training pipeline from one
   deterministic core.
2. **A fine-tuned language-model player.** A multimodal LLM, fine-tuned for
   chess on compiled state packets, that plays through the same hub, is
   exposed as a UCI engine, and is scored on separately reported axes.

The code lives in the Rust workspace at `code/`. `crates/core` already
provides deterministic replay, rule facts, repetition ledgers, derived
features, a context-packet compiler and bounded rule simulation on
`cozy-chess`. This revision supersedes the v1 research framing with the v2
program scope; the v1 disciplines survive unchanged.

## Intended system

```text
any input ─► ingest (auto-detect + parse) ─► GameRecord/GameState (canonical, cozy-chess)
                                                │
        ┌──────────────┬────────────────────────┼─────────────────────┐
        ▼              ▼                        ▼                     ▼
   export (any    viewer (Leptos +          engine tooling:       training data
   notation)      axum, Rust-only)          Stockfish UCI client  compiler (packets +
                                              + player-as-UCI        teacher labels)
                                                                        │
                                                                        ▼
                                                          fine-tuned LLM (SmolVLM → Qwen)
                                                          Track A (HuggingFace), Track B (Burn)
```

## 6.10. Levels

| Level | What it is | Who closes it |
|---|---|---|
| Release | this SAS at an accepted version | a human, `war sign <version>` |
| Objective | one §98 phase, reached when its Exit holds | the records of its Warrants |
| Requirement | one §106 row, satisfied by the Warrants that implement it | resolved Warrants |
| Warrant | one bounded intervention, authorized and resolved by a human | `war sign <alias>` |
| Milestone / Stage | a checkpoint inside a Warrant; a dispatchable unit of work | evidence and verification |

A SAS and a Warrant are the same class of document at two levels: one SAS per
program, many Warrants under it. Starting a program means writing this file;
starting a unit of work means `war new`.

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

- `apps/viewer`: a Leptos + axum application compiled to WASM, no JavaScript or TypeScript;
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

## 106. Architecture requirements index

Stable identifiers for traceability. A Warrant's `[[implements]]` ref names a row
here; `war check` refuses a ref that names no row.

| ID | Requirement |
|---|---|
| KLM-SAS-RQ-001 | This SAS is accepted by a human signature and every Warrant pins the accepted revision |
| KLM-SAS-RQ-002 | Every Warrant is authorized and resolved by a human, never by the performer |
| KLM-SAS-RQ-003 | Every resolution rests on an independent verification and recorded gate evidence |
| KLM-SAS-RQ-004 | Every supported notation normalizes to the canonical GameRecord/GameState or fails explicitly; nothing is silently repaired or invented |
| KLM-SAS-RQ-005 | Every supported export format round-trips: state, export, ingest, identical canonical state, proven on fixtures |
| KLM-SAS-RQ-006 | Every GameRecord carries source and history_complete as source assertions; a four-field FEN source never yields history_complete true |
| KLM-SAS-RQ-007 | Standard chess and Chess960/DFRC are first-class in ingest, replay, facts and export, in both UCI castling flavors |
| KLM-SAS-RQ-008 | The viewer is Rust-only, Leptos and axum compiled to WASM, with no JavaScript or TypeScript in the application |
| KLM-SAS-RQ-009 | An engine is reachable only as teacher labeler, post-decision scorer or opponent; the model's inference path never queries an engine |
| KLM-SAS-RQ-010 | The player is exposed as a UCI engine, and an illegal model move is a reported failure, never silently corrected |
| KLM-SAS-RQ-011 | Training corpora carry provenance and pass split audits before use; teacher labels are recorded as labels only |
| KLM-SAS-RQ-012 | Model ability is reported per scoring axis with bounded evidence; no composite winner score and no scored free prose |
| KLM-SAS-RQ-013 | A Burn-native model is admitted only through the parity gates, with reset isolation between inference passes |

## 130. Disciplines

Carried from v1, unchanged, and binding on every Warrant.

- The deterministic compiler SHALL be authoritative for rule facts, legality and history; model output and pixels SHALL NOT originate them.
- Every `GameRecord` SHALL carry a `source` and a `history_complete` assertion made by the trusted source; unknown history SHALL NOT be invented; a four-field FEN source SHALL NOT yield `history_complete: true`.
- A failure SHALL be reported, never silently repaired: unparseable notation, observation/ledger mismatch, illegal model move.
- An engine MAY supply teacher labels or post-decision scoring, or play as an opponent; the model's inference path SHALL NOT query an engine.
- Ability SHALL be reported per scoring axis with bounded evidence; a composite winner score SHALL NOT be produced.
- Free prose SHALL NOT be scored; only mechanically checkable structured claims.

## 140. Technology pins

- Rust 2024 edition, `Cargo.lock` pinned. `cozy-chess` =0.3.4: move generation, FEN including Shredder-FEN, UCI helpers; it parses no SAN or PGN — the notation layer is this program's own.
- `burn` =0.21.0 for Track B; CPU ndarray for checks, GPU backends for training.
- Viewer: Leptos 0.8.x + axum, compiled to WASM; the application SHALL contain no JavaScript or TypeScript. A Tauri desktop shell MAY wrap the same viewer later.
- Track A: HuggingFace `transformers` with LoRA on cloud CUDA GPUs; local hardware for iteration and CPU-verified runs.
- Stockfish: an external process over UCI; no engine binary is bundled or auto-downloaded.

## 150. Scope and non-goals

- In scope: standard untimed chess and Chess960/DFRC; the notations named by RQ-004; local CLI, local viewer, cloud training.
- Out of scope for v2: other board games (a compiler seam remains, no transfer claim), auto-download of engines, weights or bulk datasets by any command, and any engine in the model's inference path.
