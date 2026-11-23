"""Agreement between the head's move and the language head's completion (torch-free, engine-free).

Inference order: the head picks the move, the LLM writes reasoning conditioned on it, and this
check feeds the existing play_completion codes. A completion whose move field or stated move
differs from the head's move is `stated_move_mismatch`; there is no repair and no fallback.
"""

from model.serve.play import PlayResult, interpret, stated_move


def interpret_with_head(text, legal_moves, head_move):
    if head_move not in legal_moves:
        raise ValueError("the head's move must be legal: it is chosen from the masked distribution")
    result = interpret(text, legal_moves)
    if result.code != "ok":
        return result
    if result.move != head_move:
        return PlayResult("stated_move_mismatch", result.move, result.reasoning, stated_move(result.reasoning))
    return result
