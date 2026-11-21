"""Small CNN square classifier: one 32x32 RGB crop -> 13 class logits (about 60k parameters)."""
import torch
from torch import nn

from .fen import NUM_CLASSES


class SquareCNN(nn.Module):
    def __init__(self, width: int = 24):
        super().__init__()

        def block(i, o):
            return nn.Sequential(nn.Conv2d(i, o, 3, padding=1, bias=False), nn.BatchNorm2d(o), nn.ReLU(inplace=True),
                                 nn.MaxPool2d(2))

        self.features = nn.Sequential(block(3, width), block(width, width * 2), block(width * 2, width * 2))
        self.head = nn.Sequential(nn.Flatten(), nn.Linear(width * 2 * 16, 96), nn.ReLU(inplace=True),
                                  nn.Linear(96, NUM_CLASSES))

    def forward(self, x):  # x: (B, 3, 32, 32) float in 0..1
        return self.head(self.features(x))


def to_tensor(cells):
    """uint8 (N, 32, 32, 3) numpy -> float tensor (N, 3, 32, 32)."""
    return torch.from_numpy(cells).permute(0, 3, 1, 2).float().div(255.0)


@torch.no_grad()
def predict(model, cells, batch: int = 1024):
    model.eval()
    out = []
    for i in range(0, len(cells), batch):
        out.append(model(to_tensor(cells[i : i + batch])).argmax(1))
    return torch.cat(out).numpy()
