"""Experimental packet boundary. No model inference or chess engine."""
import argparse
import json
from pathlib import Path

REQUIRED = {"schema_version", "ruleset", "fen", "history_complete", "repetition_ledger", "facts"}
OPTIONAL = {"legal_moves", "features"}
FACTS = {"in_check", "checkmate", "stalemate", "repetition_count", "threefold_claim_now",
         "fifty_move_claim_now", "fivefold_automatic", "seventy_five_move_automatic",
         "dead_position", "terminal_reason"}


def load_packet(path):
    packet = json.loads(Path(path).read_text())
    if not isinstance(packet, dict) or not REQUIRED <= packet.keys() or packet.keys() - REQUIRED - OPTIONAL:
        raise ValueError("invalid packet fields")
    if type(packet["schema_version"]) is not int or packet["schema_version"] != 1:
        raise ValueError("unsupported packet version")
    if packet["ruleset"] != "standard-untimed-v1":
        raise ValueError("unsupported ruleset")
    if type(packet["history_complete"]) is not bool:
        raise ValueError("history_complete must be boolean")
    fields = packet["fen"].split()
    if len(fields) != 6 or fields[1] not in ("w", "b"):
        raise ValueError("invalid FEN fields")
    if not fields[4].isascii() or not fields[4].isdigit() or not fields[5].isascii() or not fields[5].isdigit():
        raise ValueError("invalid FEN counters")
    if not 0 <= int(fields[4]) <= 2**32 - 1 or not 1 <= int(fields[5]) <= 2**32 - 1:
        raise ValueError("FEN counter outside u32 range")
    square_map(fields[0])
    if not isinstance(packet["repetition_ledger"], dict):
        raise ValueError("invalid repetition ledger")
    for key, count in packet["repetition_ledger"].items():
        if len(key.split()) != 4 or type(count) is not int or not 1 <= count <= 2**32 - 1:
            raise ValueError("invalid repetition entry")
    facts = packet["facts"]
    if not isinstance(facts, dict) or facts.keys() != FACTS:
        raise ValueError("invalid rule facts")
    for name in ("in_check", "checkmate", "stalemate", "fifty_move_claim_now", "seventy_five_move_automatic"):
        if type(facts[name]) is not bool:
            raise ValueError("invalid boolean fact")
    for name in ("threefold_claim_now", "fivefold_automatic", "dead_position"):
        if facts[name] is not None and type(facts[name]) is not bool:
            raise ValueError("invalid optional boolean fact")
    count = facts["repetition_count"]
    if count is not None and (type(count) is not int or not 0 <= count <= 2**32 - 1):
        raise ValueError("invalid repetition count")
    if facts["terminal_reason"] is not None and not isinstance(facts["terminal_reason"], str):
        raise ValueError("invalid terminal reason")
    if "legal_moves" in packet and (not isinstance(packet["legal_moves"], list) or
                                    not all(isinstance(m, str) for m in packet["legal_moves"])):
        raise ValueError("invalid legal moves")
    if "features" in packet:
        features = packet["features"]
        if not isinstance(features, dict) or features.keys() != {"material_counts", "pinned_to_move", "attacks"}:
            raise ValueError("invalid derived features")
    return packet


def square_map(placement):
    """a8..h1 expansion; retains empty squares, so exact-board equality is testable."""
    ranks = placement.split("/")
    if len(ranks) != 8:
        raise ValueError("eight ranks required")
    result = []
    for rank in ranks:
        expanded = []
        for char in rank:
            if char in "12345678":
                expanded.extend(["."] * int(char))
            elif char in "prnbqkPRNBQK":
                expanded.append(char)
            else:
                raise ValueError("invalid piece or empty-square run")
        if len(expanded) != 8:
            raise ValueError("eight squares per rank required")
        result.extend(expanded)
    return result


def canonical(packet):
    return json.dumps(packet, sort_keys=True, separators=(",", ":"), ensure_ascii=False, allow_nan=False)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("packet")
    args = parser.parse_args()
    print(canonical(load_packet(args.packet)))


if __name__ == "__main__":
    main()
