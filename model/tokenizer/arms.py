"""Arms T1 and T3: the position of a play prompt as ctok/1 tokens.

The prompt of an arm is the play prompt of arm T0 with the position block replaced:

    {instruction, call format, play schema}  "Position tokens:\\n"  {ctok tokens}

The ctok tokens are written as their token strings (`<pos><side:w>...</pos>`) back to back. After
`hf.extend_tokenizer` every one of them is a single added token, so the host tokenizer turns that text
into exactly `base + local id` for each token (`tokens_to_ids` is the inverse and the parity test pins
the equality). Train and serve therefore build the same prompt text with the same function and tokenise
it with the same extended tokenizer. The completion is unchanged: the host-text `submit_move` call
(arms T1 and T3 change the input only, docs/model/tokenizer.md), so model/prompt.py's parser and every
eval axis apply as they are.

    play-t1       T1 board: <pos> .. <board> 64 pieces <moves> legal moves </moves> </pos>
    play-t3       T3 cgt-lite ids
    play-t3-full  T3 cgt-full ids

T3 text comes from the compiler (`knightlm cgt`), never from Python; `cgt_text` is passed in.
"""

from model.tokenizer import cgt, position, vocab
from model.tokenizer.errors import TokenizerError

KINDS = ("play-t1", "play-t3", "play-t3-full")
CGT_VARIANT = {"play-t3": "lite", "play-t3-full": "full"}
LABEL = "Position tokens:\n"


def position_ids(kind, packet, cgt_text=None):
    """Local ctok ids of the position block for an arm kind."""
    if kind == "play-t1":
        return position.encode(position.packet_to_record(packet))
    if kind in CGT_VARIANT:
        if cgt_text is None:
            raise TokenizerError(f"{kind} needs the compiler's cgt text")
        return cgt.encode(cgt_text)
    raise TokenizerError(f"unknown ctok arm {kind!r}; expected one of {KINDS}")


def ids_to_text(ids):
    """Token strings back to back: what the extended host tokenizer reads as those ids."""
    return "".join(vocab.TOKENS[i] for i in ids)


def tokens_to_ids(text):
    """Inverse of ids_to_text for a string made only of ctok tokens (refuses anything else)."""
    ids, i = [], 0
    while i < len(text):
        if text[i] != "<":
            raise TokenizerError(f"not a ctok token string at offset {i}")
        j = text.find(">", i)
        token = text[i:j + 1] if j >= 0 else ""
        if token not in vocab.ID:
            raise TokenizerError(f"not a ctok token at offset {i}: {text[i:i + 24]!r}")
        ids.append(vocab.ID[token])
        i = j + 1
    return ids


def position_text(kind, packet, cgt_text=None):
    return ids_to_text(position_ids(kind, packet, cgt_text))
