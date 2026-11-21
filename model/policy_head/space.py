"""The head's output space is the ctok/1 move set, reused from model.tokenizer (never re-derived)."""

from model.tokenizer import moves, vocab

UCI = tuple(moves.UCI)
SIZE = len(UCI)  # 1968
INDEX = {uci: i for i, uci in enumerate(UCI)}
assert SIZE == vocab.RANGES["move"][1] - vocab.RANGES["move"][0]


class MoveSpaceError(ValueError):
    pass


def indices(legal_moves):
    """Class indices of a packet's legal_moves list. An empty list or an unknown move is refused:
    a terminal position has nothing to choose and a move outside ctok/1 means the spec is stale."""
    if not legal_moves:
        raise MoveSpaceError("no legal moves: terminal positions are not head rows")
    try:
        return sorted({INDEX[m] for m in legal_moves})
    except KeyError as error:
        raise MoveSpaceError(f"legal move {error.args[0]!r} is not in the ctok/1 move space") from None


def legal_mask_row(legal_moves):
    """Plain-list 0/1 mask of length SIZE from the packet's legal_moves."""
    row = [False] * SIZE
    for i in indices(legal_moves):
        row[i] = True
    return row


def target_index(target, legal_moves):
    if target not in legal_moves:
        raise MoveSpaceError(f"target {target!r} is not legal in its own packet")
    return INDEX[target]
