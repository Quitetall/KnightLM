"""Coach mode (chat on), torch-free: the `Provisional:` commit rule, coach tool calls and the chat turn.

The contract is docs/model/modes-and-tools.md section 3. The model plays first, then replies:
it writes `Provisional: <uci>` lines, the last legal one is the move, and if none is legal the
constrained path chooses (a fallback, labelled, with no reply shown). The tracker here mirrors
`ProvisionalTracker` in crates/ai/src/model_turn.rs; crates/contract/schemas/provisional_cases.json
holds the cases both must answer identically.

Coach tool calls are not run here: the app owns the board, so they are validated against the coach
schema (model/prompt.py reads it from crates/contract/schemas/model_tools.json) and forwarded as
OpenAI `tool_calls` for the Rust executors (crates/ai/src/coach.rs). A coach tool can never submit
the real move; `submit_move` and every app tool are refused.
"""

from dataclasses import dataclass, field
import json

from model.prompt import (MAX_MESSAGE_CHARS, MAX_SUMMARY_CHARS, TOOL_CALL_CLOSE, TOOL_CALL_OPEN, combined_text,
                          load_tool_schemas, roll_chat)

PROVISIONAL = "Provisional:"
GATE_ERROR = "`{shown}` is not a legal move in this position. Call list_legal_moves or submit another UCI move."


def provisional_token(line):
    """The move named by a `Provisional: <token>` line, or None if `line` is not one."""
    if not line.startswith(PROVISIONAL):
        return None
    parts = line[len(PROVISIONAL):].split()
    return parts[0] if len(parts) == 1 else None


def _lines(text):
    """Rust's str::lines: split on newlines, drop one `\\r` before each `\\n`."""
    parts = text.split("\n")
    if parts[-1] == "":
        parts.pop()
    last = len(parts) - 1
    return [p[:-1] if (i < last or text.endswith("\n")) and p.endswith("\r") else p
            for i, p in enumerate(parts)]


def strip_provisional_lines(text):
    """Coach text with the `Provisional:` lines removed: the reply a person sees."""
    return "\n".join(line for line in _lines(text) if provisional_token(line) is None).strip()


class ProvisionalTracker:
    """The coach commit rule. Every provisional line goes through one attempt budget; the last
    legal line is the committed move; an illegal later line never erases a legal earlier one."""

    def __init__(self, legal, max_attempts):
        self.legal = list(legal)
        self.max_attempts = max(1, max_attempts)
        self.attempts = self.accepted = self.rejected = 0
        self.committed = None
        self.moves = []  # every legal provisional move read, in order
        self.exhausted = False
        self.feedback = []

    def feed(self, text, finished=True):
        """Read the provisional lines of `text`. `finished` is false for a stream that stopped
        early: the unterminated last line may be cut mid-move and is ignored. Returns the errors
        to show the model if another round follows."""
        self.feedback = []
        lines = text.split("\n")
        if not finished:
            lines.pop()
        for line in lines:
            token = provisional_token(line.rstrip("\r"))
            if token is None:
                continue
            if self.exhausted:
                break
            self._submit(token)
        return list(self.feedback)

    def _submit(self, token):
        if self.attempts >= self.max_attempts:
            self.rejected += 1
            self.exhausted = True
            return
        self.attempts += 1
        if token in self.legal:
            self.accepted += 1
            self.committed = token
            self.moves.append(token)
        elif self.attempts >= self.max_attempts:
            self.rejected += 1
            self.exhausted = True
        else:
            self.rejected += 1
            self.feedback.append(GATE_ERROR.format(shown=token[:8]))


@dataclass
class ProvisionalResult:
    committed: str | None  # the last legal provisional move
    fallback: bool         # none was legal: the constrained path must choose, and no reply is shown
    attempts: int
    accepted: int
    rejected: int
    exhausted: bool
    feedback: list = field(default_factory=list)
    reply: str | None = None  # the text without `Provisional:` lines; None after a fallback
    moves: list = field(default_factory=list)


def parse_provisional(text, legal, max_attempts=3, finished=True):
    """One coach completion's commit outcome, mirroring `ProvisionalTracker` byte for byte."""
    tracker = ProvisionalTracker(legal, max_attempts)
    feedback = tracker.feed(text, finished)
    return ProvisionalResult(
        committed=tracker.committed, fallback=tracker.committed is None, attempts=tracker.attempts,
        accepted=tracker.accepted, rejected=tracker.rejected, exhausted=tracker.exhausted, feedback=feedback,
        reply=None if tracker.committed is None else strip_provisional_lines(text), moves=list(tracker.moves))


# --- coach tool calls ---------------------------------------------------------------------


def extract_tool_blocks(text):
    """Every `<tool_call>` block's inner text, in order; an unclosed last block is `None`."""
    out, rest = [], text
    while True:
        start = rest.find(TOOL_CALL_OPEN)
        if start < 0:
            return out
        after = rest[start + len(TOOL_CALL_OPEN):]
        end = after.find(TOOL_CALL_CLOSE)
        if end < 0:
            out.append(None)
            return out
        out.append(after[:end])
        rest = after[end + len(TOOL_CALL_CLOSE):]


def strip_tool_calls(text):
    out, rest = [], text
    while True:
        start = rest.find(TOOL_CALL_OPEN)
        if start < 0:
            return "".join(out) + rest
        out.append(rest[:start])
        end = rest.find(TOOL_CALL_CLOSE, start)
        if end < 0:
            return "".join(out)
        rest = rest[end + len(TOOL_CALL_CLOSE):]


def _type_ok(kind, value):
    if kind == "string":
        return isinstance(value, str)
    if kind == "integer":
        return isinstance(value, int) and not isinstance(value, bool)
    if kind == "array":
        return isinstance(value, list)
    if kind == "object":
        return isinstance(value, dict)
    return False


def _valid(value, schema):
    """The subset of JSON Schema the tool schemas use. Returns an error string or None."""
    if not _type_ok(schema.get("type"), value):
        return f"expected {schema.get('type')}"
    if "enum" in schema and value not in schema["enum"]:
        return "not one of the allowed values"
    if isinstance(value, str) and not schema.get("minLength", 0) <= len(value) <= schema.get("maxLength", 10**9):
        return "string length out of bounds"
    if isinstance(value, int) and "minimum" in schema and value < schema["minimum"]:
        return "below the minimum"
    if isinstance(value, list):
        if not schema.get("minItems", 0) <= len(value) <= schema.get("maxItems", 10**9):
            return "wrong number of items"
        for item in value:
            problem = _valid(item, schema.get("items", {"type": "string"}))
            if problem:
                return problem
    if isinstance(value, dict):
        props = schema.get("properties", {})
        for key in value:
            if key not in props:
                return f"unknown argument {key!r}"
        for key in schema.get("required", []):
            if key not in value:
                return f"missing argument {key!r}"
        for key, item in value.items():
            if item is not None:
                problem = _valid(item, props[key])
                if problem:
                    return problem
    return None


def forward_tool_calls(text, schemas=None):
    """Split the coach tool calls of `text` into (accepted, refused).

    accepted: [{"name", "arguments"}] in order, each valid against the coach schema.
    refused: ["<name or ?>: <why>"], for a malformed block, a tool outside the coach set (a play
    tool, an app tool, anything else) or arguments that break the schema."""
    tools = {t["name"]: t["parameters"] for t in (schemas or load_tool_schemas())["modes"]["coach"]}
    accepted, refused = [], []
    for block in extract_tool_blocks(text):
        if block is None:
            refused.append("?: unclosed tool call")
            continue
        try:
            call = json.loads(block.strip())
        except ValueError:
            refused.append("?: malformed")
            continue
        if not isinstance(call, dict) or sorted(call) != ["arguments", "name"] or not isinstance(call["name"], str):
            refused.append("?: malformed")
            continue
        name = call["name"]
        if name not in tools:
            refused.append(f"{name[:32]}: not a coach tool")
            continue
        problem = _valid(call["arguments"], tools[name])
        if problem:
            refused.append(f"{name}: {problem}")
            continue
        accepted.append({"name": name, "arguments": call["arguments"]})
    return accepted, refused


# --- the chat turn ---------------------------------------------------------------------------


class ChatError(ValueError):
    """A request the coach turn cannot build a prompt from; `code` is the API error code."""

    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def chat_history(earlier, last_text, summary=None):
    """(summary, recent) for the prompt. `earlier` is [(role, text)] of the request's messages before
    the packet message, `last_text` what the person typed before the packet in the last one. Rolled to
    the contract bounds (model/prompt.py roll_chat); overflow is an error, never silent truncation."""
    history = [{"role": role, "text": text} for role, text in earlier if role in ("user", "assistant")]
    if last_text.strip():
        history.append({"role": "user", "text": last_text.strip()})
    if summary is not None and len(summary) > MAX_SUMMARY_CHARS:
        raise ChatError("chat_too_long", "chat summary exceeds MAX_SUMMARY_CHARS")
    summary, recent = roll_chat(summary, history)
    for message in recent:
        if len(message["text"]) > MAX_MESSAGE_CHARS:
            raise ChatError("chat_too_long", "chat message exceeds MAX_MESSAGE_CHARS")
    return summary, recent


def coach_prompt(packet, summary, recent):
    """The coach turn's prompt: both schemas, `Mode: coach`, bounded chat, the fresh packet last.
    The legal list is not in the prompt (the model asks the harness); the gate keeps it."""
    shown = {k: v for k, v in packet.items() if k != "legal_moves"}
    return combined_text(shown, "coach", summary, recent)


@dataclass
class CoachTurn:
    """One coach turn's outcome: the move, who chose it, what the person sees, what to run."""
    move: str | None
    provisional: list
    rejected: int
    fallback: bool
    reply: str | None
    tool_calls: list
    refused_calls: list
    raw_reply: str
    input_tokens: int = 0
    output_tokens: int = 0


def play_coach_turn(predictor, packet, summary, recent, mode="constrained", max_attempts=3):
    """Model plays first, then replies. `predictor.predict_coach(prompt_text)` returns an object
    with `text`, `input_tokens`, `output_tokens`. With no legal provisional line, constrained mode
    asks `predictor.predict(packet, "constrained")` for a legal move (a labelled fallback, no reply);
    raw mode returns no move."""
    legal = packet["legal_moves"]
    got = predictor.predict_coach(coach_prompt(packet, summary, recent))
    result = parse_provisional(got.text, legal, max_attempts)
    calls, refused = forward_tool_calls(got.text)
    move, reply = result.committed, result.reply
    extra_in = extra_out = 0
    if result.fallback and mode == "constrained":
        fallback = predictor.predict(packet, "constrained")
        move, extra_in, extra_out = fallback.move, fallback.input_tokens, fallback.output_tokens
    if result.fallback:  # the commentary is withheld, and so are the board effects it asked for
        reply, calls = None, []
    else:
        reply = strip_provisional_lines(strip_tool_calls(got.text))
    return CoachTurn(move=move, provisional=result.moves, rejected=result.rejected, fallback=result.fallback,
                     reply=reply, tool_calls=calls, refused_calls=refused, raw_reply=got.text,
                     input_tokens=got.input_tokens + extra_in, output_tokens=got.output_tokens + extra_out)
