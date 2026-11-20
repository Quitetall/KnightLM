#!/usr/bin/env python3
"""A scripted UCI engine for tests; stands in for lc0 and Maia, never for Stockfish scores.

Config (JSON file named by FAKE_UCI_SCRIPT):
  name       engine name reported by `id name`
  positions  {fen: [[move, cp], ...]} best line first; a position not listed answers
             `a1a2` at 0 cp (child positions reached while scoring a move)
  maia_positions  the same table used when the WeightsFile name contains 'maia'
  strict     true: exit with status 3 on `setoption name Hash` (lc0 has no Hash option)
  need_weights  a path: exit with status 4 unless `setoption name WeightsFile value <path>` arrived
Every received line is appended to FAKE_UCI_LOG when that variable is set.
"""
import json
import os
import sys

config = json.load(open(os.environ["FAKE_UCI_SCRIPT"]))
log = open(os.environ["FAKE_UCI_LOG"], "a") if os.environ.get("FAKE_UCI_LOG") else None
fen, multipv, weights = None, 1, None


def out(line):
    print(line, flush=True)


for raw in sys.stdin:
    line = raw.strip()
    if log:
        log.write(line + "\n")
        log.flush()
    words = line.split()
    if not words:
        continue
    if words[0] == "uci":
        out(f"id name {config.get('name', 'FakeUCI 0')}")
        out("id author test")
        out("uciok")
    elif words[0] == "isready":
        out("readyok")
    elif words[0] == "setoption":
        name = words[words.index("name") + 1]
        value = " ".join(words[words.index("value") + 1:]) if "value" in words else ""
        if name == "Hash" and config.get("strict"):
            sys.exit(3)
        if name == "MultiPV":
            multipv = int(value)
        if name == "WeightsFile":
            weights = value
    elif words[0] == "position":
        fen = " ".join(words[2:8])
    elif words[0] == "go":
        if config.get("need_weights") and weights != config["need_weights"]:
            sys.exit(4)
        limit = int(words[2])
        table = config["positions"]
        if weights and "maia" in os.path.basename(weights) and "maia_positions" in config:
            table = config["maia_positions"]  # a Maia net answers differently from the lc0 net
        lines = table.get(fen) or [["a1a2", 0]]
        for k, (move, cp) in enumerate(lines[:multipv], start=1):
            out(f"info depth 1 seldepth 1 multipv {k} score cp {cp} nodes {limit} nps 1000 pv {move}")
        out(f"bestmove {lines[0][0]}")
    elif words[0] == "quit":
        break
