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
