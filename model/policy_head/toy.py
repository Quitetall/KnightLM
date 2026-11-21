"""A tiny random-weight causal LM on CPU, standing in for the host in tests. Not a chess model."""

import torch
from torch import nn

from model.policy_head.head import last_hidden


class ToyHost(nn.Module):
    def __init__(self, vocab=64, hidden=32, layers=2, heads=4, max_len=32, seed=0):
        super().__init__()
        gen = torch.manual_seed(seed)  # noqa: F841 - seeds init
        self.hidden_size = hidden
        self.embed = nn.Embedding(vocab, hidden)
        self.pos = nn.Embedding(max_len, hidden)
        layer = nn.TransformerEncoderLayer(hidden, heads, hidden * 2, dropout=0.0, batch_first=True)
        self.blocks = nn.TransformerEncoder(layer, layers)

    def forward(self, ids, attention_mask):
        t = ids.shape[1]
        causal = torch.triu(torch.ones(t, t, dtype=torch.bool, device=ids.device), 1)
        x = self.embed(ids) + self.pos(torch.arange(t, device=ids.device))
        states = self.blocks(x, mask=causal, src_key_padding_mask=~attention_mask.bool())
        return last_hidden(states, attention_mask)
