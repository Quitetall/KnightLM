"""Deterministic board rendering from a FEN placement and a Style."""
import random

import numpy as np
from PIL import Image, ImageFilter, ImageDraw
import io

from . import pieces as P
from .fen import placement_to_labels, flip_labels
from .styles import Style

CELL = 32  # model input cell size
BOARD_PX = 8 * CELL


def _vector_sprite(style: Style, cls: int, cell: int):
    white = cls <= 6
    kind = P.KINDS[(cls - 1) % 6]
    hollow = style.pieces == "vector-hollow"
    if hollow:  # outline-only pieces: stroke colour carries the side
        fill, outline = (0, 0, 0), ((255, 255, 255) if white else (0, 0, 0))
    else:
        fill, outline = (style.white_fill if white else style.black_fill), style.outline
    return P.glyph(kind, white, cell, tuple(fill) + (255,), tuple(outline) + (255,), style.scale, style.line, hollow)


def _sprite(style: Style, cls: int, cell: int):
    if style.pieces.startswith("svg:"):
        sp = P.svg_sprites(style.pieces[4:])[cls - 1]
        return sp.resize((cell, cell), Image.LANCZOS)
    return _vector_sprite(style, cls, cell)


def render_board(placement: str, style: Style, seed: int = 0) -> Image.Image:
    """RGB image of the board only (no margins). Deterministic in (placement, style, seed)."""
    labels = placement_to_labels(placement)
    if style.flip:
        labels = flip_labels(labels)
    size = style.size
    cell = size // 8
    size = cell * 8
    img = Image.new("RGB", (size, size))
    d = ImageDraw.Draw(img)
    for i in range(64):
        r, c = divmod(i, 8)
        col = style.light if (r + c) % 2 == 0 else style.dark
        d.rectangle([c * cell, r * cell, (c + 1) * cell - 1, (r + 1) * cell - 1], fill=col)
    cache: dict = {}
    for i, cls in enumerate(labels):
        if cls == 0:
            continue
        if cls not in cache:
            cache[cls] = _sprite(style, cls, cell)
        r, c = divmod(i, 8)
        img.paste(cache[cls], (c * cell, r * cell), cache[cls])
    if style.blur:
        img = img.filter(ImageFilter.GaussianBlur(style.blur))
    rng = np.random.default_rng(seed)
    if style.noise:
        arr = np.asarray(img, dtype=np.float32) + rng.normal(0, style.noise, (size, size, 3))
        img = Image.fromarray(np.clip(arr, 0, 255).astype(np.uint8))
    if style.jpeg:
        buf = io.BytesIO()
        img.save(buf, "JPEG", quality=style.jpeg)
        buf.seek(0)
        img = Image.open(buf).convert("RGB")
    return img


def board_to_cells(img: Image.Image, flipped: bool) -> np.ndarray:
    """uint8 array (64, CELL, CELL, 3) in FEN order (a8..h1) for a declared orientation."""
    arr = np.asarray(img.convert("RGB").resize((BOARD_PX, BOARD_PX), Image.BILINEAR))
    cells = arr.reshape(8, CELL, 8, CELL, 3).transpose(0, 2, 1, 3, 4).reshape(64, CELL, CELL, 3)
    return cells[::-1] if flipped else cells
