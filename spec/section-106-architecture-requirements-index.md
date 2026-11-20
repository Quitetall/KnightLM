---
format: openwarrant/1.0
profile: spec
part_of: doc:SPEC.md
summary: 106. Architecture requirements index, from docs/sas/KnightLM_SAS.md.
---
# 106. Architecture requirements index

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
| KLM-SAS-RQ-008 | The single viewer is RustyBrickChess, Rust-only, Leptos and axum compiled to WASM, with no JavaScript or TypeScript in the application |
| KLM-SAS-RQ-009 | An engine is reachable only as teacher labeler, post-decision scorer or opponent; the model's inference path never queries an engine |
| KLM-SAS-RQ-010 | The player is exposed as a UCI engine, and an illegal model move is a reported failure, never silently corrected |
| KLM-SAS-RQ-011 | Training corpora carry provenance and pass split audits before use; teacher labels are recorded as labels only |
| KLM-SAS-RQ-012 | Model ability is reported per scoring axis with bounded evidence; no composite winner score and no scored free prose |
| KLM-SAS-RQ-013 | A Burn-native model is admitted only through the parity gates, with reset isolation between inference passes |

## REQ-001 · requirement — This SAS is accepted by a human signature and every Warrant pins the accepted revision
statement: This SAS is accepted by a human signature and every Warrant pins the accepted revision
legacy.id: KLM-SAS-RQ-001

## REQ-002 · requirement — Every Warrant is authorized and resolved by a human, never by the performer
statement: Every Warrant is authorized and resolved by a human, never by the performer
legacy.id: KLM-SAS-RQ-002

## REQ-003 · requirement — Every resolution rests on an independent verification and recorded gate evidence
statement: Every resolution rests on an independent verification and recorded gate evidence
legacy.id: KLM-SAS-RQ-003

## REQ-004 · requirement — Every supported notation normalizes to the canonical GameRecord/GameState or fails explicitly; nothing is silently re...
statement: Every supported notation normalizes to the canonical GameRecord/GameState or fails explicitly; nothing is silently repaired or invented
legacy.id: KLM-SAS-RQ-004

## REQ-005 · requirement — Every supported export format round-trips: state, export, ingest, identical canonical state, proven on fixtures
statement: Every supported export format round-trips: state, export, ingest, identical canonical state, proven on fixtures
legacy.id: KLM-SAS-RQ-005

## REQ-006 · requirement — Every GameRecord carries source and history_complete as source assertions; a four-field FEN source never yields histo...
statement: Every GameRecord carries source and history_complete as source assertions; a four-field FEN source never yields history_complete true
legacy.id: KLM-SAS-RQ-006

## REQ-007 · requirement — Standard chess and Chess960/DFRC are first-class in ingest, replay, facts and export, in both UCI castling flavors
statement: Standard chess and Chess960/DFRC are first-class in ingest, replay, facts and export, in both UCI castling flavors
legacy.id: KLM-SAS-RQ-007

## REQ-008 · requirement — The single viewer is RustyBrickChess: Rust-only Leptos and axum on WASM, no JavaScript or TypeScript...
statement: The single viewer is RustyBrickChess, Rust-only, Leptos and axum compiled to WASM, with no JavaScript or TypeScript in the application
legacy.id: KLM-SAS-RQ-008

## REQ-009 · requirement — An engine is reachable only as teacher labeler, post-decision scorer or opponent; the model's inference path never qu...
statement: An engine is reachable only as teacher labeler, post-decision scorer or opponent; the model's inference path never queries an engine
legacy.id: KLM-SAS-RQ-009

## REQ-010 · requirement — The player is exposed as a UCI engine, and an illegal model move is a reported failure, never silently corrected
statement: The player is exposed as a UCI engine, and an illegal model move is a reported failure, never silently corrected
legacy.id: KLM-SAS-RQ-010

## REQ-011 · requirement — Training corpora carry provenance and pass split audits before use; teacher labels are recorded as labels only
statement: Training corpora carry provenance and pass split audits before use; teacher labels are recorded as labels only
legacy.id: KLM-SAS-RQ-011

## REQ-012 · requirement — Model ability is reported per scoring axis with bounded evidence; no composite winner score and no scored free prose
statement: Model ability is reported per scoring axis with bounded evidence; no composite winner score and no scored free prose
legacy.id: KLM-SAS-RQ-012

## REQ-013 · requirement — A Burn-native model is admitted only through the parity gates, with reset isolation between inference passes
statement: A Burn-native model is admitted only through the parity gates, with reset isolation between inference passes
legacy.id: KLM-SAS-RQ-013
