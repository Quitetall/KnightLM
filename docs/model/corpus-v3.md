# Corpus v3: one pipeline for every source

Status: draft spec, with two measured pilots (evidence:
`artifacts/KnightLM/keep/corpus-v3-pilots-2026-11/`). Run 1 trains on the
`knightlm-lichess-v2` corpus; nothing here changes it.

## Why

Run 1 data is one decision per game from the Lichess database. The coach needs
commentary and book text that is tied to real positions, with a licence and a
leak audit per source. Every new source gets the same stages, not its own
script.

## Pipeline

```
adapter -> typed record -> replay/validate (notation) -> dedupe/decontaminate
        -> render row kinds (prompt.py) -> shards + manifest (licence, weights)
```

1. **Adapters** read one source kind and emit records only. They never write
   rows. Pilots built: `model/corpus/commentary.py` (annotated PGN, Lichess
   studies and broadcasts) and `model/corpus/book.py` (plain-text book, English
   descriptive notation). Still to build: PDF text and OCR (see "Next adapters").
2. **Typed record** (`model/corpus/records.py`, format `knightlm-record-v3`):
   `source_id`, `licence`, `content_hash` (sha256 of `text`), `kind`
   (`commentary`, `text`, `move`, `reasoning`), `text`, `positions` (list of
   `{role, fen}` where every FEN was produced by the notation crate, never copied
   from the source), and `provenance` (adapter, source file sha256, locator such
   as game URL or book game and ply, original move text).
3. **Replay and validate** is done only by the notation crate through the
   `knightlm` binary, the same pattern as `compile_candidates` in `pipeline.py`:
   - `knightlm corpus-commentary --source NAME` reads PGN on stdin, replays each
     main line, emits comment rows with before and after FEN, a per-game
     summary, and a `quarantine` row at the first move that fails (the rest of
     that game is dropped).
   - `knightlm replay-lines` reads `{id, fen?, notation?, sans}` lines and
     returns the validated FEN after each move and the first failure.
     `notation` is `san` (default) or `descriptive` (new
     `knightlm_notation::descriptive`, accepts a move only when exactly one
     legal move can be written that way).
   A prose or comment passage is linked to a position only if every move before
   it replayed. Anything else is quarantined with a reason, never guessed.
4. **Dedupe and decontaminate**: drop records with equal `(text, 4-field
   position key)`; drop records whose `content_hash` already exists; apply the
   leak audit below against the holdout.
5. **Render row kinds** through `model/prompt.py`, the single prompt source:
   `move` rows keep today's prompt; `commentary`, `reasoning` and `text` rows
   get their prompt templates added to `prompt.py` by the stage 3 contract PR
   (play and coach schemas). Rendering is a pure function of a record and a
   position packet, so a corpus can be re-rendered without re-extracting.
6. **Manifest** (`knightlm-corpus-v3`): per source `source_id`, `licence`, source
   sha256 and bytes, adapter and version, record counts by kind and by
   quarantine reason, and the **mixture weights actually used** (rows drawn per
   source and kind). Weights are recorded, not implied by file sizes. Shards
   keep the v2 conventions: fixed size, sha256 per shard, a manifest that
   accounts for every file.

## Leak audit (extends v2)

`audit_records` in `model/corpus/records.py` refuses a split when:

- any `source_id` appears on both sides (**holdout books and studies are
  whole-source**: a held-out book or study contributes nothing to training, in
  any chapter or chunk);
- any `content_hash` appears on both sides;
- any 4-field position key (placement, side, castling, en passant) of any
  holdout record appears in **any training record of any kind**: move,
  commentary, reasoning or text.

The v2 audit (`audit_splits`: game id, ECO family, scored position, strict time
order) stays as it is and keeps guarding the v2 move rows. A v3 corpus runs both:
v2 on its move rows, `audit_records` on everything. Tests plant each leak and
expect a refusal (`model/corpus/test_v3.py`).

Known gap, to close before a v3 corpus is trained on: positions reached by
*transposition* from a held-out position inside a longer line are not
decontaminated; only exact position keys are. Also open: near-duplicate text
(for example the same annotator pasting one comment across studies) needs a
MinHash pass.

## Reading v2

`read_corpus` and `audit` in `pipeline.py` are untouched and still read
`knightlm-lichess-v2`. A v3 manifest has `format: knightlm-corpus-v3`; the v2
reader refuses it by name and the v3 reader refuses v2, so neither can load the
other by accident.

## Licences and provenance

Every source carries a licence string that a person verified from a cited page;
`UNVERIFIED` sources can be measured but not trained on.

| source | licence | status |
|---|---|---|
| Lichess database game exports | CC0 | verified: database.lichess.org states "Database exports are released under the Creative Commons CC0 license" |
| Lichess broadcast games | CC BY-SA 4.0 | verified on the same page; share-alike and attribution obligations need a decision before use |
| Lichess studies (user-authored comments) | not stated on the database page | **UNVERIFIED**. The page covers database exports and broadcasts only; the terms of service page was not readable by script (Cloudflare challenge). A person must read lichess.org/terms-of-service and decide. Commentary may carry per-author rights |
| Capablanca, *Chess Fundamentals* (1921) | public domain in the US | Project Gutenberg #33870 states "no one owns a United States copyright in these works"; the title page reads "COPYRIGHT, 1921". Public-domain status outside the US is not checked |

## Pilot results (summary; numbers in the evidence README)

- Lichess studies and broadcasts, 11.9 MB of the `lichess` user's studies:
  4,854 usable commentary records from 228,498 mainline comments; nothing failed
  to replay. The yield is low because most comments are clock tags or engine
  templates, and the median record is 15 words.
- Capablanca: 319 of about 2,740 move plies replay, because the book is in
  descriptive notation and most examples start from a printed diagram with no
  position in the text. 58 prose passages link to a validated position.

## Next adapters, in order

1. Lichess studies by explicit study id with licence check (needs the licence
   decision above), plus Lichess broadcasts under CC BY-SA if accepted.
2. Diagram to FEN for book pages (the vision slot): unlocks about 85% of the
   book plies that are now `no_start_position`. Until then use only passages
   linked from a game start.
3. PDF text (PyMuPDF or Docling) feeding the same book adapter, with OCR error
   classes recorded as quarantine reasons; figurine notation mapping before
   the SAN pass.
