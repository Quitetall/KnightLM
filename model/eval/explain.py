"""Explanation scorecard: mechanically check a free-text explanation against the board.

Given a position (FEN plus the compiler's legal move list, as the corpus packet
carries them) and a model's explanation, extract the claims that can be checked
without judgment and check each one. Claim types:

  piece_on_square  "the knight on f3", "white pawns on e4 and d4", "the e5 pawn"
  square_state     "e4 is empty", "d5 is occupied"
  move_legal       a move named in SAN or UCI is legal for the side to move
  move_capture     a named SAN capture (`Nxe5`, `exd5`) really captures
  move_check       a named move marked `+`/`#` or described as giving check really does
  capture_target   "captures the knight on e5": an enemy piece of that kind stands there
  material         "White has two rooks", "Black is up a pawn", "material is equal"

Nothing here uses an engine or a model, and there is no composite: the result is a
count of checkable claims and failed claims per type, plus a count of claims that
were seen but could not be checked, each with its reason. A mechanical check is
not an understanding check: an explanation can make zero checkable claims, and
the scorecard reports that count too.

Scope caveats (also in docs/model/strength-baseline.md): moves are checked
against the side to move in the given position, so a sentence about a reply or a
hypothetical line ("then Black plays ...", "if ...", "after ...") is counted as
unchecked, never as true or false. Mate (`#`) is checked as check only.
"""

import re

CLAIM_TYPES = ("piece_on_square", "square_state", "move_legal", "move_capture", "move_check",
               "capture_target", "material")
UNCHECKED_REASONS = ("later_ply_or_hypothetical", "mate_not_verified", "move_not_legal",
                     "check_without_named_move")
PIECE_NAMES = {"king": "K", "queen": "Q", "rook": "R", "bishop": "B", "knight": "N", "pawn": "P"}
VALUES = {"P": 1, "N": 3, "B": 3, "R": 5, "Q": 9, "K": 0}
NUMBERS = {"a": 1, "an": 1, "one": 1, "no": 0, "zero": 0, "two": 2, "three": 3, "four": 4, "five": 5,
           "six": 6, "seven": 7, "eight": 8, "nine": 9, "ten": 10}
FILES = "abcdefgh"
KNIGHT = ((1, 2), (2, 1), (-1, 2), (-2, 1), (1, -2), (2, -1), (-1, -2), (-2, -1))
KING = ((1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1))
ROOK_DIRS, BISHOP_DIRS = KING[:4], KING[4:]

PIECE_WORD = r"(king|queen|rook|bishop|knight|pawn)"
SQUARE = r"[a-h][1-8]"
SQUARE_LIST = rf"({SQUARE}(?:\s*(?:,|and|&)\s*(?:and\s+)?{SQUARE})*)"
COLOR = r"(?:(white|black)(?:'s)?\s+)?"
COUNT = r"(a|an|one|no|zero|two|three|four|five|six|seven|eight|nine|ten|\d+)"

RE_ON = re.compile(rf"\b{COLOR}{PIECE_WORD}s?\s+(?:on|at)\s+{SQUARE_LIST}", re.I)
RE_PREFIX = re.compile(rf"\b{COLOR}({SQUARE})[- ]{PIECE_WORD}\b", re.I)
RE_STATE = re.compile(rf"\b({SQUARE})\s+(?:is|are|stays|remains)\s+(empty|vacant|occupied)\b", re.I)
RE_CAPTURE = re.compile(
    rf"\b(?:captur(?:e|es|ing)|tak(?:e|es|ing)|win(?:s|ning)?)\s+(?:the\s+|a\s+|an\s+)?"
    rf"(?:(?:white|black)(?:'s)?\s+)?{PIECE_WORD}\s+(?:on|at)\s+({SQUARE})\b", re.I)
RE_HAS = re.compile(rf"\b(white|black)\s+(?:has|have|owns|keeps|retains)\s+{COUNT}\s+{PIECE_WORD}s?\b", re.I)
RE_LEAD = re.compile(rf"\b(white|black)\s+(?:is|stands|are)\s+(up|ahead|down|behind)\s+(?:by\s+)?{COUNT}\s+"
                     rf"(?:{PIECE_WORD}s?|points?)\b", re.I)
RE_EQUAL = re.compile(r"\b(?:material\s+(?:is|stays|remains)\s+(?:equal|even|level|balanced)"
                      r"|(?:equal|even|level)\s+material)\b", re.I)
RE_CHECK_PHRASE = re.compile(r"\b(?:gives?|giving|delivers?|delivering|with)\s+check\b|\bchecks?\s+the\s+(?:black\s+|white\s+)?king\b"
                             r"|\bputs?\s+the\s+(?:black\s+|white\s+)?king\s+in\s+check\b", re.I)
RE_HYPOTHETICAL = re.compile(r"\b(then|repl(?:y|ies|ied)|respond(?:s|ed)?|response|answers?|after|if|"
                             r"threat\w*|opponent(?:'s)?)\b|\.\.\.|…", re.I)
RE_SENTENCES = re.compile(r"(?<=[.!?])\s+(?=[A-Z0-9(])|\n+")

SAN_CASTLE = re.compile(r"(?<![\w-])(O-O-O|O-O|0-0-0|0-0)(?![\w-])([+#])?")
SAN_PIECE = re.compile(r"(?<![\w])([KQRBN])([a-h])?([1-8])?(x)?([a-h][1-8])(?:=([QRBN]))?([+#])?(?!\w)")
SAN_PAWN_CAPTURE = re.compile(r"(?<![\w])([a-h])x([a-h][1-8])(?:=([QRBN]))?([+#])?(?!\w)")
SAN_PROMOTION = re.compile(r"(?<![\w])([a-h][18])=([QRBN])([+#])?(?!\w)")
SAN_PUSH = re.compile(rf"\b(?:play|plays|played|playing|push|pushes|pushed|pushing)\s+({SQUARE})(?:\+)?(?!\w)",
                      re.I)
UCI_TOKEN = re.compile(r"(?<![\w])([a-h][1-8][a-h][1-8][qrbn]?)(?!\w)")


def number(word):
    word = word.lower()
    return int(word) if word.isdigit() else NUMBERS[word]


class Board:
    """Piece placement, side to move and en passant square from a FEN. No move generator."""

    def __init__(self, fen):
        fields = fen.split()
        if len(fields) < 4:
            raise ValueError(f"not a FEN: {fen!r}")
        self.pieces = {}
        ranks = fields[0].split("/")
        if len(ranks) != 8:
            raise ValueError(f"bad FEN placement: {fen!r}")
        for row, text in zip(range(7, -1, -1), ranks, strict=True):
            col = 0
            for ch in text:
                if ch.isdigit():
                    col += int(ch)
                else:
                    self.pieces[FILES[col] + str(row + 1)] = ("w" if ch.isupper() else "b", ch.upper())
                    col += 1
        self.turn = fields[1]
        self.ep = None if fields[3] == "-" else fields[3]

    def copy(self):
        other = Board.__new__(Board)
        other.pieces, other.turn, other.ep = dict(self.pieces), self.turn, self.ep
        return other

    def attacked(self, square, by):
        file, rank = FILES.index(square[0]), int(square[1]) - 1

        def at(df, dr):
            f, r = file + df, rank + dr
            return self.pieces.get(FILES[f] + str(r + 1)) if 0 <= f < 8 and 0 <= r < 8 else None

        for df, dr in KNIGHT:
            if at(df, dr) == (by, "N"):
                return True
        for df, dr in KING:
            if at(df, dr) == (by, "K"):
                return True
        back = -1 if by == "w" else 1  # a white pawn attacks upward, so it sits one rank below
        for df in (-1, 1):
            if at(df, back) == (by, "P"):
                return True
        for dirs, kinds in ((ROOK_DIRS, "RQ"), (BISHOP_DIRS, "BQ")):
            for df, dr in dirs:
                step = 1
                while True:
                    f, r = file + df * step, rank + dr * step
                    if not (0 <= f < 8 and 0 <= r < 8):
                        break
                    piece = self.pieces.get(FILES[f] + str(r + 1))
                    if piece:
                        if piece[0] == by and piece[1] in kinds:
                            return True
                        break
                    step += 1
        return False

    def after(self, uci):
        """The board after a legal UCI move (castling, en passant and promotion handled)."""
        nxt = self.copy()
        src, dst = uci[:2], uci[2:4]
        piece = nxt.pieces.pop(src)
        if piece[1] == "K" and abs(FILES.index(src[0]) - FILES.index(dst[0])) == 2:
            rank = src[1]
            rook_from, rook_to = ("h" + rank, "f" + rank) if dst[0] == "g" else ("a" + rank, "d" + rank)
            if rook_from in nxt.pieces:
                nxt.pieces[rook_to] = nxt.pieces.pop(rook_from)
        if piece[1] == "P" and src[0] != dst[0] and dst not in nxt.pieces:
            nxt.pieces.pop(dst[0] + src[1], None)  # en passant
        if len(uci) == 5:
            piece = (piece[0], uci[4].upper())
        nxt.pieces[dst] = piece
        nxt.turn = "b" if self.turn == "w" else "w"
        return nxt

    def gives_check(self, uci):
        after = self.after(uci)
        king = next((sq for sq, p in after.pieces.items() if p == (after.turn, "K")), None)
        return king is not None and after.attacked(king, self.turn)

    def is_capture(self, uci):
        src, dst = uci[:2], uci[2:4]
        piece = self.pieces.get(src)
        if dst in self.pieces and self.pieces[dst][0] != self.turn:
            return True
        return bool(piece and piece[1] == "P" and src[0] != dst[0])  # en passant

    def material(self):
        totals = {"w": 0, "b": 0}
        for color, kind in self.pieces.values():
            totals[color] += VALUES[kind]
        return totals

    def count(self, color, kind):
        return sum(1 for p in self.pieces.values() if p == (color, kind))


class Result:
    def __init__(self):
        self.types = {t: {"checkable": 0, "failed": 0} for t in CLAIM_TYPES}
        self.unchecked = dict.fromkeys(UNCHECKED_REASONS, 0)
        self.failures = []

    def claim(self, kind, ok, text, reason=""):
        self.types[kind]["checkable"] += 1
        if not ok:
            self.types[kind]["failed"] += 1
            self.failures.append({"type": kind, "claim": text, "reason": reason})

    def skip(self, reason):
        self.unchecked[reason] += 1

    def as_dict(self):
        return {"claim_types": self.types, "unchecked": self.unchecked, "failures": self.failures,
                "checkable_total": sum(v["checkable"] for v in self.types.values()),
                "failed_total": sum(v["failed"] for v in self.types.values())}


def resolve(board, legal_moves, token):
    """Legal UCI moves a SAN/UCI token can mean. Empty means the token names no legal move."""
    kind = token["kind"]
    if kind == "uci":
        return [token["uci"]] if token["uci"] in legal_moves else []
    out = []
    for uci in legal_moves:
        piece = board.pieces.get(uci[:2])
        if piece is None or piece[0] != board.turn:
            continue
        if kind == "castle":
            home = "1" if board.turn == "w" else "8"
            want = "g" if token["short"] else "c"
            if piece[1] == "K" and uci[:2] == "e" + home and uci[2:4] == want + home:
                out.append(uci)
            continue
        if piece[1] != token["piece"] or uci[2:4] != token["dest"]:
            continue
        if token.get("file") and uci[0] != token["file"]:
            continue
        if token.get("rank") and uci[1] != token["rank"]:
            continue
        promo = uci[4].upper() if len(uci) == 5 else None
        if promo != token.get("promo"):
            continue
        if piece[1] == "P" and not token.get("file") and uci[0] != token["dest"][0]:
            continue  # a pawn push stays on its file; `exd5` carries its file
        out.append(uci)
    return out


def move_tokens(sentence):
    """Moves named in a sentence, in text order: (position, token dict)."""
    found = []
    for m in SAN_CASTLE.finditer(sentence):
        found.append((m.start(), m.group(0), {"kind": "castle", "short": m.group(1).count("-") == 1,
                                              "suffix": m.group(2)}))
    taken = [(s, s + len(t)) for s, t, _ in found]

    def free(m):
        return not any(a <= m.start() < b for a, b in taken)

    for m in SAN_PIECE.finditer(sentence):
        if free(m):
            found.append((m.start(), m.group(0), {
                "kind": "san", "piece": m.group(1), "file": m.group(2), "rank": m.group(3),
                "capture": bool(m.group(4)), "dest": m.group(5), "promo": m.group(6), "suffix": m.group(7)}))
            taken.append((m.start(), m.end()))
    for m in SAN_PAWN_CAPTURE.finditer(sentence):
        if free(m):
            found.append((m.start(), m.group(0), {
                "kind": "san", "piece": "P", "file": m.group(1), "rank": None, "capture": True,
                "dest": m.group(2), "promo": m.group(3), "suffix": m.group(4)}))
            taken.append((m.start(), m.end()))
    for m in SAN_PROMOTION.finditer(sentence):
        if free(m):
            found.append((m.start(), m.group(0), {
                "kind": "san", "piece": "P", "file": m.group(1)[0], "rank": None, "capture": False,
                "dest": m.group(1), "promo": m.group(2), "suffix": m.group(3)}))
            taken.append((m.start(), m.end()))
    for m in UCI_TOKEN.finditer(sentence):
        if free(m):
            found.append((m.start(), m.group(0), {"kind": "uci", "uci": m.group(1), "suffix": None}))
            taken.append((m.start(), m.end()))
    for m in SAN_PUSH.finditer(sentence):
        square = m.group(1).lower()
        if not any(a <= m.start(1) < b for a, b in taken):
            found.append((m.start(1), square, {"kind": "san", "piece": "P", "file": None, "rank": None,
                                                "capture": False, "dest": square, "promo": None,
                                                "suffix": None}))
    return sorted(found, key=lambda item: item[0])


def split_sentences(text):
    return [s for s in RE_SENTENCES.split(text) if s and s.strip()]


def check_squares(board, sentence, result):
    spans = [m.span() for m in RE_CAPTURE.finditer(sentence)]  # a capture claim is counted once, as such
    for m in RE_ON.finditer(sentence):
        if any(a <= m.start() < b or m.start() <= a < m.end() for a, b in spans):
            continue
        color, piece_word, squares = m.group(1), m.group(2).lower(), m.group(3)
        for square in re.findall(SQUARE, squares):
            want = PIECE_NAMES[piece_word]
            have = board.pieces.get(square)
            ok = have is not None and have[1] == want and (color is None or have[0] == color.lower()[0])
            result.claim("piece_on_square", ok, f"{color + ' ' if color else ''}{piece_word} on {square}",
                         "" if ok else f"{square} holds {describe(have)}")
    for m in RE_PREFIX.finditer(sentence):
        color, square, piece_word = m.group(1), m.group(2).lower(), m.group(3).lower()
        have = board.pieces.get(square)
        ok = have is not None and have[1] == PIECE_NAMES[piece_word] and (
            color is None or have[0] == color.lower()[0])
        result.claim("piece_on_square", ok, f"{square} {piece_word}", "" if ok else f"{square} holds {describe(have)}")
    for m in RE_STATE.finditer(sentence):
        square, state = m.group(1).lower(), m.group(2).lower()
        empty = square not in board.pieces
        ok = empty if state in ("empty", "vacant") else not empty
        result.claim("square_state", ok, f"{square} is {state}",
                     "" if ok else f"{square} holds {describe(board.pieces.get(square))}")


def describe(piece):
    if piece is None:
        return "nothing"
    return f"a {'white' if piece[0] == 'w' else 'black'} {next(k for k, v in PIECE_NAMES.items() if v == piece[1])}"


def check_material(board, sentence, result):
    totals = board.material()
    for m in RE_HAS.finditer(sentence):
        color, count, piece_word = m.group(1).lower()[0], number(m.group(2)), m.group(3).lower()
        actual = board.count(color, PIECE_NAMES[piece_word])
        result.claim("material", actual == count, f"{m.group(0)}", "" if actual == count else f"actual count {actual}")
    for m in RE_LEAD.finditer(sentence):
        color, direction, count = m.group(1).lower()[0], m.group(2).lower(), number(m.group(3))
        unit = VALUES[PIECE_NAMES[m.group(4).lower()]] if m.group(4) else 1
        claimed = count * unit
        diff = totals[color] - totals["b" if color == "w" else "w"]
        ok = diff == (claimed if direction in ("up", "ahead") else -claimed)
        result.claim("material", ok, m.group(0), "" if ok else f"actual lead for {m.group(1).lower()} is {diff} points")
    if RE_EQUAL.search(sentence):
        ok = totals["w"] == totals["b"]
        result.claim("material", ok, "material is equal", "" if ok else f"white {totals['w']} vs black {totals['b']} points")


def check_sentence(board, legal_moves, sentence, move, result):
    check_squares(board, sentence, result)
    check_material(board, sentence, result)
    hypothetical = bool(RE_HYPOTHETICAL.search(sentence))
    for m in RE_CAPTURE.finditer(sentence):
        if hypothetical:
            result.skip("later_ply_or_hypothetical")
            continue
        piece_word, square = m.group(1).lower(), m.group(2).lower()
        have = board.pieces.get(square)
        ok = have is not None and have[1] == PIECE_NAMES[piece_word] and have[0] != board.turn
        result.claim("capture_target", ok, m.group(0), "" if ok else f"{square} holds {describe(have)}")
    tokens = move_tokens(sentence)
    check_phrase = RE_CHECK_PHRASE.search(sentence)
    named_moves = []
    for _, text, token in tokens:
        if hypothetical:
            result.skip("later_ply_or_hypothetical")
            continue
        candidates = resolve(board, legal_moves, token)
        result.claim("move_legal", bool(candidates), text, "" if candidates else "no legal move matches")
        if not candidates:
            if token.get("capture") or token.get("suffix"):
                result.skip("move_not_legal")
            continue
        named_moves.append(candidates[0])
        if token.get("capture"):
            ok = all(board.is_capture(c) for c in candidates)
            result.claim("move_capture", ok, text, "" if ok else "the move captures nothing")
        if token.get("suffix"):
            ok = all(board.gives_check(c) for c in candidates)
            result.claim("move_check", ok, text, "" if ok else "the move does not give check")
            if token["suffix"] == "#":
                result.skip("mate_not_verified")
    if check_phrase and not hypothetical and not any(t.get("suffix") for _, _, t in tokens):
        subject = named_moves[0] if named_moves else move
        if subject and subject in legal_moves:
            ok = board.gives_check(subject)
            result.claim("move_check", ok, f"{subject}: {check_phrase.group(0)}",
                         "" if ok else "the move does not give check")
        else:
            result.skip("check_without_named_move")


def check_explanation(fen, legal_moves, text, move=None):
    """Check one explanation. `move` is the model's committed move, used only to
    attribute a "gives check" sentence that names no move. Returns a plain dict."""
    board = Board(fen)
    legal = set(legal_moves)
    result = Result()
    for sentence in split_sentences(text or ""):
        check_sentence(board, legal, sentence, move, result)
    return result.as_dict()


def aggregate(per_row):
    """Sum per-explanation results into the scorecard axis. Counts only; no composite.

    `per_row` holds one result dict per requested row, or None where the model gave no
    explanation (counted as `missing`, never dropped from the denominator)."""
    n = len(per_row)
    types = {t: {"checkable": 0, "failed": 0} for t in CLAIM_TYPES}
    unchecked = dict.fromkeys(UNCHECKED_REASONS, 0)
    with_failure = without_claims = missing = 0
    for result in per_row:
        if result is None:
            missing += 1
            continue
        for kind in CLAIM_TYPES:
            for field in ("checkable", "failed"):
                types[kind][field] += result["claim_types"][kind][field]
        for reason in UNCHECKED_REASONS:
            unchecked[reason] += result["unchecked"][reason]
        with_failure += result["failed_total"] > 0
        without_claims += result["checkable_total"] == 0
    return {
        "status": "scored", "n": n, "explanations_missing": missing,
        "explanations_with_a_failed_claim": with_failure,
        "explanations_without_checkable_claims": without_claims,
        "claim_types": types, "unchecked_claims": unchecked,
        "checkable_total": sum(v["checkable"] for v in types.values()),
        "failed_total": sum(v["failed"] for v in types.values()),
        "note": "counts only; no composite. An explanation with no checkable claim is not a true one.",
    }
