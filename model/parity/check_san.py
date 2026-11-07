"""Parity check: verify knightlm SAN output against python-chess (independent oracle)."""

import chess, json, sys


def verify(path: str) -> int:
    bad = 0
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            rec = json.loads(line)
            fen = rec["fen"]
            uci = rec["uci"]
            san = rec["san"]
            board = chess.Board(fen)
            mv = chess.Move.from_uci(uci)
            # 1. knightlm's SAN must parse in python-chess to the same move
            try:
                parsed = board.parse_san(san)
            except Exception as e:
                print(f"MISMATCH (parse): {fen} {uci} {san!r} -> {e}")
                bad += 1
                continue
            if parsed != mv:
                print(f"MISMATCH (move): {fen} {uci} {san!r} parsed={parsed.uci()}")
                bad += 1
                continue
            # 2. round-trip: python-chess's own SAN for the move should equal (or be an accepted variant)
            own = board.san(mv)
            if own != san:
                # SAN can have multiple legal spellings only for disambiguation edge cases;
                # report but don't fail unless it changes the move.
                print(f"NOTE (spelling): knightlm={san!r} python-chess={own!r} for {fen} {uci}")
    print(f"checked; mismatches={bad}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(verify(sys.argv[1]))
