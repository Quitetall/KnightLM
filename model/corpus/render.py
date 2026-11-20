"""Render a reasoning-corpus row as a training pair. The only place the format lives.

Prompt: `model.prompt.play_text(packet)` (instruction, play schema, packet last).
Completion: `model.prompt.format_play_completion(reasoning, move)`, exactly one
`submit_move` call with `reasoning` then `move` (docs/model/modes-and-tools.md).
Both come from model/prompt.py, which owns the contract and its parser. The chat
template ends the assistant turn with its end-of-turn token and the trainer masks
loss on the prompt.
"""

from model import prompt as prompt_module


def render_completion(reasoning, move):
    return prompt_module.format_play_completion(reasoning, move)


def render_play(packet, reasoning, move):
    return {"prompt": prompt_module.play_text(packet), "completion": render_completion(reasoning, move)}
