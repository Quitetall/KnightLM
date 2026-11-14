"""Fake predictors for tests and the worked example. They never touch an engine.

The fixture rows (fixtures/worked-corpus/holdout-00000.jsonl) are six real Run 1 holdout rows; every
fake is indexed by the row's position in that file.
"""

import json
import time
from pathlib import Path

from model.eval.predictors import Prediction

FIXTURE = Path(__file__).with_name("fixtures") / "worked-corpus" / "holdout-00000.jsonl"


def fixture_rows():
    return [json.loads(line) for line in FIXTURE.read_text().splitlines()]


class Fake:
    """Base: look the row up by FEN, then ask `reply(index, packet, mode)`."""

    def __init__(self):
        self.rows = fixture_rows()
        self.index = {row["packet"]["fen"]: i for i, row in enumerate(self.rows)}

    def predict(self, packet, mode):
        i = self.index.get(packet["fen"], -1)
        out = self.reply(i, packet, mode)
        move, reply, tokens, latency = out
        return Prediction(move=move, raw_reply=reply, legal=True, input_tokens=100 + i,
                          output_tokens=tokens, latency_ms=latency)

    def target(self, i):
        return self.rows[i]["target"]

    def other_legal(self, i):
        return next(m for m in self.rows[i]["packet"]["legal_moves"] if m != self.rows[i]["target"])

    def manifest(self):
        return {"fake": type(self).__name__}


class Perfect(Fake):
    def reply(self, i, packet, mode):
        move = self.target(i)
        return move, move, 4, 10.0 + i


class AlwaysIllegal(Fake):
    """Well-formed UCI that is never legal; claims legality anyway."""

    def reply(self, i, packet, mode):
        return "a1a1", "a1a1", 4, 10.0


class Garbled(Fake):
    def reply(self, i, packet, mode):
        replies = ["I think Nf3 is best!", "", "e2e4e5", "```json {", "E2E4", "e9e4"]
        text = replies[i % len(replies)]
        return (text or None), text, 7, 12.0


class Timeout(Fake):
    """Hangs on odd rows (longer than any test timeout), answers perfectly otherwise."""

    def reply(self, i, packet, mode):
        if i % 2:
            time.sleep(5)
        return self.target(i), self.target(i), 4, 10.0


class Crashing(Fake):
    def reply(self, i, packet, mode):
        if i % 2:
            raise RuntimeError("server returned 500")
        return self.target(i), self.target(i), 4, 10.0


class FirstLegal(Fake):
    def reply(self, i, packet, mode):
        move = packet["legal_moves"][0]
        return move, move, 4, 10.0


class WorkedExample(Fake):
    """The scripted predictor behind docs/model/strength-baseline.md's worked example.

    raw:         0 matches the human move, 1 legal but different, 2 illegal, 3 garbled prose,
                 4 timeout, 5 crash.
    constrained: 0 matches, 1..5 legal but different (constraint repaired every raw failure).
    """

    def reply(self, i, packet, mode):
        if mode == "constrained":
            move = self.target(0) if i == 0 else self.other_legal(i)
            return move, move, 4, 20.0 + i
        if i == 0:
            return self.target(0), self.target(0), 4, 10.0
        if i == 1:
            return self.other_legal(1), self.other_legal(1), 4, 12.0
        if i == 2:
            return "e1e8", "e1e8", 4, 14.0
        if i == 3:
            return "Knight to f3", "Knight to f3", 8, 16.0
        if i == 4:
            time.sleep(5)
        raise RuntimeError("server returned 500")


def perfect():
    return Perfect()


def always_illegal():
    return AlwaysIllegal()


def garbled():
    return Garbled()


def timeout():
    return Timeout()


def crashing():
    return Crashing()


def first_legal():
    return FirstLegal()


def worked_example():
    return WorkedExample()
