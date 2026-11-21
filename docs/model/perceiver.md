# Perceiver: image to `BoardObservation`

Invariant 9: perception is not reasoning. The perceiver turns a picture into a
piece placement; the compiler decides whether that placement is true. It is a
separate model from any LLM, so any language model can take image input by
calling it (or its output) rather than learning to see.

## Contract

```
image (board crop, orientation declared by caller)
  -> 64 square crops, FEN order a8..h1 (a flipped board is un-rotated by the declared flag)
  -> 13-class classifier per square: empty, white/black x pawn/knight/bishop/rook/queen/king
  -> placement string (FEN field 1)
  -> BoardObservation {schema_version, image_id, placement}
  -> knightlm verify-observation <game> <observation>   (GameState::verify_observation)
```

- Placement only. No side to move, castling rights, clocks, en passant or
  history: pixels do not carry them (`docs/model/contracts.md`).
- The compiler is the judge. A predicted placement that differs from the
  ledger fails with `perception failure: observed placement disagrees with
  authoritative ledger`. The perceiver never repairs, retries into agreement,
  or reads the ledger. `model/perceiver/observation.py` shells out to the
  existing CLI command and does not reimplement the comparison.
- Orientation is an input (`flip`), not something inferred; board localisation
  (finding and cropping the board in a photo) is out of scope here.
- Relation to `apps/rusty-brick-chess/src/vision.rs`: that path is an optional,
  operator-downloaded AGPL-3.0 YOLOv8m detector (13 classes with a `board`
  class, no empty). It stays the app's detector and is not duplicated. This
  perceiver is a trained-from-scratch square classifier whose purpose is a
  measured, licence-clean research arm and a dataset/scoring harness; its
  output goes through the same `BoardObservation` check, so either source can
  feed the same ledger gate. Class order differs (ours: empty + `PNBRQKpnbrqk`).

## Data and licences

Positions: distinct placements from the Run 1 corpus
(`/mnt/4tb/artifacts/KnightLM/expire/corpus/run1-reasoning-2026-09-a`),
training positions from `train-*.jsonl`, evaluation positions from
`holdout-*.jsonl` with any overlap removed.

Pieces (only permissive sources; each dataset manifest records them):

| Source | Licence | Use |
|---|---|---|
| `vector-solid`, `vector-hollow` (code in `pieces.py`) | original geometry, repository licence | train / held-out |
| RhosGFX (`crates/brick-chess/assets/pieces/rhosgfx`) | CC0-1.0 | train; also a held-out colour theme |
| Chessnut | Apache-2.0 | train |
| Fantasy | MIT | held-out only |

Not used: Cburnett and Merida (GPL-2.0-or-later), Letter/Pirouetti/Pixel
(AGPL-3.0). Spatial and Celtic are excluded because their `LICENSE` notes are
a verbatim copy of the Fantasy note, so their licence is unverified. The repo
ships these sets only inside `crates/brick-chess`; RBC itself carries no
user-facing sets. SVGs are rasterised with `rsvg-convert` (styles using them
are skipped when it is absent).

Styles (`styles.py`): 6 train and 4 held-out. A style fixes board colours,
piece artwork, size (192-320 px), noise, blur, JPEG quality and flip. Held-out
styles use unseen artwork (Fantasy, hollow vector pieces) or unseen colours.
All rendering is deterministic in `(placement, style, seed)`.

## Model and scoring

`SquareCNN`: three conv-BN-ReLU-pool blocks and a 2-layer head over a 32x32 RGB
crop (about 60k parameters), shared across squares.

`python3 -m model.perceiver.evaluate` reports, per style and for the train and
held-out groups: per-square accuracy, occupied-square accuracy, whole-board
exact match, and a 13x13 confusion matrix. Whole-board exact match is the
headline number: one wrong square is a perception failure at the ledger. The
held-out group is the generalisation claim; the train group is not.

## Smoke result (CPU, 2026-11-20)

Fitting 36 rendered boards (2 vector styles) reaches at least 0.98 square
accuracy in 12 epochs inside the unit test, so the model and pipeline can
learn. A 300-board fit (50 boards x 6 train styles, 6 epochs) reached 1.0 on
train styles; held-out styles in the same run scored square accuracy
0.67 to 1.0 and board exact match 0.0 on three of four. That is a smoke
signal about the harness, not a result: 6 epochs on 300 positions says nothing
about a trained perceiver.

## Prepared training run (not run)

```
python3 -m model.perceiver.train --out /mnt/4tb/artifacts/KnightLM/expire/perceiver/run-a \
    --positions 4000 --boards-per-style 2000 --epochs 12 --device cuda
python3 -m model.perceiver.evaluate --checkpoint /mnt/4tb/artifacts/KnightLM/expire/perceiver/run-a/perceiver.pt \
    --boards-per-style 300 --out /mnt/4tb/artifacts/KnightLM/expire/perceiver/run-a/scorecard.json
```

Estimate: rendering 12k boards about 4 min on one CPU core (about 20 ms per
board, measured); 768k crops x 12 epochs = 9.2M samples held as 2.4 GB uint8 in RAM.
CPU throughput measured about 900 samples/s under a concurrent build, so
about 2.8 h on CPU; on a GPU the small net is input-bound, roughly 10 to 20
minutes (estimate, unmeasured). Check `free -g`, `df -h /` before launching. To
improve held-out transfer first add styles and augmentation, not capacity.

## What is not claimed

No real photographs, no board localisation, no perspective warp (only flip),
no lighting or occlusion, no piece-set coverage beyond the listed ones, and no
trained-perceiver accuracy until the prepared run is executed and scored.
