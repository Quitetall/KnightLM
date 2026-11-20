"""Single source of the KnightLM move prompt (training, evaluation and serving)."""

import json

INSTRUCTION = "Choose a legal chess move. Reply with just the UCI move.\n"


def packet_json(packet):
    return json.dumps(packet, sort_keys=True, separators=(",", ":"))


def user_text(packet):
    return INSTRUCTION + packet_json(packet)


def user_chat(packet):
    """One user turn, in the shape the SmolVLM chat template expects."""
    return [{"role": "user", "content": [{"type": "text", "text": user_text(packet)}]}]


# --- Modes and tools (docs/model/modes-and-tools.md) -------------------------
# Additive: user_text and user_chat above are the move prompt that corpora,
# evaluations and the served model were built on, and stay byte-identical.
# The tool schemas are defined once, in Rust (crates/ai/src/tools.rs), and read
# here from the checked-in JSON that Rust generates and tests.

from pathlib import Path

TOOLS_PATH = Path(__file__).resolve().parent.parent / "crates" / "ai" / "schemas" / "model_tools.json"

MODES = ("play", "coach")
MAX_SUMMARY_CHARS = 1200
MAX_RECENT_MESSAGES = 6
MAX_MESSAGE_CHARS = 800

PLAY_INSTRUCTION = (
    "You are playing chess in competition mode. Write one short reasoning field, "
    "then call submit_move with a legal UCI move. Use only the tools listed; "
    "reply with a single tool call.\n"
)
COACH_INSTRUCTION = (
    "You are a chess coach playing a game. Your first line must be "
    "`Move: <legal UCI move>`; then reply to the user. Use only the tools listed.\n"
)
_INSTRUCTIONS = {"play": PLAY_INSTRUCTION, "coach": COACH_INSTRUCTION}
TOOL_CALL_FORMAT = (
    '<tool_call>{"name": "<tool>", "arguments": {...}}</tool_call>\n'
)


def load_tool_schemas(path=None):
    """The shared tool JSON: {"schema_version": str, "modes": {mode: [tool]}}."""
    return json.loads(Path(path or TOOLS_PATH).read_text(encoding="utf-8"))


def tool_names(mode, schemas=None):
    return [t["name"] for t in (schemas or load_tool_schemas())["modes"][mode]]


def _check_mode(mode):
    if mode not in MODES:
        raise ValueError(f"unknown mode {mode!r}; expected one of {MODES}")


def prefix(mode, schemas=None):
    """The fixed part of a prompt: instruction, call format, this mode's schema.

    Identical for every turn of a mode, so a serving cache can reuse it. It never
    mentions the other mode's tools.
    """
    _check_mode(mode)
    tools = (schemas or load_tool_schemas())["modes"][mode]
    return (
        _INSTRUCTIONS[mode]
        + TOOL_CALL_FORMAT
        + "Tools:\n"
        + json.dumps(tools, sort_keys=True, separators=(",", ":"))
        + "\n"
    )


def chat_context(summary=None, recent=()):
    """Bounded rolling chat context: a summary and/or the last few messages.

    Overflow of the summary or of one message is an error; older messages beyond
    MAX_RECENT_MESSAGES are dropped (that is the rolling rule).
    """
    parts = []
    if summary:
        if len(summary) > MAX_SUMMARY_CHARS:
            raise ValueError("chat summary exceeds MAX_SUMMARY_CHARS")
        parts.append("Conversation summary: " + summary + "\n")
    for message in list(recent)[-MAX_RECENT_MESSAGES:]:
        role, text = message["role"], message["text"]
        if role not in ("user", "assistant"):
            raise ValueError(f"unknown chat role {role!r}")
        if len(text) > MAX_MESSAGE_CHARS:
            raise ValueError("chat message exceeds MAX_MESSAGE_CHARS")
        parts.append(f"{role}: {text}\n")
    return "".join(parts)


def play_text(packet, schemas=None):
    """Play mode: instruction, play schema, packet. Nothing else; packet last."""
    return prefix("play", schemas) + "Position packet:\n" + packet_json(packet)


def coach_text(packet, summary=None, recent=(), schemas=None):
    """Coach mode: fixed prefix, bounded chat context, then the packet last."""
    return (
        prefix("coach", schemas)
        + chat_context(summary, recent)
        + "Position packet:\n"
        + packet_json(packet)
    )


def mode_chat(text):
    """Wrap a mode prompt as one user turn, like user_chat."""
    return [{"role": "user", "content": [{"type": "text", "text": text}]}]
