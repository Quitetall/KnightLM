"""Visual styles. Each style fixes theme, piece rendering, size, noise and flip.

Styles in `TRAIN_STYLES` are used for fitting; `HELDOUT_STYLES` differ in
board colours AND (for most) in the piece artwork, and are never fitted.
"""
from dataclasses import dataclass, asdict


@dataclass(frozen=True)
class Style:
    name: str
    split: str  # "train" | "heldout"
    pieces: str  # "vector-solid" | "vector-hollow" | "svg:<set>"
    light: tuple
    dark: tuple
    white_fill: tuple = (250, 250, 250)
    black_fill: tuple = (40, 40, 40)
    outline: tuple = (10, 10, 10)
    size: int = 256  # rendered board pixels (square)
    scale: float = 0.95
    line: float = 2.0
    noise: float = 0.0  # gaussian sigma, 0..255 units
    blur: float = 0.0  # gaussian radius px
    jpeg: int = 0  # 0 = off, else quality
    flip: bool = False  # board drawn from Black's side

    def to_dict(self):
        return asdict(self)


def _s(**kw):
    return Style(**kw)


TRAIN_STYLES = [
    _s(name="vec-green", split="train", pieces="vector-solid", light=(238, 238, 210), dark=(118, 150, 86)),
    _s(name="vec-brown-flip", split="train", pieces="vector-solid", light=(240, 217, 181), dark=(181, 136, 99),
       size=320, flip=True, noise=4.0),
    _s(name="rhos-blue", split="train", pieces="svg:rhosgfx", light=(222, 227, 230), dark=(140, 162, 173), size=288),
    _s(name="rhos-brown-noisy", split="train", pieces="svg:rhosgfx", light=(240, 217, 181), dark=(181, 136, 99),
       size=224, noise=8.0, blur=0.6, jpeg=60, flip=True),
    _s(name="chessnut-grey", split="train", pieces="svg:chessnut", light=(215, 215, 215), dark=(125, 135, 150),
       size=256),
    _s(name="vec-thin-small", split="train", pieces="vector-solid", light=(235, 236, 208), dark=(119, 149, 86),
       size=192, scale=0.85, line=1.5, jpeg=70),
]

HELDOUT_STYLES = [
    # unseen artwork (Fantasy, MIT) on unseen colours
    _s(name="ho-fantasy-purple", split="heldout", pieces="svg:fantasy", light=(232, 224, 244), dark=(140, 110, 180),
       size=272),
    _s(name="ho-fantasy-flip-noisy", split="heldout", pieces="svg:fantasy", light=(245, 235, 200), dark=(170, 120, 80),
       size=240, flip=True, noise=6.0, jpeg=65),
    # unseen glyph rendering (hollow vector pieces) on a seen colour theme
    _s(name="ho-vec-hollow", split="heldout", pieces="vector-hollow", light=(240, 217, 181), dark=(181, 136, 99),
       size=256, scale=1.05, line=3.0),
    # unseen colours only (seen artwork)
    _s(name="ho-rhos-red", split="heldout", pieces="svg:rhosgfx", light=(250, 225, 225), dark=(190, 100, 100),
       size=256),
]

# Styles that need no external tool (used when rsvg-convert is absent).
VECTOR_ONLY = [s for s in TRAIN_STYLES + HELDOUT_STYLES if s.pieces.startswith("vector")]
ALL_STYLES = TRAIN_STYLES + HELDOUT_STYLES
