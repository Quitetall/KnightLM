"""T3: Chess Graph Text (cgt/1) <-> token ids.

The text is the compiler's canonical rendering (docs/model/graph-text.md). Each
line becomes: a key token, then its items, then `<nl>`; a document opens with
`<cgt/1> <v:lite|full>` and closes with `</cgt>`. A node `Nb1` is two tokens,
`<pc:N> <sq:b1>`; the `:` after a line's source node and the single spaces
between items are implied and put back by `decode`. A move destination is
`[<mv:x>] <sq:..> [<mv:ep>] [<mv:=Q>] [<mv:+>|<mv:#>]`, castling is
`<mv:O-O>` or `<mv:O-O-O>` (then an optional check mark).

`encode` refuses any text that is not exactly canonical; `decode` refuses any
id sequence that is not the encoding of one canonical document. Both check the
order rules of the spec (node order, destination order) but know no chess:
whether the facts are true is `parse_cgt` in the Rust compiler.
"""

import re

from model.tokenizer import vocab
from model.tokenizer.errors import TokenizerError

ID = vocab.ID
_NL = ID["<nl>"]
_END = ID["</cgt>"]
_DASH = ID["<dash>"]
_PIECE_OF_ID = {ID[f"<pc:{p}>"]: p for p in vocab.PIECES}
_SQ_OF_ID = {ID[f"<sq:{s}>"]: s for s in vocab.SQUARES}
_KEY_OF_ID = {ID[f"<k:{k}>"]: k for k in vocab.CGT_KEYS}
_GROUP_OF_ID = {ID[f"<g:{g}>"]: g for g in vocab.CGT_GROUPS}
_MARK_OF_ID = {ID[f"<mv:{m}>"]: m for m in vocab.CGT_MOVE_MARKS}
_TERM_OF_ID = {ID[f"<term:{t}>"]: t for t in vocab.TERMS}
_CR_OF_ID = {ID[f"<cr:{c}>"]: c for c in vocab.CASTLE_RIGHTS}
_HM_FIRST = vocab.RANGES["halfmove"][0]
_DIGIT_FIRST = vocab.RANGES["digit"][0]
_REP_FIRST = vocab.RANGES["rep"][0]
_ROLE_ORDER = "KQRBN P".replace(" ", "")
_NODE = re.compile(r"^[KQRBNPkqrbnp][a-h][1-8]$")
_ITEM = re.compile(r"^(?:(O-O-O|O-O)([+#])?|(x)?([a-h][1-8])(ep)?(=[QRBN])?([+#])?)$")


def _bad(message):
    raise TokenizerError(message)


def node_key(node):
    """Canonical node order: white then black, K Q R B N P, file a-h, rank 1-8."""
    piece = node[0]
    return (piece.islower(), _ROLE_ORDER.index(piece.upper()), node[1], node[2])


def _sq_key(sq):
    return (sq[0], sq[1])


def _dest_key(item):
    m = _ITEM.match(item)
    if m.group(1):
        return (1, 0, 0, 0, 0 if m.group(1) == "O-O" else 1)
    promo = m.group(6)
    return (0, vocab.FILES.index(m.group(4)[0]), vocab.RANKS.index(m.group(4)[1]),
            0 if not promo else "QRBN".index(promo[1]) + 1, 0)


def _node_ids(node):
    return [ID[f"<pc:{node[0]}>"], ID[f"<sq:{node[1:]}>"]]


def _item_ids(item):
    m = _ITEM.match(item)
    if m is None:
        _bad(f"not a move destination: {item!r}")
    if m.group(1):
        out = [ID[f"<mv:{m.group(1)}>"]]
        if m.group(2):
            out.append(ID[f"<mv:{m.group(2)}>"])
        return out
    out = [ID["<mv:x>"]] if m.group(3) else []
    out.append(ID[f"<sq:{m.group(4)}>"])
    if m.group(5):
        out.append(ID["<mv:ep>"])
    if m.group(6):
        out.append(ID[f"<mv:{m.group(6)}>"])
    if m.group(7):
        out.append(ID[f"<mv:{m.group(7)}>"])
    return out


def _nodes(text_items):
    for t in text_items:
        if not _NODE.match(t):
            _bad(f"not a node: {t!r}")
    return [i for t in text_items for i in _node_ids(t)]


def _number_ids(text, low, high):
    if not (text.isdigit() and str(int(text)) == text and low <= int(text) <= high):
        _bad(f"number out of range: {text!r}")
    return int(text)


def encode(text):
    """Canonical cgt/1 text -> list of ids."""
    if not isinstance(text, str) or text.endswith("\n") or "\r" in text:
        _bad("cgt text must be LF-separated with no trailing newline")
    lines = text.split("\n")
    head = lines[0]
    if head not in ("cgt/1 lite", "cgt/1 full"):
        _bad(f"unsupported or malformed header: {head!r}")
    ids = [ID["<cgt/1>"], ID["<v:" + head.split(" ")[1] + ">"]]
    section = None
    for line in lines[1:]:
        ids.append(_NL)
        words = line.split(" ")
        w0 = words[0]
        if w0 == "turn" and len(words) == 2 and words[1] in ("w", "b"):
            ids += [ID["<k:turn>"], ID[f"<side:{words[1]}>"]]
        elif w0 == "castle" and len(words) == 2:
            rights = words[1]
            if rights == "-":
                ids += [ID["<k:castle>"], ID["<cr:none>"]]
            elif rights and all(c in vocab.CASTLE_RIGHTS for c in rights):
                ids += [ID["<k:castle>"]] + [ID[f"<cr:{c}>"] for c in rights]
            else:
                _bad(f"unsupported castling field {rights!r}")
        elif w0 == "ep" and len(words) == 2:
            ids += [ID["<k:ep>"], _DASH if words[1] == "-" else ID.get(f"<sq:{words[1]}>", -1)]
            if ids[-1] < 0:
                _bad(f"bad ep square {words[1]!r}")
        elif w0 == "clock" and len(words) == 3:
            hm = _number_ids(words[1], 0, vocab.MAX_HALFMOVE)
            _number_ids(words[2], 1, 99999)
            ids += [ID["<k:clock>"], _HM_FIRST + hm] + [_DIGIT_FIRST + int(d) for d in words[2]]
        elif w0 == "rep" and len(words) == 2:
            ids += [ID["<k:rep>"], ID["<rep:?>"] if words[1] == "?" else _REP_FIRST + _number_ids(words[1], 0, vocab.MAX_REP)]
        elif w0 == "term" and len(words) == 2 and f"<term:{words[1]}>" in ID:
            ids += [ID["<k:term>"], ID[f"<term:{words[1]}>"]]
        elif w0 in ("W:", "B:"):
            ids += [ID[f"<k:{w0[0]}>"]] + _nodes(words[1:])
            section = None
        elif line in ("moves:", "atk:", "def:", "pin:", "ksafe:"):
            ids.append(ID[f"<k:{line[:-1]}>"])
            section = line[:-1]
        elif w0 in ("hang:", "check:") and len(words) >= 2:
            ids.append(ID[f"<k:{w0[:-1]}>"])
            ids += [_DASH] if words[1:] == ["-"] else _nodes(words[1:])
            section = None
        elif section and w0.endswith(":") and _NODE.match(w0[:-1]):
            ids += _node_ids(w0[:-1])
            rest = words[1:]
            if section == "moves":
                for item in rest:
                    ids += _item_ids(item)
            elif section in ("atk", "def", "pin"):
                ids += _nodes(rest)
            else:  # ksafe
                for item in rest:
                    if item in vocab.CGT_GROUPS:
                        ids.append(ID[f"<g:{item}>"])
                    elif f"<sq:{item}>" in ID:
                        ids.append(ID[f"<sq:{item}>"])
                    else:
                        _bad(f"bad ksafe item {item!r}")
        else:
            _bad(f"unrecognised line {line!r}")
    ids.append(_END)
    if decode(ids) != text:
        _bad("text is not in canonical cgt/1 form")
    return ids


class _Reader:
    def __init__(self, ids):
        self.ids, self.pos = ids, 0

    def peek(self, k=0):
        i = self.pos + k
        return self.ids[i] if i < len(self.ids) else None

    def take(self):
        if self.pos >= len(self.ids):
            _bad("sequence ends early")
        self.pos += 1
        return self.ids[self.pos - 1]

    def expect(self, tok):
        if self.take() != ID[tok]:
            _bad(f"expected {tok} at index {self.pos - 1}")

    def node(self):
        p = self.take()
        if p not in _PIECE_OF_ID:
            _bad(f"expected a piece token at index {self.pos - 1}")
        s = self.take()
        if s not in _SQ_OF_ID:
            _bad(f"expected a square token at index {self.pos - 1}")
        return _PIECE_OF_ID[p] + _SQ_OF_ID[s]

    def at_node(self, k=0):
        return self.peek(k) in _PIECE_OF_ID

    def nodes(self, minimum=0, side=None, ordered=True):
        out = []
        while self.at_node():
            out.append(self.node())
        if len(out) < minimum:
            _bad("a line needs more nodes")
        if ordered:
            _strictly_increasing(out, node_key, "nodes")
        if side == "W" and any(n[0].islower() for n in out) or side == "B" and any(n[0].isupper() for n in out):
            _bad("a piece is on the wrong colour's list")
        return out

    def item(self):
        out = ""
        tok = self.take()
        if tok in _MARK_OF_ID and _MARK_OF_ID[tok] in ("O-O", "O-O-O"):
            out = _MARK_OF_ID[tok]
        else:
            if tok == ID["<mv:x>"]:
                out, tok = "x", self.take()
            if tok not in _SQ_OF_ID:
                _bad(f"expected a destination square at index {self.pos - 1}")
            out += _SQ_OF_ID[tok]
            if self.peek() == ID["<mv:ep>"]:
                self.pos += 1
                out += "ep"
            if self.peek() in _MARK_OF_ID and _MARK_OF_ID[self.peek()].startswith("="):
                out += _MARK_OF_ID[self.take()]
        if self.peek() in _MARK_OF_ID and _MARK_OF_ID[self.peek()] in ("+", "#"):
            out += _MARK_OF_ID[self.take()]
        return out


def _strictly_increasing(items, key, what):
    keys = [key(i) for i in items]
    if any(a >= b for a, b in zip(keys, keys[1:])):
        _bad(f"{what} are not in strictly increasing canonical order")


def _number(r, token_range_first, count):
    tok = r.take()
    if not token_range_first <= tok < token_range_first + count:
        _bad("expected a number token")
    return tok - token_range_first


def decode(ids):
    """ids -> canonical text; raises TokenizerError on anything else."""
    ids = list(ids)
    for x in ids:
        if not isinstance(x, int) or isinstance(x, bool) or not 0 <= x < vocab.SIZE:
            _bad(f"id out of the ctok range: {x!r}")
    r = _Reader(ids)
    lines = []
    r.expect("<cgt/1>")
    v = r.take()
    if v not in (ID["<v:lite>"], ID["<v:full>"]):
        _bad("expected a variant token")
    full = v == ID["<v:full>"]
    lines.append("cgt/1 " + ("full" if full else "lite"))

    def key_line(name):
        r.expect("<nl>")
        r.expect(f"<k:{name}>")

    key_line("turn")
    t = r.take()
    if t not in (ID["<side:w>"], ID["<side:b>"]):
        _bad("expected a side token")
    lines.append("turn " + ("w" if t == ID["<side:w>"] else "b"))
    key_line("castle")
    t = r.take()
    if t == ID["<cr:none>"]:
        castle = "-"
    elif t in _CR_OF_ID:
        rights = [_CR_OF_ID[t]]
        while r.peek() in _CR_OF_ID:
            rights.append(_CR_OF_ID[r.take()])
        if "".join(sorted(set(rights), key=vocab.CASTLE_RIGHTS.index)) != "".join(rights):
            _bad("castling rights are not unique and in KQkq order")
        castle = "".join(rights)
    else:
        _bad("expected castling tokens")
    lines.append("castle " + castle)
    key_line("ep")
    t = r.take()
    if t == _DASH:
        lines.append("ep -")
    elif t in _SQ_OF_ID and _SQ_OF_ID[t][1] in "36":
        lines.append("ep " + _SQ_OF_ID[t])
    else:
        _bad("expected an en passant token")
    key_line("clock")
    hm = _number(r, _HM_FIRST, vocab.MAX_HALFMOVE + 1)
    digits = ""
    while r.peek() is not None and _DIGIT_FIRST <= r.peek() < _DIGIT_FIRST + 10:
        digits += str(r.take() - _DIGIT_FIRST)
    if not digits or digits[0] == "0" or len(digits) > 5:
        _bad("bad fullmove digits")
    lines.append(f"clock {hm} {digits}")
    key_line("rep")
    t = r.take()
    if t == ID["<rep:?>"]:
        lines.append("rep ?")
    elif _REP_FIRST <= t <= _REP_FIRST + vocab.MAX_REP:
        lines.append(f"rep {t - _REP_FIRST}")
    else:
        _bad("expected a repetition token")
    if r.peek() == _NL and r.peek(1) == ID["<k:term>"]:
        r.pos += 2
        t = r.take()
        if t not in _TERM_OF_ID:
            _bad("expected a terminal-reason token")
        lines.append("term " + _TERM_OF_ID[t])
    for colour in ("W", "B"):
        key_line(colour)
        lines.append(f"{colour}:" + "".join(" " + n for n in r.nodes(side=colour)))
    key_line("moves")
    lines.append("moves:")
    sources = []
    while r.peek() == _NL and r.at_node(1):
        r.pos += 1
        src = r.node()
        items = []
        while r.peek() not in (_NL, _END, None):
            items.append(r.item())
        if not items:
            _bad("a moves line needs a destination")
        _strictly_increasing(items, _dest_key, "destinations")
        sources.append(src)
        lines.append(f"{src}: " + " ".join(items))
    _strictly_increasing(sources, node_key, "moves lines")
    if full:
        for name in ("atk", "def", "pin"):
            key_line(name)
            lines.append(f"{name}:")
            srcs, keys = [], []
            while r.peek() == _NL and r.at_node(1):
                r.pos += 1
                src = r.node()
                targets = r.nodes(minimum=1, ordered=name != "pin")
                if name == "pin":
                    if len(targets) != 2:
                        _bad("a pin line has a pinner and a king")
                    keys.append((node_key(src), tuple(targets)))
                else:
                    keys.append((node_key(src),))
                srcs.append(src)
                lines.append(f"{src}: " + " ".join(targets))
            if any(a >= b for a, b in zip(keys, keys[1:])):
                _bad(f"{name} lines are not in canonical order")
        for name in ("hang", "check"):
            key_line(name)
            if r.peek() == _DASH:
                r.pos += 1
                lines.append(f"{name}: -")
            else:
                lines.append(f"{name}: " + " ".join(r.nodes(minimum=1)))
        key_line("ksafe")
        lines.append("ksafe:")
        srcs = []
        while r.peek() == _NL and r.at_node(1):
            r.pos += 1
            src = r.node()
            parts, seen = [], []
            while r.peek() in _GROUP_OF_ID:
                group = _GROUP_OF_ID[r.take()]
                sqs = []
                while r.peek() in _SQ_OF_ID:
                    sqs.append(_SQ_OF_ID[r.take()])
                if not sqs:
                    _bad("a ksafe group needs squares")
                _strictly_increasing(sqs, _sq_key, "ksafe squares")
                seen.append(group)
                parts.append(group + "".join(" " + s for s in sqs))
            if seen != [g for g in vocab.CGT_GROUPS if g in seen]:
                _bad("ksafe groups are not in open, hit, own order")
            srcs.append(src)
            lines.append(f"{src}:" + "".join(" " + p for p in parts))
        _strictly_increasing(srcs, node_key, "ksafe lines")
    r.expect("</cgt>")
    if r.pos != len(ids):
        _bad("tokens after </cgt>")
    return "\n".join(lines)
