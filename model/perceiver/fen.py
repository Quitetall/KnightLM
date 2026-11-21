"""Placement <-> 64-square class labels. Class 0 is empty; 1..12 are PNBRQKpnbrqk."""

PIECES = "PNBRQKpnbrqk"
CLASS_NAMES = ["empty"] + [
    f"{c}_{r}" for c in ("white", "black") for r in ("pawn", "knight", "bishop", "rook", "queen", "king")
]
NUM_CLASSES = 13


def placement_to_labels(placement: str) -> list[int]:
    """Index 0 is a8 and index 63 is h1 (FEN order, white at the bottom)."""
    ranks = placement.split()[0].split("/")
    if len(ranks) != 8:
        raise ValueError(f"placement needs 8 ranks: {placement!r}")
    out: list[int] = []
    for rank in ranks:
        row: list[int] = []
        for ch in rank:
            if ch.isdigit():
                row += [0] * int(ch)
            elif ch in PIECES:
                row.append(PIECES.index(ch) + 1)
            else:
                raise ValueError(f"bad placement character {ch!r}")
        if len(row) != 8:
            raise ValueError(f"rank does not have 8 squares: {rank!r}")
        out += row
    return out


def labels_to_placement(labels) -> str:
    labels = [int(x) for x in labels]
    if len(labels) != 64 or any(not 0 <= x < NUM_CLASSES for x in labels):
        raise ValueError("need 64 class ids in 0..12")
    ranks = []
    for r in range(8):
        text, empty = "", 0
        for x in labels[r * 8 : r * 8 + 8]:
            if x == 0:
                empty += 1
                continue
            text += (str(empty) if empty else "") + PIECES[x - 1]
            empty = 0
        ranks.append(text + (str(empty) if empty else ""))
    return "/".join(ranks)


def flip_labels(labels):
    """Rotate the grid by 180 degrees (the board drawn from Black's side)."""
    return list(labels)[::-1]
