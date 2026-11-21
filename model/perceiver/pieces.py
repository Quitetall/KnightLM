"""Programmatic vector pieces. No third-party artwork is used or shipped.

Each silhouette is original geometry written for this repo (released with the
repository), drawn with Pillow at 4x and downsampled. See `piece_manifest`.
"""
from PIL import Image, ImageDraw

SS = 4  # supersampling factor
KINDS = "PNBRQK"

BASE = [(24, 78, 76, 87)]


def _shapes(kind: str):
    """(polys, ellipses, rects) in a 0..100 square, y down."""
    polys, ells, rects = [], [], list(BASE)
    if kind == "P":
        ells.append((50, 31, 11, 11))
        polys.append([(41, 42), (59, 42), (68, 78), (32, 78)])
    elif kind == "R":
        rects += [(34, 34, 66, 78), (29, 17, 40, 34), (44, 17, 56, 34), (60, 17, 71, 34)]
    elif kind == "N":
        polys.append([(30, 80), (35, 52), (27, 44), (40, 22), (47, 14), (53, 24), (67, 34), (73, 56), (70, 80)])
    elif kind == "B":
        ells += [(50, 31, 10, 15), (50, 13, 4, 4)]
        polys.append([(37, 80), (44, 44), (56, 44), (63, 80)])
    elif kind == "Q":
        polys.append([(28, 80), (22, 28), (38, 50), (50, 20), (62, 50), (78, 28), (72, 80)])
        ells += [(22, 26, 4, 4), (50, 17, 4, 4), (78, 26, 4, 4)]
    elif kind == "K":
        polys.append([(31, 80), (36, 46), (64, 46), (69, 80)])
        rects += [(46, 8, 54, 36), (39, 16, 61, 24)]
    else:
        raise ValueError(kind)
    return polys, ells, rects


def glyph(kind: str, white: bool, cell: int, fill, outline, scale: float, line: float, hollow: bool):
    """RGBA sprite of one piece on a transparent `cell` x `cell` square."""
    big = cell * SS
    img = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    f = big / 100.0

    def tx(x, y):
        return ((50 + (x - 50) * scale) * f, (50 + (y - 50) * scale) * f)

    w = max(1, int(round(line * f)))
    paint = (0, 0, 0, 0) if hollow else fill
    polys, ells, rects = _shapes(kind)
    # draw outline first (thick) then fill, so the outline sits outside the fill
    def poly(pts, col, width=0):
        if width:
            d.line([tx(*p) for p in pts + [pts[0]]], fill=col, width=width, joint="curve")
        else:
            d.polygon([tx(*p) for p in pts], fill=col)

    for p in polys:
        poly(p, paint)
        poly(p, outline, w)
    for (cx, cy, rx, ry) in ells:
        a, b = tx(cx - rx, cy - ry), tx(cx + rx, cy + ry)
        d.ellipse([a, b], fill=paint, outline=outline, width=w)
    for (x0, y0, x1, y1) in rects:
        a, b = tx(x0, y0), tx(x1, y1)
        d.rectangle([a, b], fill=paint, outline=outline, width=w)
    if kind == "B":  # slit distinguishes the bishop from a pawn at small sizes
        a, b = tx(47, 26), tx(55, 36)
        d.line([a, b], fill=outline, width=w)
    return img.resize((cell, cell), Image.LANCZOS)


def piece_manifest() -> dict:
    return {
        "name": "knightlm-vector-pieces-v1",
        "kind": "programmatic",
        "licence": "repository licence (original geometry; no third-party artwork)",
        "third_party_assets": [],
        "note": (
            "No openly licensed piece set is shipped in this repo: apps/rusty-brick-chess loads "
            "operator-uploaded SVG sets and ships none, so pieces are drawn from code."
        ),
    }


# ---- vendored SVG sets (permissive only) ----------------------------------

import hashlib
import io
import shutil
import subprocess
from pathlib import Path

ASSET_ROOT = Path(__file__).resolve().parents[2] / "crates" / "brick-chess" / "assets" / "pieces"
# Only sets whose LICENSE note names a permissive licence and is not a copy of another set's note.
# spatial and celtic are excluded: their LICENSE files repeat the Fantasy note verbatim.
SVG_SETS = {
    "rhosgfx": "CC0-1.0",
    "chessnut": "Apache-2.0",
    "fantasy": "MIT",
}
SVG_CODES = ["wP", "wN", "wB", "wR", "wQ", "wK", "bP", "bN", "bB", "bR", "bQ", "bK"]  # class 1..12
SPRITE_PX = 160
_svg_cache: dict = {}


def svg_available() -> bool:
    return shutil.which("rsvg-convert") is not None and ASSET_ROOT.is_dir()


def svg_sprites(name: str) -> list:
    """Twelve RGBA sprites (class order 1..12) rasterised once with rsvg-convert."""
    if name not in SVG_SETS:
        raise ValueError(f"piece set {name!r} is not on the permissive allow-list")
    if name not in _svg_cache:
        out = []
        for code in SVG_CODES:
            png = subprocess.run(
                ["rsvg-convert", "-w", str(SPRITE_PX), "-h", str(SPRITE_PX), "-f", "png",
                 str(ASSET_ROOT / name / f"{code}.svg")],
                check=True, capture_output=True, timeout=60,
            ).stdout
            out.append(Image.open(io.BytesIO(png)).convert("RGBA"))
        _svg_cache[name] = out
    return _svg_cache[name]


def svg_manifest(name: str) -> dict:
    root = ASSET_ROOT / name
    lic = (root / "LICENSE").read_text(encoding="utf-8")
    digest = hashlib.sha256()
    for code in SVG_CODES:
        digest.update((root / f"{code}.svg").read_bytes())
    return {
        "name": name,
        "kind": "vendored_svg",
        "licence": SVG_SETS[name],
        "licence_note": lic.strip().splitlines()[0],
        "source": f"crates/brick-chess/assets/pieces/{name}",
        "svg_sha256": digest.hexdigest(),
    }
