"""The ctok/1 vocabulary: one ordered token list, its ranges, hash and manifest.

Local ids run 0..SIZE-1. A host model adds `base` to every id (see hf.py). The
list is built once, deterministically; the SHA-256 of its newline-joined token
strings is the vocabulary identity. Any change to the list is a new spec version.
"""

import hashlib
import json

SPEC_VERSION = "ctok/1"
CGT_VERSION = "cgt/1"
PACKET_SCHEMA_VERSION = 1
# The rules the tokenized facts come from: the packet ruleset, the shakmaty
# release inside the compiler (crates/notation) and the CGT version.
RULES_REVISION = "standard-untimed-v1;shakmaty-0.30.2;cgt/1"
MANIFEST_FORMAT = "ctok-manifest/1"

FILES = "abcdefgh"
RANKS = "12345678"
SQUARES = [f + r for r in RANKS for f in FILES]  # a1, b1, ... h8 (index = 8*rank + file)
PIECES = "KQRBNPkqrbnp"  # canonical node order: white then black, K Q R B N P
PROMOS = "qrbn"
TERMS = ("checkmate", "stalemate", "proven_dead_position", "fivefold_repetition",
         "seventy_five_move", "white_wins", "black_wins", "draw")
MAX_HALFMOVE = 199
MAX_REP = 9
RESERVED_MIN = 128
CGT_KEYS = ("turn", "castle", "ep", "clock", "rep", "term", "W", "B", "moves",
            "atk", "def", "pin", "hang", "check", "ksafe")
CGT_GROUPS = ("open", "hit", "own")
CGT_MOVE_MARKS = ("x", "ep", "=Q", "=R", "=B", "=N", "+", "#", "O-O", "O-O-O")
CASTLE_RIGHTS = "KQkq"


def _square_index(sq):
    return RANKS.index(sq[1]) * 8 + FILES.index(sq[0])


def _queen_or_knight_targets(frm):
    f, r = frm % 8, frm // 8
    out = set()
    for df, dr in ((1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)):
        nf, nr = f + df, r + dr
        while 0 <= nf < 8 and 0 <= nr < 8:
            out.add(nr * 8 + nf)
            nf, nr = nf + df, nr + dr
    for df, dr in ((1, 2), (2, 1), (-1, 2), (-2, 1), (1, -2), (2, -1), (-1, -2), (-2, -1)):
        nf, nr = f + df, r + dr
        if 0 <= nf < 8 and 0 <= nr < 8:
            out.add(nr * 8 + nf)
    return out


def move_space():
    """Every UCI move string a standard game can produce, in token order.

    A move is (from, to) with an optional promotion letter. Every legal move of
    every piece except a pawn promotion lies on a queen line or is a knight jump,
    and a queen line from a square covers a king step, a pawn push or capture,
    a double push, en passant and the castling king move (e1g1 is a rook line).
    Promotions are the pawn moves from the seventh rank to the eighth (white) or
    second to first (black): a push or a capture, each with q, r, b or n.
    1456 queen-line + 336 knight + 176 promotion = 1968 tokens. Chess960 castling
    written king-takes-rook is also a queen line, so it has a token but is not
    claimed (see the spec).
    """
    moves = []
    for frm in range(64):
        for to in sorted(_queen_or_knight_targets(frm)):
            moves.append(SQUARES[frm] + SQUARES[to])
    for frm_rank, to_rank in ((6, 7), (1, 0)):
        for f in range(8):
            for df in (-1, 0, 1):
                if 0 <= f + df < 8:
                    for p in PROMOS:
                        moves.append(SQUARES[frm_rank * 8 + f] + SQUARES[to_rank * 8 + f + df] + p)
    moves.sort(key=lambda m: (_square_index(m[:2]), _square_index(m[2:4]), PROMOS.index(m[4]) + 1 if len(m) == 5 else 0))
    return moves


def _build():
    """Return (tokens, ranges) with ranges {name: (start, end)}."""
    tokens, ranges = [], {}

    def section(name, items):
        start = len(tokens)
        tokens.extend(items)
        ranges[name] = (start, len(tokens))

    section("structure", ["<pos>", "</pos>", "<cgt/1>", "</cgt>", "<nl>", "<dash>", "<board>", "<moves>", "</moves>",
                          "<v:lite>", "<v:full>"])
    section("call", ["<call:submit_move>", "</call>", "<reasoning>", "</reasoning>", "<arg:move>"])
    section("side", ["<side:w>", "<side:b>"])
    section("castle", [f"<cr:{c}>" for c in CASTLE_RIGHTS] + ["<cr:none>"])
    section("square", [f"<sq:{s}>" for s in SQUARES])
    section("piece", [f"<pc:{p}>" for p in PIECES] + ["<pc:.>"])
    section("halfmove", [f"<hm:{n}>" for n in range(MAX_HALFMOVE + 1)])
    section("digit", [f"<d:{n}>" for n in range(10)])
    section("rep", [f"<rep:{n}>" for n in range(MAX_REP + 1)] + ["<rep:?>"])
    section("term", [f"<term:{t}>" for t in TERMS])
    section("cgt_key", [f"<k:{k}>" for k in CGT_KEYS])
    section("cgt_group", [f"<g:{g}>" for g in CGT_GROUPS])
    section("cgt_move", [f"<mv:{m}>" for m in CGT_MOVE_MARKS])
    section("move", [f"<m:{m}>" for m in move_space()])
    pad = -(len(tokens) + RESERVED_MIN) % 64
    section("reserved", [f"<reserved:{i:03d}>" for i in range(RESERVED_MIN + pad)])
    return tokens, ranges


TOKENS, RANGES = _build()
SIZE = len(TOKENS)
ID = {t: i for i, t in enumerate(TOKENS)}
assert len(ID) == SIZE and SIZE % 64 == 0


def vocab_sha256():
    return hashlib.sha256("\n".join(TOKENS).encode("utf-8")).hexdigest()


def manifest():
    return {
        "format": MANIFEST_FORMAT,
        "spec_version": SPEC_VERSION,
        "cgt_version": CGT_VERSION,
        "packet_schema_version": PACKET_SCHEMA_VERSION,
        "rules_revision": RULES_REVISION,
        "vocab_size": SIZE,
        "vocab_sha256": vocab_sha256(),
        "ranges": {name: {"start": a, "end": b} for name, (a, b) in RANGES.items()},
    }


def manifest_json():
    return json.dumps(manifest(), indent=2, sort_keys=True) + "\n"
