---
schema: oh.war/atom/v1
warrant_uuid: 01a15b89-1bea-73e5-af55-6182429637ab
role: work_order
jurisdiction: authored
order: 40
classification: internal
---

# Work Order

## Deliverables

1. New crate `code/crates/notation/` (`knightlm-notation`) with:
   - a SAN writer and parser over `knightlm_core::GameState`: piece moves,
     captures, disambiguation (file and rank letters), promotions including
     under-promotions, castling, check and mate suffixes;
   - a format-detection seam with `Format::{San, Pgn}` today (variants are
     added by KLM-WAR-0003/0004 additively);
   - a thin Lichess bulk-DB reader: zstd-decompressed month PGN to an iterator
     of `GameRecord`s, carrying `source` provenance (`lichess-bulk/<YYYY-MM>`)
     and `history_complete` per KLM-SAS-RQ-006.
2. `knightlm-notation` CLI additions: `ingest --input <file.pgn.zst> --out
   <dir>` writing shard JSONL plus a manifest row per run.
3. A fixture corpus under `code/crates/notation/tests/`: hand-authored SAN
   edge cases plus real excerpts of Lichess month files (excerpted from a
   file a team member downloads; nothing auto-downloads).

## Seams and who depends on them

1. **SEAM `nota-reader`** — `knightlm_notation::reader::open(path) -> impl
   Iterator<Item = GameRecord>`. Contract: yields records or fails with an
   error carrying position and token; never silently drops or repairs a game.
   Consumers: KLM-WAR-0007 (corpus ingest) and KLM-WAR-0003 (shares the
   fixture corpus).
2. **SEAM `nota-detect`** — `knightlm_notation::detect(&[u8]) -> Format`.
   Consumers: KLM-WAR-0004 (adds variants), KLM-WAR-0010 (viewer ingest).
3. Frozen without change in this Warrant: the `knightlm-core` public API and
   the `GameRecord`/`GameState` schemas, provenance fields included.

## Owner's space

Inside the two seams the performer owns the rest: parser representations and
error types, streaming and buffering strategy, test layout and fixture
choice, CLI ergonomics, internal module structure. Improving the interior so
downstream consumers get a better tool is in scope; changing a seam is not —
a seam change is an amendment negotiated with the named consumers.

## Premade instructions

- `cargo test -p knightlm-notation` after every interior change.
- Never silently repair: a malformed game is an explicit error (RQ-004).

## Autonomy and escalation

Tier T2: the performer builds, tests and reports; the human authorizes and
resolves. A seam change request escalates to the sponsor.

## Rollback

Remove `code/crates/notation/` and the CLI command; nothing outside the crate
depends on the new seams yet.
