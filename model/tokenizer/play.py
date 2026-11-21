"""The play-call frame: reasoning in host text tokens, the move as one ctok token.

    <call:submit_move> <reasoning> {host ids} </reasoning> <arg:move> <m:uci> </call>

This is the token form of the completion `parse_play_completion` reads
(`<tool_call>{"name":"submit_move","arguments":{"reasoning":..,"move":..}}`):
same key order, reasoning first, one call and nothing else. The reasoning ids
are the host tokenizer's and pass through untouched; the frame never looks at them.
"""

from model import prompt
from model.tokenizer import vocab
from model.tokenizer.errors import TokenizerError
from model.tokenizer.moves import id_to_move, move_to_id

ID = vocab.ID


def build_call(reasoning_host_ids, uci, base):
    """Ids of one call. ctok ids are shifted by `base` (the host vocabulary size); the
    reasoning ids are the host's and must all lie below `base`."""
    reasoning = list(reasoning_host_ids)
    if not reasoning:
        raise TokenizerError("reasoning is required and must not be empty")
    if any(not isinstance(t, int) or not 0 <= t < base for t in reasoning):
        raise TokenizerError("reasoning ids must be host ids below the ctok base")
    frame = [ID["<call:submit_move>"], ID["<reasoning>"]], [ID["</reasoning>"], ID["<arg:move>"], move_to_id(uci), ID["</call>"]]
    return [t + base for t in frame[0]] + reasoning + [t + base for t in frame[1]]


def split_call(ids, base):
    """Inverse of build_call: (reasoning_host_ids, uci). Refuses anything but one canonical call."""
    ids = list(ids)

    def local(t):
        return t - base if isinstance(t, int) and base <= t < base + vocab.SIZE else None

    if len(ids) < 6 or local(ids[0]) != ID["<call:submit_move>"] or local(ids[1]) != ID["<reasoning>"]:
        raise TokenizerError("not a submit_move call")
    try:
        close = next(i for i in range(2, len(ids)) if local(ids[i]) == ID["</reasoning>"])
    except StopIteration:
        raise TokenizerError("reasoning is not closed") from None
    reasoning = ids[2:close]
    if not reasoning or any(local(t) is not None for t in reasoning):
        raise TokenizerError("reasoning must be non-empty host text with no ctok tokens")
    tail = [local(t) for t in ids[close + 1:]]
    if len(tail) != 3 or tail[0] != ID["<arg:move>"] or tail[2] != ID["</call>"] or tail[1] is None:
        raise TokenizerError("a call ends with <arg:move> <move> </call>")
    return reasoning, id_to_move(tail[1])


def completion_text(reasoning, uci):
    """The text completion this frame stands for (model/prompt.py is its one source)."""
    return prompt.format_play_completion(reasoning, uci)
