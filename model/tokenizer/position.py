"""T1: a position record <-> board-native token ids.

Sequence (all ids local to the ctok vocabulary):

    <pos> <side:x> <cr:..>+ ep hm digits+ rep [<term:..>] <board> p*64 [<moves> m* </moves>] </pos>

`ep` is `<dash>` or one `<sq:..>`; `hm` is the exact halfmove clock token;
`digits` is the fullmove number in decimal; the 64 piece tokens run in FEN order
(a8 b8 .. h8, a7 .. h1). The record is what the packet carries about the
position: {"fen", "repetition_count" (None = unknown), "legal_moves" (optional
list, packet order), "terminal_reason" (optional)}. Nothing here knows the rules
of chess: legality is the compiler's, and `decode` only refuses sequences that
are not in this exact shape.
"""

from model.tokenizer import vocab
from model.tokenizer.errors import TokenizerError
from model.tokenizer.moves import id_to_move, is_move_id, move_to_id

ID = vocab.ID
_PIECE_FIRST, _ = vocab.RANGES["piece"]
_PIECE_OF_ID = {ID[f"<pc:{p}>"]: p for p in vocab.PIECES}
_ID_OF_PIECE = {p: ID[f"<pc:{p}>"] for p in vocab.PIECES}
EMPTY = ID["<pc:.>"]
_HM_FIRST = vocab.RANGES["halfmove"][0]
_DIGIT_FIRST = vocab.RANGES["digit"][0]
_REP_FIRST = vocab.RANGES["rep"][0]
_TERM_IDS = {ID[f"<term:{t}>"]: t for t in vocab.TERMS}
_SQ_OF_ID = {ID[f"<sq:{s}>"]: s for s in vocab.SQUARES}
_CR_OF_ID = {ID[f"<cr:{c}>"]: c for c in vocab.CASTLE_RIGHTS}


def _bad(message):
    raise TokenizerError(message)


def parse_fen(fen):
    """(squares[64] in FEN order, side, castling, ep, halfmove, fullmove); strict, six fields."""
    if not isinstance(fen, str):
        _bad("fen must be a string")
    fields = fen.split(" ")
    if len(fields) != 6:
        _bad(f"fen needs six fields (clocks are never invented): {fen!r}")
    placement, side, castling, ep, hm, fm = fields
    rows = placement.split("/")
    if len(rows) != 8:
        _bad(f"fen placement needs 8 ranks: {fen!r}")
    squares = []
    for row in rows:
        width = 0
        for ch in row:
            if ch in "12345678":
                squares.extend(["."] * int(ch))
                width += int(ch)
            elif ch in vocab.PIECES:
                squares.append(ch)
                width += 1
            else:
                _bad(f"bad placement character {ch!r}: {fen!r}")
        if width != 8:
            _bad(f"fen rank is not 8 wide: {fen!r}")
    # a rank such as "44" or "17" writes the same squares as "8"; only the compact form is canonical
    if to_fen_placement(squares) != placement:
        _bad(f"fen placement is not in compact form: {fen!r}")
    if side not in ("w", "b"):
        _bad(f"bad side to move in {fen!r}")
    if castling != "-" and (not castling or any(c not in vocab.CASTLE_RIGHTS for c in castling)
                            or "".join(sorted(set(castling), key=vocab.CASTLE_RIGHTS.index)) != castling):
        _bad(f"unsupported castling field {castling!r} (standard KQkq only)")
    if ep != "-":
        if ep not in vocab.SQUARES or ep[1] != ("6" if side == "w" else "3"):
            _bad(f"en passant square {ep!r} does not fit side {side!r}")
    if not (hm.isdigit() and str(int(hm)) == hm and int(hm) <= vocab.MAX_HALFMOVE):
        _bad(f"halfmove clock out of range: {hm!r}")
    if not (fm.isdigit() and str(int(fm)) == fm and 1 <= int(fm) <= 99999):
        _bad(f"fullmove number out of range: {fm!r}")
    return squares, side, castling, ep, int(hm), int(fm)


def to_fen_placement(squares):
    rows = []
    for r in range(8):
        out, run = "", 0
        for piece in squares[r * 8:(r + 1) * 8]:
            if piece == ".":
                run += 1
            else:
                out += (str(run) if run else "") + piece
                run = 0
        rows.append(out + (str(run) if run else ""))
    return "/".join(rows)


def packet_to_record(packet, with_moves=True):
    """The record a packet (dict) contributes; the packet's derived facts are not carried."""
    facts = packet.get("facts", {})
    record = {"fen": packet["fen"], "repetition_count": facts.get("repetition_count")}
    if with_moves:
        record["legal_moves"] = list(packet["legal_moves"])
    if facts.get("terminal_reason"):
        record["terminal_reason"] = facts["terminal_reason"]
    return record


def encode(record):
    squares, side, castling, ep, hm, fm = parse_fen(record["fen"])
    rep = record.get("repetition_count")
    if rep is not None and not (isinstance(rep, int) and not isinstance(rep, bool) and 0 <= rep <= vocab.MAX_REP):
        _bad(f"repetition count out of range: {rep!r}")
    ids = [ID["<pos>"], ID[f"<side:{side}>"]]
    ids += [ID["<cr:none>"]] if castling == "-" else [ID[f"<cr:{c}>"] for c in castling]
    ids.append(ID["<dash>"] if ep == "-" else ID[f"<sq:{ep}>"])
    ids.append(_HM_FIRST + hm)
    ids += [_DIGIT_FIRST + int(d) for d in str(fm)]
    ids.append(ID["<rep:?>"] if rep is None else _REP_FIRST + rep)
    term = record.get("terminal_reason")
    if term is not None:
        if term not in vocab.TERMS:
            _bad(f"unknown terminal reason {term!r}")
        ids.append(ID[f"<term:{term}>"])
    ids.append(ID["<board>"])
    ids += [_ID_OF_PIECE[p] if p != "." else EMPTY for p in squares]
    moves = record.get("legal_moves")
    if moves is not None:
        if list(moves) != sorted(set(moves)):
            _bad("legal_moves must be unique and in packet order (ascending UCI string)")
        ids.append(ID["<moves>"])
        ids += [move_to_id(m) for m in moves]
        ids.append(ID["</moves>"])
    ids.append(ID["</pos>"])
    return ids


def decode(ids):
    """ids -> record; raises TokenizerError on anything but one canonical sequence."""
    ids = list(ids)
    n = len(ids)
    for x in ids:
        if not isinstance(x, int) or isinstance(x, bool) or not 0 <= x < vocab.SIZE:
            _bad(f"id out of the ctok range: {x!r}")
    pos = 0

    def take():
        nonlocal pos
        if pos >= n:
            _bad("sequence ends early")
        pos += 1
        return ids[pos - 1]

    def expect(name):
        if take() != ID[name]:
            _bad(f"expected {name} at index {pos - 1}")

    expect("<pos>")
    side = take()
    if side not in (ID["<side:w>"], ID["<side:b>"]):
        _bad("expected a side token")
    side = "w" if side == ID["<side:w>"] else "b"
    tok = take()
    if tok == ID["<cr:none>"]:
        castling = "-"
    elif tok in _CR_OF_ID:
        rights = [_CR_OF_ID[tok]]
        while pos < n and ids[pos] in _CR_OF_ID:
            rights.append(_CR_OF_ID[take()])
        castling = "".join(rights)  # order and uniqueness are checked by parse_fen below
    else:
        _bad("expected castling tokens")
    tok = take()
    if tok == ID["<dash>"]:
        ep = "-"
    elif tok in _SQ_OF_ID:
        ep = _SQ_OF_ID[tok]
    else:
        _bad("expected an en passant token")
    tok = take()
    hm = tok - _HM_FIRST
    if not 0 <= hm <= vocab.MAX_HALFMOVE:
        _bad("expected a halfmove token")
    digits = ""
    while pos < n and _DIGIT_FIRST <= ids[pos] < _DIGIT_FIRST + 10:
        digits += str(ids[pos] - _DIGIT_FIRST)
        pos += 1
    if not digits or digits[0] == "0" or len(digits) > 5:
        _bad("bad fullmove digits")
    tok = take()
    if tok == ID["<rep:?>"]:
        rep = None
    elif _REP_FIRST <= tok <= _REP_FIRST + vocab.MAX_REP:
        rep = tok - _REP_FIRST
    else:
        _bad("expected a repetition token")
    term = None
    if pos < n and ids[pos] in _TERM_IDS:
        term = _TERM_IDS[take()]
    expect("<board>")
    squares = []
    for _ in range(64):
        tok = take()
        if tok == EMPTY:
            squares.append(".")
        elif tok in _PIECE_OF_ID:
            squares.append(_PIECE_OF_ID[tok])
        else:
            _bad("expected a piece token on the board")
    moves = None
    if pos < n and ids[pos] == ID["<moves>"]:
        pos += 1
        moves = []
        while True:
            tok = take()
            if tok == ID["</moves>"]:
                break
            if not is_move_id(tok):
                _bad("expected a move token")
            moves.append(id_to_move(tok))
        if moves != sorted(set(moves)):
            _bad("moves are not unique and in ascending UCI order")
    expect("</pos>")
    if pos != n:
        _bad("tokens after </pos>")
    ep_part = ep
    fen = f"{to_fen_placement(squares)} {side} {castling} {ep_part} {hm} {int(digits)}"
    parse_fen(fen)  # re-checks ep rank against the side
    record = {"fen": fen, "repetition_count": rep}
    if moves is not None:
        record["legal_moves"] = moves
    if term is not None:
        record["terminal_reason"] = term
    return record
