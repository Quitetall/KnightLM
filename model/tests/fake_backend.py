"""A stand-in for the BLUT image-text trainer's data module (`hf_image_text_data`), for CI.

The joint trainers (`model.policy_head.joint`, `model.encoder.task`) take the data module as an argument and call
`count_rows`, `RowStream` and `encode`. The real module lives in the pinned blut-cookbooks checkout, which CI does
not have, so the tests fall back to this one, which restates the same contract in a few lines: a stream that is a
pure function of (shards, seed, buffer, epochs) through a seeded shuffle buffer, and the chat-templated prompt ids
plus the completion ids cut after the first end-of-turn token. Where the real module is present the tests use it
(`backend()` in the test modules), and `parity` below is what proves this one equal to it.
"""

import random


def count_rows(shards):
    n = 0
    for shard in shards:
        with open(shard, "rb") as stream:
            n += sum(1 for line in stream if line.strip())
    return n


def _iter_epoch(shards, seed, epoch, buffer):
    rng = random.Random(seed * 1_000_003 + epoch)
    buf = []
    for shard in shards:
        with open(shard, "r", encoding="utf-8") as stream:
            for line in stream:
                if not line.strip():
                    continue
                if buffer <= 1:
                    yield line
                elif len(buf) < buffer:
                    buf.append(line)
                else:
                    i = rng.randrange(buffer)
                    out, buf[i] = buf[i], line
                    yield out
    rng.shuffle(buf)
    yield from buf


class RowStream:
    def __init__(self, shards, seed, buffer, epochs, skip=0):
        self.consumed = 0
        self._it = self._gen(list(shards), seed, buffer, epochs)
        for _ in range(skip):
            if next(self._it, None) is None:
                break
            self.consumed += 1

    @staticmethod
    def _gen(shards, seed, buffer, epochs):
        for epoch in range(epochs):
            yield from _iter_epoch(shards, seed, epoch, buffer)

    def next_line(self):
        line = next(self._it, None)
        if line is not None:
            self.consumed += 1
        return line


def _end_ids(tokenizer):
    ids = [tokenizer.eos_token_id] if tokenizer.eos_token_id is not None else []
    for token in ("<end_of_utterance>", "<|im_end|>"):
        tid = tokenizer.convert_tokens_to_ids(token)
        if isinstance(tid, int) and tid >= 0 and tid not in ids:
            ids.append(tid)
    return ids


def encode(prompt, completion, processor):
    tokenizer = processor.tokenizer
    templater = processor if getattr(processor, "chat_template", None) else tokenizer
    chat = [{"role": "user", "content": [{"type": "text", "text": prompt}]}]
    prompt_text = templater.apply_chat_template(chat, tokenize=False, add_generation_prompt=True)
    chat.append({"role": "assistant", "content": [{"type": "text", "text": completion}]})
    answer_text = templater.apply_chat_template(chat, tokenize=False)
    prompt_ids = tokenizer.encode(prompt_text, add_special_tokens=False)
    ids = tokenizer.encode(answer_text, add_special_tokens=False)
    if ids[:len(prompt_ids)] != prompt_ids:
        raise ValueError("prompt and completion tokenize to different prefixes")
    ends = _end_ids(tokenizer)
    cut = [i for i in range(len(prompt_ids), len(ids)) if ids[i] in ends]
    return prompt_ids, ids[:cut[0] + 1 if cut else len(ids)]
