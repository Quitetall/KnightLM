"""Host integration: append ctok tokens to a Hugging Face tokenizer and initialise their rows.

`extend_tokenizer` appends the whole vocabulary after the host's last id, so the
ctok id `i` is host id `base + i` with `base = len(tokenizer)` before the call.
`init_new_rows` fills the new rows of an embedding matrix (and, for an untied
model, of lm_head) from the host's own rows; see docs/model/tokenizer.md.
Needs `transformers` (and `torch` for init_new_rows); nothing else imports it.
"""

from model.tokenizer import vocab

PIECE_WORDS = {"K": "white king", "Q": "white queen", "R": "white rook", "B": "white bishop", "N": "white knight",
               "P": "white pawn", "k": "black king", "q": "black queen", "r": "black rook", "b": "black bishop",
               "n": "black knight", "p": "black pawn"}
PROMO_WORDS = {"q": "queen", "r": "rook", "b": "bishop", "n": "knight"}
STRUCTURE_WORDS = {
    "<pos>": "position", "</pos>": "end position", "<cgt/1>": "graph", "</cgt>": "end graph", "<nl>": "\n",
    "<dash>": "-", "<board>": "board", "<moves>": "moves", "</moves>": "end moves", "<v:lite>": "lite",
    "<v:full>": "full", "<call:submit_move>": "submit_move", "</call>": "end call", "<reasoning>": "reasoning",
    "</reasoning>": "end reasoning", "<arg:move>": "move",
}
CGT_MOVE_WORDS = {"x": "captures", "ep": "en passant", "=Q": "promotes to queen", "=R": "promotes to rook",
                  "=B": "promotes to bishop", "=N": "promotes to knight", "+": "check", "#": "checkmate",
                  "O-O": "castles kingside", "O-O-O": "castles queenside"}
CASTLE_WORDS = {"K": "white kingside", "Q": "white queenside", "k": "black kingside", "q": "black queenside",
                "none": "no castling"}
FROM_WEIGHT, TO_WEIGHT = 0.6, 0.4
RESERVED_STD_SCALE = 0.02


def descriptor(token):
    """Short host-language text a token is initialised from, or None for the reserved range.

    A move token has no single descriptor: see `move_parts`.
    """
    body = token[1:-1]
    if token in STRUCTURE_WORDS:
        return STRUCTURE_WORDS[token]
    kind, _, rest = body.partition(":")
    if kind == "sq":
        return rest
    if kind == "pc":
        return "empty" if rest == "." else PIECE_WORDS[rest]
    if kind == "side":
        return "white to move" if rest == "w" else "black to move"
    if kind == "cr":
        return CASTLE_WORDS[rest]
    if kind in ("hm", "d", "rep"):
        return "unknown" if rest == "?" else rest
    if kind == "term":
        return rest.replace("_", " ")
    if kind in ("k", "g"):
        return {"W": "white", "B": "black"}.get(rest, rest)
    if kind == "mv":
        return CGT_MOVE_WORDS.get(rest, rest)
    return None


def move_parts(token):
    """(from square, to square, promotion word or None) for a `<m:...>` token, else None."""
    if not token.startswith("<m:"):
        return None
    uci = token[3:-1]
    return uci[:2], uci[2:4], PROMO_WORDS[uci[4]] if len(uci) == 5 else None


def extend_tokenizer(tokenizer):
    """Append every ctok token; returns `base`. Refuses a tokenizer that already holds one."""
    from tokenizers import AddedToken

    if any(t in tokenizer.get_vocab() for t in vocab.TOKENS[:8]):
        raise ValueError("tokenizer already contains ctok tokens")
    base = len(tokenizer)
    added = tokenizer.add_tokens([AddedToken(t, special=False, normalized=False, lstrip=False, rstrip=False)
                                  for t in vocab.TOKENS])
    if added != vocab.SIZE or len(tokenizer) != base + vocab.SIZE:
        raise ValueError(f"expected {vocab.SIZE} new tokens, got {added}")
    for i in (0, vocab.ID["<sq:e4>"], vocab.SIZE - 1):
        if tokenizer.convert_tokens_to_ids(vocab.TOKENS[i]) != base + i:
            raise ValueError("ctok tokens did not land contiguously after the host vocabulary")
    return base


def _mean_rows(weight, tokenizer, text):
    ids = tokenizer.encode(text, add_special_tokens=False)
    if not ids:
        raise ValueError(f"descriptor {text!r} has no host tokens")
    return weight[ids].float().mean(dim=0)


def init_new_rows(weight, tokenizer, base, seed=0):
    """Fill `weight[base:base+SIZE]` in place from the host rows `weight[:base]`.

    A token with a descriptor gets the mean of the host rows of that text's
    sub-tokens; a move token gets 0.6 * mean(from square text) + 0.4 * mean(to
    square text) (+ the promotion word, averaged in); reserved tokens get the
    host mean row plus small seeded noise. Deterministic for a given host and seed.
    Apply to `embed_tokens.weight` and, when untied, to `lm_head.weight`.
    """
    import torch

    gen = torch.Generator().manual_seed(seed)
    host = weight[:base].float()
    mean, std = host.mean(dim=0), host.std(dim=0)
    with torch.no_grad():
        for i, token in enumerate(vocab.TOKENS):
            parts = move_parts(token)
            if parts:
                row = FROM_WEIGHT * _mean_rows(weight, tokenizer, parts[0]) + TO_WEIGHT * _mean_rows(weight, tokenizer, parts[1])
                if parts[2]:
                    row = (row + _mean_rows(weight, tokenizer, parts[2])) / 2
            else:
                text = descriptor(token)
                if text is None:
                    row = mean + RESERVED_STD_SCALE * std * torch.randn(mean.shape, generator=gen)
                else:
                    row = _mean_rows(weight, tokenizer, text)
            weight[base + i] = row.to(weight.dtype)
