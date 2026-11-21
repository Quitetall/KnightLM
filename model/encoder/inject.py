"""Put encoder soft tokens into a host idefics3-family model (SmolVLM) through its own slot.

Idefics3 and SmolVLM take image features as `image_hidden_states` of shape
`(n_blocks, tokens_per_block, text_hidden)` and write them over the `<image>` token
positions of `input_ids` (`Idefics3Model.inputs_merger` / `SmolVLMModel.inputs_merger`):
the n-th run of `tokens_per_block` image tokens takes the n-th block. We pass the
projected board tokens as `image_hidden_states`, so the vision tower and the connector
are bypassed and nothing in the host is patched. The count of `<image>` ids per board
must equal the number of soft tokens (64, or 64 + n_global). SmolVLM-500M uses 64.
"""

import torch


def board_block(input_ids, image_token_id, n_tokens):
    """Placeholder ids for one board: `n_tokens` copies of the host's `<image>` id."""
    return [image_token_id] * n_tokens


def check_slots(input_ids, image_token_id, soft):
    """Refuse a mismatch the host would otherwise reject late (or, worse, accept silently)."""
    per_sample = (input_ids == image_token_id).sum(dim=1)
    expected = soft.shape[0] // input_ids.shape[0] * soft.shape[1]
    if soft.shape[0] != input_ids.shape[0] or not bool((per_sample == expected).all()):
        raise ValueError(f"<image> slots per sample {per_sample.tolist()} do not match "
                         f"{soft.shape[1]} soft tokens per board x {soft.shape[0] // input_ids.shape[0]} boards")


def soft_tokens(encoder, projector, planes):
    """Planes (B, 22, 8, 8) -> (B, n_tokens, host_hidden)."""
    return projector(encoder(planes))


def host_forward(host, input_ids, soft, attention_mask=None, labels=None, **kwargs):
    """Run the host with board tokens in the image slot (no pixel_values)."""
    check_slots(input_ids, host.config.image_token_id, soft)
    return host(input_ids=input_ids, attention_mask=attention_mask, labels=labels,
                image_hidden_states=soft.to(host.dtype), **kwargs)


def injected_embeds(host, input_ids, soft):
    """The input embeddings the host's language model will see (for tests and debugging)."""
    inner = host.model if hasattr(host, "model") and hasattr(host.model, "inputs_merger") else host
    embeds = inner.get_input_embeddings()(input_ids)
    return inner.inputs_merger(input_ids, embeds, soft.to(embeds.dtype))
