"""The board encoder, its rules-QA heads and the per-host projector (PyTorch).

  planes (B, 22, 8, 8) -> BoardEncoder -> tokens (B, 64 + n_global, d_model)
  tokens -> RulesHeads            (pretraining only; the encoder is frozen afterwards)
  tokens -> Projector -> (B, 64 + n_global, host_hidden)   (one projector per host LLM)

Nothing here knows a chess rule: the encoder learns them from labels the compiler computed.
"""

import json
from dataclasses import asdict, dataclass
from pathlib import Path

import torch
from torch import nn
from torch.nn import functional as F

from model.encoder.planes import PLANES

# Production size is about 5M parameters; tests use `EncoderConfig.tiny()`.
@dataclass
class EncoderConfig:
    d_model: int = 256
    n_layers: int = 6
    n_heads: int = 8
    ff_mult: int = 4
    n_global: int = 0  # extra learned tokens after the 64 square tokens; 16 gives 80
    dropout: float = 0.0
    in_planes: int = PLANES

    @classmethod
    def tiny(cls):
        return cls(d_model=32, n_layers=2, n_heads=4, ff_mult=2)

    @property
    def n_tokens(self):
        return 64 + self.n_global


class BoardEncoder(nn.Module):
    """One token per square (a1 first), self-attention across the board, optional global tokens."""

    def __init__(self, cfg):
        super().__init__()
        self.cfg = cfg
        self.embed = nn.Linear(cfg.in_planes, cfg.d_model)
        self.square_pos = nn.Parameter(torch.randn(64, cfg.d_model) * 0.02)
        self.global_tokens = nn.Parameter(torch.randn(cfg.n_global, cfg.d_model) * 0.02)
        layer = nn.TransformerEncoderLayer(
            cfg.d_model, cfg.n_heads, cfg.d_model * cfg.ff_mult, cfg.dropout,
            activation="gelu", batch_first=True, norm_first=True)
        self.blocks = nn.TransformerEncoder(layer, cfg.n_layers, enable_nested_tensor=False)
        self.norm = nn.LayerNorm(cfg.d_model)

    def forward(self, planes):
        b = planes.shape[0]
        x = self.embed(planes.reshape(b, self.cfg.in_planes, 64).transpose(1, 2)) + self.square_pos
        x = torch.cat([x, self.global_tokens.expand(b, -1, -1)], dim=1)
        return self.norm(self.blocks(x))


class RulesHeads(nn.Module):
    """Rules-QA heads over the encoder tokens. Square tokens are the first 64."""

    def __init__(self, d):
        super().__init__()
        self.piece = nn.Linear(d, 13)          # empty + 12 pieces, per square
        self.attacked = nn.Linear(d, 2)        # attacked by white, by black, per square
        self.move_from = nn.Linear(d, d)       # legal-move mask: from-square x to-square bilinear
        self.move_to = nn.Linear(d, d)
        self.check = nn.Linear(d, 1)           # side to move is in check
        self.material = nn.Linear(d, 10)       # P N B R Q counts, white then black
        self.scale = d ** -0.5

    def forward(self, tokens):
        sq, pooled = tokens[:, :64], tokens.mean(dim=1)
        legal = torch.einsum("bid,bjd->bij", self.move_from(sq), self.move_to(sq)) * self.scale
        return {"piece": self.piece(sq), "attacked": self.attacked(sq).transpose(1, 2), "legal": legal,
                "check": self.check(pooled).squeeze(-1), "counts": self.material(pooled)}


class Projector(nn.Module):
    """Encoder tokens -> the host LLM's embedding width. One per host; the encoder stays frozen."""

    def __init__(self, d_encoder, host_hidden, mid=None):
        super().__init__()
        mid = mid or host_hidden
        self.net = nn.Sequential(nn.LayerNorm(d_encoder), nn.Linear(d_encoder, mid), nn.GELU(),
                                 nn.Linear(mid, host_hidden))
        self.d_encoder, self.host_hidden, self.mid = d_encoder, host_hidden, mid

    def forward(self, tokens):
        return self.net(tokens)


def to_targets(rows_targets):
    """Stack `labels.targets` dicts into tensors."""
    return {
        "piece": torch.tensor([t["piece"] for t in rows_targets], dtype=torch.long),
        "attacked": torch.tensor([t["attacked"] for t in rows_targets], dtype=torch.float32),
        "legal": torch.tensor([t["legal"] for t in rows_targets], dtype=torch.float32),
        "check": torch.tensor([t["check"] for t in rows_targets], dtype=torch.float32),
        "counts": torch.tensor([t["counts"] for t in rows_targets], dtype=torch.float32),
    }


def head_losses(out, tgt, legal_pos_weight=8.0):
    """Per-head losses, kept separate. Training sums them; evaluation never does."""
    pw = torch.tensor(legal_pos_weight)
    return {
        "piece": F.cross_entropy(out["piece"].reshape(-1, 13), tgt["piece"].reshape(-1)),
        "attacked": F.binary_cross_entropy_with_logits(out["attacked"], tgt["attacked"]),
        "legal": F.binary_cross_entropy_with_logits(out["legal"], tgt["legal"], pos_weight=pw),
        "check": F.binary_cross_entropy_with_logits(out["check"], tgt["check"]),
        "counts": F.smooth_l1_loss(out["counts"], tgt["counts"]),
    }


def save_encoder(path, encoder, heads=None):
    """`encoder.pt` (state dict) beside `config.json`; the pair is what a projector run pins."""
    path = Path(path)
    path.mkdir(parents=True, exist_ok=True)
    (path / "config.json").write_text(json.dumps(asdict(encoder.cfg), sort_keys=True))
    torch.save(encoder.state_dict(), path / "encoder.pt")
    if heads is not None:
        torch.save(heads.state_dict(), path / "heads.pt")


def load_encoder(path):
    path = Path(path)
    encoder = BoardEncoder(EncoderConfig(**json.loads((path / "config.json").read_text())))
    encoder.load_state_dict(torch.load(path / "encoder.pt", map_location="cpu", weights_only=True))
    return encoder


def save_projector(path, projector):
    path = Path(path)
    path.mkdir(parents=True, exist_ok=True)
    meta = {"d_encoder": projector.d_encoder, "host_hidden": projector.host_hidden, "mid": projector.mid}
    (path / "projector.json").write_text(json.dumps(meta, sort_keys=True))
    torch.save(projector.state_dict(), path / "projector.pt")


def load_projector(path):
    path = Path(path)
    projector = Projector(**json.loads((path / "projector.json").read_text()))
    projector.load_state_dict(torch.load(path / "projector.pt", map_location="cpu", weights_only=True))
    return projector
