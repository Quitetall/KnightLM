"""UCI move <-> single move token."""

from model.tokenizer import vocab
from model.tokenizer.errors import TokenizerError

_FIRST, _LAST = vocab.RANGES["move"]
UCI = [t[3:-1] for t in vocab.TOKENS[_FIRST:_LAST]]  # "<m:e2e4>" -> "e2e4"
_TOKEN_OF = {uci: _FIRST + i for i, uci in enumerate(UCI)}


def move_to_id(uci):
    try:
        return _TOKEN_OF[uci]
    except (KeyError, TypeError):
        raise TokenizerError(f"not a UCI move in the ctok/1 move space: {uci!r}") from None


def id_to_move(token_id):
    if not isinstance(token_id, int) or isinstance(token_id, bool) or not _FIRST <= token_id < _LAST:
        raise TokenizerError(f"not a move token id: {token_id!r}")
    return UCI[token_id - _FIRST]


def is_move_id(token_id):
    return isinstance(token_id, int) and not isinstance(token_id, bool) and _FIRST <= token_id < _LAST
