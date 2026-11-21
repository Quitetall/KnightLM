"""Loss masks from a row's `segments` (reasoning v2 variant b), for the joint trainers.

A row may carry `segments`: the completion cut into spans, each `{"text", "loss"}`. The model writes the spans with
`loss: true` (read calls, the final submit_move); the spans with `loss: false` are tool results that the harness
inserts at inference, so no loss may fall on them. `encode` gives the same ids as the BLUT trainer's
`hf_image_text_data.encode` and labels that are -100 over the prompt, the padding and the result spans.

The BLUT recipe path (`hf_sft_train`, `hf_image_text_data.collate`) cannot do this: its rows are
`{"prompt", "completion"}` and every completion token carries loss. `check_recipe_rows` is what
`model.train.run` calls so that a corpus with result spans is refused there instead of trained on the results.
Only the joint trainers (`model.policy_head.joint`, `model.encoder.task`) honour `segments`.
"""

IGNORE = -100


class SegmentError(ValueError):
    """The segments do not describe the completion, or a span boundary does not fall on a token boundary."""


def completion_of(segments):
    return "".join(s["text"] for s in segments)


def masked_spans(segments):
    """True when some span carries no loss (a tool result)."""
    return any(not s["loss"] for s in segments)


def check_recipe_rows(rows):
    """Rows whose result spans the BLUT recipe would train on. Returns their ids (empty when the corpus is fine)."""
    return [r.get("id", "?") for r in rows if r.get("segments") and masked_spans(r["segments"])]


def _templater(processor):
    return processor if getattr(processor, "chat_template", None) else processor.tokenizer


def encode(prompt, row_or_segments, processor, data):
    """(prompt_ids, ids, labels) for one row. `row_or_segments` is the segment list, or a row (its `segments`, else
    its whole `completion` with loss). The ids equal `data.encode(prompt, completion, processor)`."""
    segments = row_or_segments
    if isinstance(row_or_segments, dict):
        segments = row_or_segments.get("segments") or [{"text": row_or_segments["completion"], "loss": True}]
    completion = completion_of(segments)
    prompt_ids, ids = data.encode(prompt, completion, processor)
    labels = [IGNORE] * len(prompt_ids) + list(ids[len(prompt_ids):])
    if not masked_spans(segments):
        return prompt_ids, ids, labels
    templater, tokenizer = _templater(processor), processor.tokenizer
    chat = [{"role": "user", "content": [{"type": "text", "text": prompt}]}]
    prompt_text = templater.apply_chat_template(chat, tokenize=False, add_generation_prompt=True)
    chat.append({"role": "assistant", "content": [{"type": "text", "text": completion}]})
    text = templater.apply_chat_template(chat, tokenize=False)
    at = text.find(completion, len(prompt_text))
    if not text.startswith(prompt_text) or at < 0 or text[len(prompt_text):at].strip():
        raise SegmentError("the chat template does not place the completion verbatim after the generation prompt")
    cut = len(prompt_text)
    lead = at - cut  # whitespace the template writes between the generation prompt and the reply (SmolVLM: one space)

    def boundary(chars):
        part = tokenizer.encode(text[:chars], add_special_tokens=False)
        if ids[:len(part)] != part:
            raise SegmentError("a segment boundary falls inside a token (the spans do not tokenise independently)")
        return len(part)

    start = boundary(cut)
    if start != len(prompt_ids):
        raise SegmentError("the prompt boundary differs from the trainer's prompt ids")
    for n, seg in enumerate(segments):
        cut += len(seg["text"]) + (lead if n == 0 else 0)  # the template's space belongs to the first span
        end = min(boundary(cut), len(ids))
        if not seg["loss"]:
            for i in range(start, end):
                labels[i] = IGNORE
        start = end
    if all(x == IGNORE for x in labels):
        raise SegmentError("no token carries loss")
    return prompt_ids, ids, labels


def collate(batch, pad_id):
    """Right-pad (prompt_ids, ids, labels) triples: input_ids, attention_mask, labels (-100 over padding)."""
    width = max(len(ids) for _, ids, _ in batch)
    out = {"input_ids": [], "attention_mask": [], "labels": []}
    for _, ids, labels in batch:
        pad = width - len(ids)
        out["input_ids"].append(list(ids) + [pad_id] * pad)
        out["attention_mask"].append([1] * len(ids) + [0] * pad)
        out["labels"].append(list(labels) + [IGNORE] * pad)
    return out
