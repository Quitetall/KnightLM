"""Input planes from a FEN or a position packet. No chess logic lives here.

The piece placement comes from `model.eval.explain.Board` (the repo's existing FEN
reader); castling, clocks and the en passant square are copied from the FEN fields,
and repetition from the packet. Nothing is inferred: a FEN with four fields is
refused (no invented clocks), and an unknown repetition count is a flag, not a zero.

Layout: `(PLANES, 8, 8)` flattened as `[plane][rank * 8 + file]`, a1 = square 0, h8 = 63
(the same square index as `crates/cli/src/board_labels.rs`). Plane order:

  0-5    white P N B R Q K        12   side to move is white (constant plane)
  6-11   black p n b r q k        13-16  castling K Q k q (constant planes)
  17     en passant square        18   halfmove clock / 100 (clamped to 1)
  19     fullmove number / 200 (clamped to 1)
  20     repetition known         21   repetition count / 3 (clamped to 1; 0 when unknown)
"""

from model.eval.explain import Board

PIECES = "PNBRQKpnbrqk"
PLANES = 22
SIDE, CASTLE0, EP, HALF, FULL, REP_KNOWN, REP = 12, 13, 17, 18, 19, 20, 21
FILES = "abcdefgh"


def square_index(name):
    return (int(name[1]) - 1) * 8 + FILES.index(name[0])


def packet_inputs(packet):
    """(fen, repetition) from a position packet. History that is not complete leaves repetition unknown."""
    facts = packet.get("facts", {})
    rep = facts.get("repetition_count") if packet.get("history_complete") else None
    return packet["fen"], rep


def build_planes(fen, repetition=None):
    """Flat list of PLANES * 64 floats for one position."""
    fields = fen.split()
    if len(fields) != 6:
        raise ValueError(f"planes need a six-field FEN (clocks are never invented): {fen!r}")
    board = Board(fen)
    out = [0.0] * (PLANES * 64)

    def put(plane, sq, value=1.0):
        out[plane * 64 + sq] = value

    for name, (color, kind) in board.pieces.items():
        letter = kind if color == "w" else kind.lower()
        put(PIECES.index(letter), square_index(name))
    const = {SIDE: fields[1] == "w"}
    castle = fields[2]
    if castle != "-" and not set(castle) <= set("KQkq"):
        raise ValueError(f"only standard castling letters are supported: {castle!r}")
    for k, letter in enumerate("KQkq"):
        const[CASTLE0 + k] = letter in castle
    for plane, on in const.items():
        if on:
            out[plane * 64:(plane + 1) * 64] = [1.0] * 64
    if board.ep:
        put(EP, square_index(board.ep))
    half, full = int(fields[4]), int(fields[5])
    out[HALF * 64:(HALF + 1) * 64] = [min(half / 100.0, 1.0)] * 64
    out[FULL * 64:(FULL + 1) * 64] = [min(full / 200.0, 1.0)] * 64
    if repetition is not None:
        out[REP_KNOWN * 64:(REP_KNOWN + 1) * 64] = [1.0] * 64
        out[REP * 64:(REP + 1) * 64] = [min(repetition / 3.0, 1.0)] * 64
    return out


def packet_planes(packet):
    return build_planes(*packet_inputs(packet))
