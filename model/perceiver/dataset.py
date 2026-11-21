"""Positions from the corpus, rendered boards, and the 64-square dataset."""
import hashlib
import json
import random
from pathlib import Path

import numpy as np

from . import pieces as P
from .fen import placement_to_labels
from .render import render_board, board_to_cells
from .styles import ALL_STYLES, Style

CORPUS_DEFAULT = Path("/mnt/4tb/artifacts/KnightLM/expire/corpus/run1-reasoning-2026-09-a")


def load_placements(shards, limit: int, seed: int = 0) -> list[str]:
    """Distinct FEN placement fields from corpus JSONL rows (`packet.fen`), seeded sample."""
    seen: dict[str, None] = {}
    for shard in shards:
        with open(shard, encoding="utf-8") as fh:
            for line in fh:
                try:
                    fen = json.loads(line)["packet"]["fen"]
                except (KeyError, ValueError, TypeError):
                    continue
                seen.setdefault(fen.split()[0], None)
    items = sorted(seen)
    random.Random(seed).shuffle(items)
    return items[:limit]


def corpus_split(root: Path = CORPUS_DEFAULT):
    """(train shards, holdout shards). Positions are deduplicated across the split by the caller."""
    root = Path(root)
    return sorted(root.glob("train-*.jsonl")), sorted(root.glob("holdout-*.jsonl"))


def available(style: Style) -> bool:
    return not style.pieces.startswith("svg:") or P.svg_available()


def build(placements: list[str], styles: list[Style], seed: int, boards_per_style: int | None = None):
    """Render each placement once per style, round-robin. Returns (cells, labels, style_idx, ids).

    cells uint8 (N*64, 32, 32, 3); labels int64 (N*64,); style_idx (N*64,) into `styles`;
    ids list of f"{style}:{n}" per board. Deterministic in (placements, styles, seed).
    """
    cells, labels, sidx, ids = [], [], [], []
    for si, style in enumerate(styles):
        n = boards_per_style if boards_per_style is not None else len(placements)
        for k in range(n):
            placement = placements[k % len(placements)]
            img = render_board(placement, style, seed=_seed(seed, style.name, k))
            cells.append(board_to_cells(img, style.flip))
            labels.append(np.asarray(placement_to_labels(placement), dtype=np.int64))
            sidx.append(np.full(64, si, dtype=np.int64))
            ids.append(f"{style.name}:{k}:{placement}")
    return (np.concatenate(cells), np.concatenate(labels), np.concatenate(sidx), ids)


def _seed(seed: int, name: str, k: int) -> int:
    h = hashlib.sha256(f"{seed}/{name}/{k}".encode()).digest()
    return int.from_bytes(h[:4], "little")


def manifest(styles: list[Style], seed: int) -> dict:
    sets = sorted({s.pieces[4:] for s in styles if s.pieces.startswith("svg:")})
    return {
        "format": "knightlm-perceiver-dataset/v1",
        "seed": seed,
        "piece_sets": [P.piece_manifest()] + [P.svg_manifest(n) for n in sets],
        "excluded_sets": {
            "cburnett": "GPL-2.0-or-later (not permissive)",
            "merida": "GPL-2.0-or-later (not permissive)",
            "letter/pirouetti/pixel": "AGPL-3.0-or-later (not permissive)",
            "spatial/celtic": "LICENSE note is a verbatim copy of the Fantasy note; unverified",
        },
        "styles": [s.to_dict() for s in styles],
    }
