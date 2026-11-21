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

TOOLS_PATH = Path(__file__).resolve().parent.parent / "crates" / "contract" / "schemas" / "model_tools.json"

MODES = ("play", "coach")
MAX_SUMMARY_CHARS = 1200
MAX_RECENT_MESSAGES = 6
MAX_MESSAGE_CHARS = 800

PLAY_INSTRUCTION = 'You are playing chess in competition mode. Reply with exactly one tool call: call submit_move with a short reasoning field first, then a legal UCI move. Use only the tools listed.\n'
COACH_INSTRUCTION = 'You are a chess coach playing a game. Your first line must be `Provisional: <legal UCI move>`. You may think aloud and later write another `Provisional: <legal UCI move>` line to change it; the last legal one is your move when you stop. Then reply to the user. Use only the tools listed.\n'
COMBINED_INSTRUCTION = "You are a chess coach that also plays competitively. The line `Mode: play` or `Mode: coach` after the tool lists names this turn's mode; call only that mode's tools. In play mode reply with exactly one submit_move call, reasoning field first, then a legal UCI move. In coach mode your first line must be `Provisional: <legal UCI move>`, which a later `Provisional:` line may revise (the last legal one is your move), then reply to the user.\n"
_INSTRUCTIONS = {"play": PLAY_INSTRUCTION, "coach": COACH_INSTRUCTION}
TOOL_CALL_FORMAT = (
    '<tool_call>{"name": "<tool>", "arguments": {...}}</tool_call>\n'
)
MAX_REASONING_CHARS = 400
EXCERPT_CHARS = 120


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


# --- Combined prefix for a long chat -------------------------------------------
# Both schemas in one fixed prefix; each call is still checked against the schema
# of the mode named on the `Mode:` line (docs/model/modes-and-tools.md section 3).


def _blob(mode, schemas):
    return json.dumps(schemas["modes"][mode], sort_keys=True, separators=(",", ":"))


def combined_prefix(schemas=None):
    schemas = schemas or load_tool_schemas()
    return (
        COMBINED_INSTRUCTION
        + TOOL_CALL_FORMAT
        + "Play tools:\n"
        + _blob("play", schemas)
        + "\nCoach tools:\n"
        + _blob("coach", schemas)
        + "\n"
    )


def combined_text(packet, mode, summary=None, recent=(), schemas=None):
    """Long chat: both schemas, then this turn's mode, chat context, packet last."""
    _check_mode(mode)
    return (
        combined_prefix(schemas)
        + f"Mode: {mode}\n"
        + chat_context(summary, recent)
        + "Position packet:\n"
        + packet_json(packet)
    )


# --- Play-mode completion format -------------------------------------------------
# The exact completion a play row is trained to write: one submit_move call whose
# arguments are `reasoning` then `move`, nothing outside the block. The Rust parser
# (crates/ai/src/model_turn.rs) and this one are pinned by
# crates/contract/schemas/play_completion_cases.json.

import re

TOOL_CALL_OPEN = "<tool_call>"
TOOL_CALL_CLOSE = "</tool_call>"
_UCI = re.compile(r"[a-h][1-8][a-h][1-8][qrbn]?")


class PlayCompletionError(ValueError):
    """`code` is the spelling shared with Rust's PlayCompletionError::code."""

    def __init__(self, code):
        super().__init__(code)
        self.code = code


class _Obj(list):
    """A JSON object as ordered (key, value) pairs, duplicates kept."""


def _no_constant(name):
    raise ValueError(name)


def format_play_completion(reasoning, move):
    return (
        TOOL_CALL_OPEN
        + '{"name":"submit_move","arguments":{"reasoning":'
        + json.dumps(reasoning, ensure_ascii=False)
        + ',"move":'
        + json.dumps(move, ensure_ascii=False)
        + "}}"
        + TOOL_CALL_CLOSE
    )


def parse_play_completion(text):
    """Return {"reasoning", "move"} or raise PlayCompletionError. Legality is not checked."""
    err = PlayCompletionError
    if text.count(TOOL_CALL_OPEN) != 1 or text.count(TOOL_CALL_CLOSE) != 1:
        raise err("not_one_call")
    start, end = text.index(TOOL_CALL_OPEN), text.index(TOOL_CALL_CLOSE)
    if end < start:
        raise err("not_one_call")
    if text[:start].strip() or text[end + len(TOOL_CALL_CLOSE):].strip():
        raise err("extra_text")
    try:
        top = json.loads(
            text[start + len(TOOL_CALL_OPEN):end].strip(),
            object_pairs_hook=_Obj,
            parse_constant=_no_constant,
        )
    except ValueError:
        raise err("malformed") from None
    if not isinstance(top, _Obj):
        raise err("malformed")
    keys = [k for k, _ in top]
    fields = dict(top)
    if sorted(keys) != ["arguments", "name"] or not isinstance(fields["name"], str):
        raise err("malformed")
    args = fields["arguments"]
    if not isinstance(args, _Obj):
        raise err("malformed")
    if fields["name"] != "submit_move":
        raise err("wrong_tool")
    names = [k for k, _ in args]
    if names != ["reasoning", "move"]:
        if names == ["move", "reasoning"]:
            raise err("key_order")
        raise err("reasoning_missing" if names == ["move"] else "malformed")
    reasoning, move = args[0][1], args[1][1]
    if not isinstance(reasoning, str) or not reasoning.strip():
        raise err("reasoning_missing")
    if len(reasoning) > MAX_REASONING_CHARS:
        raise err("reasoning_too_long")
    if not isinstance(move, str) or not _UCI.fullmatch(move):
        raise err("bad_move")
    return {"reasoning": reasoning, "move": move}


# --- Deterministic chat summarizer -------------------------------------------------
# No model call. Mirrors summarize_chat / roll_chat in model_turn.rs, pinned by
# crates/contract/schemas/chat_summary_cases.json.


def _excerpt(text):
    flat = " ".join(text.split())
    return flat if len(flat) <= EXCERPT_CHARS else flat[: EXCERPT_CHARS - 1] + "\u2026"


def summarize_chat(previous, dropped):
    """Fold dropped messages into the rolling summary, bounded by MAX_SUMMARY_CHARS."""
    parts = [previous] if previous else []
    for message in dropped:
        tag = "U" if message["role"] == "user" else "A"
        parts.append(f"{tag}: {_excerpt(message['text'])}")
    joined = " | ".join(parts)
    if len(joined) <= MAX_SUMMARY_CHARS:
        return joined
    tail = joined[-(MAX_SUMMARY_CHARS - 1):]
    cut = tail.find(" | ")
    if cut >= 0:
        tail = tail[cut + 3:]
    return "\u2026" + tail


def roll_chat(summary, messages):
    """Keep the last MAX_RECENT_MESSAGES messages; fold older ones into the summary."""
    messages = list(messages)
    skip = max(0, len(messages) - MAX_RECENT_MESSAGES)
    if skip == 0:
        return summary, messages
    return summarize_chat(summary, messages[:skip]), messages[skip:]


# --- Play prompt with Chess Graph Text (arms T0 / T2) ---------------------------------
# `play` is the FEN packet prompt above (arm T0, the control). `play-cgt` and `play-cgt-full`
# are the same prompt with the position rendered as cgt/1 text (docs/model/graph-text.md)
# instead of the packet JSON: same instruction, same play schema, same completion format, and the
# position last. The text comes from the Rust compiler (`knightlm cgt`), never from Python.

import functools
import os
import subprocess

# `play-t1`, `play-t3`, `play-t3-full` are arms T1 and T3: the position as ctok/1 token strings
# (model/tokenizer/arms.py). They need the extended tokenizer (model/tokenizer/hf.py); the completion
# is the same host-text play call, so parse_play_completion reads their replies unchanged.
CTOK_KINDS = ("play-t1", "play-t3", "play-t3-full")
PLAY_KINDS = ("play", "play-cgt", "play-cgt-full", *CTOK_KINDS)
CGT_VARIANT = {"play-cgt": "lite", "play-cgt-full": "full"}
CGT_LABEL = "Position graph:\n"
_REPO = Path(__file__).resolve().parent.parent


def knightlm_binary(explicit=None):
    """The compiler binary: explicit path, KNIGHTLM_BIN, then target/release and target/debug."""
    candidates = [explicit, os.environ.get("KNIGHTLM_BIN"), _REPO / "target/release/knightlm", _REPO / "target/debug/knightlm"]
    for candidate in candidates:
        if candidate and Path(candidate).is_file():
            return Path(candidate).resolve()
    raise FileNotFoundError("knightlm binary not found: build it (cargo build --release -p knightlm-cli) or set KNIGHTLM_BIN")


@functools.lru_cache(maxsize=65536)
def _cgt(binary, fen, rep, variant):
    cmd = [binary, "cgt", "--fen", fen, "--variant", variant]
    if rep is not None:
        cmd += ["--rep", str(rep)]
    return subprocess.run(cmd, check=True, capture_output=True, text=True, timeout=30).stdout.rstrip("\n")


def cgt_text(packet, variant, binary=None):
    """cgt/1 text for a packet's position. The repetition count is the packet's own (`?` if absent)."""
    if variant not in ("lite", "full"):
        raise ValueError(f"unknown cgt variant {variant!r}")
    rep = packet["facts"].get("repetition_count")
    return _cgt(str(knightlm_binary(binary)), packet["fen"], rep, variant)


def check_play_kind(kind):
    if kind not in PLAY_KINDS:
        raise ValueError(f"unknown play prompt kind {kind!r}; expected one of {PLAY_KINDS}")


def play_prompt(kind, packet, schemas=None, binary=None):
    """The play prompt for a kind: `play` is play_text; `play-cgt*` swap the packet for CGT text;
    `play-t1` and `play-t3*` swap it for ctok/1 token strings."""
    check_play_kind(kind)
    if kind == "play":
        return play_text(packet, schemas)
    if kind in CTOK_KINDS:
        from model.tokenizer import arms

        text = cgt_text(packet, arms.CGT_VARIANT[kind], binary) if kind in arms.CGT_VARIANT else None
        return prefix("play", schemas) + arms.LABEL + arms.position_text(kind, packet, text)
    return prefix("play", schemas) + CGT_LABEL + cgt_text(packet, CGT_VARIANT[kind], binary)


# --- S0 rules-QA prompt (docs/model/s0 corpus: model/corpus/s0.py) -----------------------
# A question about the position, appended after it. The completion is the play call whose
# `reasoning` field is the answer.

QA_LABEL = "\nQuestion: "


def qa_prompt(kind, packet, question, schemas=None, binary=None):
    if "\n" in question:
        raise ValueError("a question is one line")
    return play_prompt(kind, packet, schemas, binary) + QA_LABEL + question
