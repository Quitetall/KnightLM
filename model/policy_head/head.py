"""Move-policy head: a linear classifier over the 1968 ctok/1 moves on the LLM's last hidden state.

The legal mask comes from the compiler's packet (`space.legal_mask_row`) and is applied at train
and inference time. Raw logits are kept so the unmasked argmax can be scored as a diagnostic;
the legal-masked distribution is the product. No engine is involved anywhere.
"""

import torch
from torch import nn
import torch.nn.functional as F

from model.policy_head import space


def legal_mask(legal_move_lists, device=None):
    """(B, SIZE) bool tensor from a list of packet legal_moves lists."""
    return torch.tensor([space.legal_mask_row(m) for m in legal_move_lists], dtype=torch.bool, device=device)


def target_tensor(targets, legal_move_lists, device=None):
    return torch.tensor([space.target_index(t, m) for t, m in zip(targets, legal_move_lists)],
                        dtype=torch.long, device=device)


def last_hidden(hidden_states, attention_mask):
    """Hidden state at the last real (non-pad, right-padded) position: (B, T, H) -> (B, H)."""
    last = attention_mask.long().sum(dim=1) - 1
    return hidden_states[torch.arange(hidden_states.shape[0], device=hidden_states.device), last]


def mask_logits(logits, mask):
    if not bool(mask.any(dim=-1).all()):
        raise ValueError("a row has an empty legal set")
    return logits.masked_fill(~mask, float("-inf"))


class MoveHead(nn.Module):
    def __init__(self, hidden_size, dropout=0.0):
        super().__init__()
        self.norm = nn.LayerNorm(hidden_size)
        self.drop = nn.Dropout(dropout)
        self.proj = nn.Linear(hidden_size, space.SIZE)

    def forward(self, hidden):
        """Raw (unmasked) logits, (B, SIZE)."""
        return self.proj(self.drop(self.norm(hidden)))

    def loss(self, hidden, mask, target):
        """Cross-entropy over legal moves only (illegal logits are -inf before the softmax)."""
        return F.cross_entropy(mask_logits(self(hidden), mask), target)

    @torch.no_grad()
    def predict(self, hidden, mask, k=3):
        raw = self(hidden)
        masked = mask_logits(raw, mask)
        kk = min(k, int(mask.sum(dim=-1).min()))
        return {"raw_logits": raw, "raw_argmax": raw.argmax(-1), "masked_argmax": masked.argmax(-1),
                "masked_topk": masked.topk(kk, dim=-1).indices, "masked_probs": masked.softmax(-1)}
