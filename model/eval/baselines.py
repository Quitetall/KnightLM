"""Reference predictors with no model: a floor to read model scorecards against.

random_legal picks uniformly among the compiler's legal moves, seeded from the
position, so it is deterministic and always legal. It is NOT a stand-in for the
model: it has no raw-output failure mode, which is exactly why the raw-legality
axis exists.
"""

import hashlib

from model.eval.predictors import Prediction


class RandomLegal:
    def __init__(self, seed=0):
        self.seed = seed

    def predict(self, packet, mode):
        digest = hashlib.sha256(f"{self.seed}|{packet['fen']}".encode()).digest()
        moves = sorted(packet["legal_moves"])
        move = moves[int.from_bytes(digest[:8], "big") % len(moves)]
        return Prediction(move=move, raw_reply=move, legal=True, input_tokens=0, output_tokens=0, latency_ms=0.0)

    def manifest(self):
        return {"kind": "random_legal", "seed": self.seed}


def random_legal():
    return RandomLegal()
