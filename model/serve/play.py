"""Play-mode completion checks, engine-free and torch-free.

A play-mode reply is one `submit_move` tool call (model/prompt.py owns the format and
its parser). `interpret` adds the two checks the parser does not make, because they
need the position or the reasoning text:

  illegal_move           the parsed move is not in the packet's legal_moves
  stated_move_mismatch   the reasoning says "I choose X" / "The move is X" / "My move is X"
                         with a UCI X different from the move field

Every refusal has a stable `code`; there is no repair and no fallback move. The codes
below the parser's are spelled as `model.prompt.PlayCompletionError.code`.
"""

from dataclasses import dataclass
import json
import re

from model.prompt import (MAX_REASONING_CHARS, PlayCompletionError, format_play_completion,
                          parse_play_completion)

PARSE_CODES = ("not_one_call", "extra_text", "malformed", "wrong_tool", "key_order",
               "reasoning_missing", "reasoning_too_long", "bad_move")
CHECK_CODES = ("illegal_move", "stated_move_mismatch")
# Constrained decoding found no usable reasoning to condition the move on.
NO_PREFIX = "constrained_no_reasoning"
PLAY_CODES = ("ok",) + PARSE_CODES + CHECK_CODES + (NO_PREFIX,)

PATH_MODEL = "model"                    # the model's own completion passed every check
PATH_CONSTRAINED = "constrained_move"   # the move field was chosen from the legal list
PATH_NONE = "none"                      # no move is returned
PATH_FORCED = "constrained_forced_reasoning"  # the call head was supplied, the model wrote the reasoning
PATHS = (PATH_MODEL, PATH_CONSTRAINED, PATH_FORCED, PATH_NONE)

STATED = re.compile(r"\b(?:my move is|i choose|the move is|i play)\s+([a-h][1-8][a-h][1-8][qrbn]?)\b", re.I)
_MARK = "\u0000move\u0000"


@dataclass
class PlayResult:
    code: str
    move: str | None = None
    reasoning: str | None = None
    stated_move: str | None = None


def stated_move(reasoning):
    found = STATED.findall(reasoning or "")
    return found[-1].lower() if found else None


def interpret(text, legal_moves):
    """Check one raw play completion. `move` is kept when it parsed, even if refused."""
    try:
        parsed = parse_play_completion(text)
    except PlayCompletionError as error:
        return PlayResult(error.code)
    move, reasoning = parsed["move"], parsed["reasoning"]
    stated = stated_move(reasoning)
    if move not in legal_moves:
        return PlayResult("illegal_move", move, reasoning, stated)
    if stated is not None and stated != move:
        return PlayResult("stated_move_mismatch", move, reasoning, stated)
    return PlayResult("ok", move, reasoning, stated)


def call_head():
    """The canonical start of a play completion, through the opening quote of the reasoning string."""
    head = format_play_completion("", _MARK)
    return head[:head.index('"reasoning":') + len('"reasoning":')] + '"'


def prefix_for(reasoning):
    """(prefix, suffix) around the move value of the canonical completion for `reasoning`.

    `prefix` runs through the opening quote of the move value, `suffix` starts at its closing
    quote. None when the reasoning is not a usable one (blank, over-long, not a string)."""
    if not isinstance(reasoning, str) or not reasoning.strip() or len(reasoning) > MAX_REASONING_CHARS:
        return None
    canonical = format_play_completion(reasoning, _MARK)
    at = canonical.index(json.dumps(_MARK))
    return canonical[:at + 1], canonical[at + len(json.dumps(_MARK)) - 1:]


def reasoning_from_forced(text):
    """The reasoning string a model wrote after the supplied call head: text up to its first
    unescaped quote, JSON-unescaped. None if it never closed the string or the escapes are invalid."""
    i = 0
    while i < len(text):
        if text[i] == "\\":
            i += 2
            continue
        if text[i] == '"':
            try:
                return json.loads('"' + text[:i] + '"')
            except ValueError:
                return None
        i += 1
    return None


def move_prefix(text):
    """Return (reasoning, prefix, suffix) when `text` opens with a well-formed play call.

    None when the reasoning cannot be recovered (no call, wrong tool, empty or over-long
    reasoning, move before reasoning)."""
    head = call_head()[:-1]
    if not text.startswith(head):
        return None
    cut = text.find(',"move":', len(head))
    if cut < 0:
        return None
    try:
        reasoning = json.loads(text[len(head):cut])
    except ValueError:
        return None
    around = prefix_for(reasoning)
    return None if around is None else (reasoning, *around)


def resolve(text, legal_moves, mode, constrain, force=None):
    """Decide what a play-mode reply yields. Returns (fields, extra_tokens).

    raw: the model's own completion, checked, never repaired. constrained: the same completion
    when it already passes every check (path "model"); otherwise, when the reasoning is
    recoverable, `constrain(prefix, suffix) -> (move, new_tokens)` chooses the move field from the
    legal list after that reasoning (path "constrained_move", code "ok", `raw_code` keeps the
    refusal). When it is not, and `force() -> (reasoning, new_tokens)` is given, the call head is
    supplied and the model writes the reasoning (path "constrained_forced_reasoning"); a force that
    yields no usable reasoning, or no `force`, gives no move (code constrained_no_reasoning). A
    constrained answer is never an illegal move: a chosen move outside `legal_moves` is an error.
    """
    result = interpret(text, legal_moves)
    fields = dict(move=result.move, reasoning=result.reasoning, legal=result.code == "ok", code=result.code,
                  path=PATH_MODEL if result.code == "ok" else PATH_NONE, raw_code=result.code,
                  stated_move=result.stated_move)
    if mode != "constrained" or result.code == "ok":
        return fields, 0
    found, path, spent = move_prefix(text), PATH_CONSTRAINED, 0
    if found is None and force is not None:
        reasoning, spent = force()
        around = prefix_for(reasoning)
        found = None if around is None else (reasoning, *around)
        path = PATH_FORCED
    if found is None:
        return dict(fields, move=None, reasoning=None, legal=False, code=NO_PREFIX), spent
    reasoning, prefix, suffix = found
    move, extra = constrain(prefix, suffix)
    if move not in legal_moves:
        raise ValueError(f"constrained decoding produced {move!r}, which is not a legal move")
    return dict(fields, move=move, reasoning=reasoning, legal=True, code="ok", path=path,
                stated_move=stated_move(reasoning)), spent + extra
