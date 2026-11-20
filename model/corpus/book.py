"""Pilot adapter: a public-domain chess book in plain text -> position-linked records.

Moves are found in prose and replayed on the notation crate through
`knightlm replay-lines`; anything that fails to replay, or has no known start
position, is quarantined with a reason instead of being linked to a board.
Written for English descriptive notation (Capablanca, Chess Fundamentals, 1921).
"""

import argparse
from collections import Counter
import json
from pathlib import Path
import re
import subprocess

from model.corpus.records import make_record, write_records

ADAPTER = "book-descriptive-v1"
PAGE = re.compile(r"\s*\{\d+\}")
ROW = re.compile(r"^\s{3,}(\d{1,3})\.\s+(\S.*?)(?:\s{2,}(\S.*?))?\s*$")
MOVE_CELL = re.compile(r"^(?:O ?- ?O(?: ?- ?O)?|[A-Za-z0-9 ()=]+? ?(?:-|x) ?[A-Za-z0-9 ()=]+?)\.?$")
NOT_A_MOVE = re.compile(r"^(resigns?\.?|draws?\.?|\.{2,}|\s*)$", re.I)
FRAGMENT = re.compile(
    r"^(?:(\d{1,3})\s*(\.\.\.|\.)?\s*)?((?:O\s?-\s?O(?:\s?-\s?O)?|(?:Q\s|K\s)?(?:Kt|R|B|Q|K|P)(?:\s(?:Kt|R|B|Q|K))?\s?(?:P\s)?[-x]\s?[A-Za-z0-9 ()=]*?)(?:\s?(?:ch|mate))?)\s*$"
)
SQUARE_END = re.compile(r"(\d|sq|P|Kt|B|R|Q|K|O)(?:\s?\(?[QRBKt]*\)?)?(?:\s?(?:ch|mate))?$")
LICENCE = "Public domain (US): first published 1921; Project Gutenberg eBook #33870"


def strip_gutenberg(text):
    start = text.index("*** START OF")
    start = text.index("\n", start) + 1
    return text[start : text.index("*** END OF")]


def paragraphs_and_rows(body):
    """Yield ('rows', [(number, white, black), ...]) and ('prose', text) in book order."""
    block, prose = [], []
    for line in body.splitlines():
        match = ROW.match(line)
        if match and all(MOVE_CELL.match(c) or NOT_A_MOVE.match(c) for c in match.groups()[1:] if c):
            if prose:
                yield "prose", " ".join(prose)
                prose = []
            number, white, black = match.groups()
            block.append((int(number), white, black))
            continue
        if block:
            yield "rows", block
            block = []
        if line.strip():
            prose.append(line.strip())
        elif prose:
            yield "prose", " ".join(prose)
            prose = []
    if block:
        yield "rows", block
    if prose:
        yield "prose", " ".join(prose)


def compact(move):
    return re.sub(r"\s+", " ", move.strip().rstrip(";,."))


def games(items):
    """Group move rows into games by numbering; prose is attached after the plies played so far.

    A block that starts at move 1 begins a new game. A block whose number does
    not continue the open game has no known start position (a diagram in the book).
    """
    out, game = [], None
    for kind, value in items:
        if kind == "rows":
            first = value[0][0]
            if first == 1 or game is None or first != game["next"]:
                game = {"start": "standard" if first == 1 else "unknown", "sans": [], "next": first,
                        "first_number": first, "notes": []}
                out.append(game)
            for number, white, black in value:
                if number != game["next"]:
                    game["gap"] = True
                if not NOT_A_MOVE.match(white):
                    game["sans"].append(compact(white))
                if black and not NOT_A_MOVE.match(black):
                    game["sans"].append(compact(black))
                game["next"] = number + 1
        elif game is not None:
            game["notes"].append((len(game["sans"]), PAGE.sub("", value)))
    return out


def inline_sequences(text):
    """Numbered move runs inside prose, e.g. '4 K - K 4, K - Q 1; 5 K - Q 5'."""
    sequences, current = [], None
    for piece in re.split(r"[;,]|(?<=\d)\.\s", PAGE.sub("", text)):
        piece = piece.strip().rstrip(".")
        match = FRAGMENT.match(piece)
        if not match:  # a run may start after prose words: "It goes 4 K - K 4"
            for digits in re.finditer(r"(?<![\w.])\d{1,3}(?=\s*(?:\.\.\.|\.)?\s*[A-Zo])", piece):
                match = FRAGMENT.match(piece[digits.start():])
                if match:
                    piece = piece[digits.start():]
                    break
        if not match or not SQUARE_END.search(piece):
            current = None
            continue
        number, dots, move = match.groups()
        if number and current is None:
            current = {"number": int(number), "black_first": dots == "...", "sans": []}
            sequences.append(current)
        if current is not None:
            current["sans"].append(compact(move))
        elif number:
            continue
    return [s for s in sequences if len(s["sans"]) >= 2]


def replay(binary, requests, timeout):
    payload = "".join(json.dumps(r) + "\n" for r in requests)
    result = subprocess.run([str(binary), "replay-lines"], input=payload.encode(),
                            capture_output=True, timeout=timeout)
    if result.returncode:
        raise ValueError(result.stderr.decode().strip())
    return [json.loads(line) for line in result.stdout.splitlines()]


def reason(error):
    if error == "empty move":
        return "empty_move"
    if "ambiguous" in error:
        return "ambiguous_move"
    if "no legal move" in error:
        return "no_legal_move_matches"
    return "other"


def build(text, binary, source_sha256, timeout=300):
    body = strip_gutenberg(text)
    items = list(paragraphs_and_rows(body))
    found = games(items)
    stats, quarantine, records = Counter(), [], []
    stats["prose_paragraphs"] = sum(kind == "prose" for kind, _ in items)
    known = [g for g in found if g["start"] == "standard"]
    orphans = [g for g in found if g["start"] != "standard"]
    stats["move_blocks_total"] = len(found)
    stats["blocks_numbered_from_1"] = len(known)
    stats["blocks_without_start_position"] = len(orphans)
    stats["plies_without_start_position"] = sum(len(g["sans"]) for g in orphans)
    results = replay(binary, [{"id": i, "notation": "descriptive", "sans": g["sans"]} for i, g in enumerate(known)], timeout)
    for index, (game, result) in enumerate(zip(known, results, strict=True)):
        if not result["ok"] and result["failed_at"] == 0:
            # Numbered from 1 but illegal as a first move: a diagram position, not the start.
            stats["blocks_numbered_1_but_first_move_illegal"] += 1
            stats["plies_without_start_position"] += len(game["sans"])
            quarantine.append({"stage": "game", "game": index, "ply": 1, "san": game["sans"][0],
                               "reason": "no_start_position", "error": result.get("error")})
            continue
        stats["plies_found"] += len(game["sans"])
        stats["plies_replayed"] += result["played"]
        if not result["ok"]:
            stats["games_failed"] += 1
            at = result["failed_at"]
            quarantine.append({"stage": "game", "game": index, "ply": at + 1 if at is not None else None,
                               "san": game["sans"][at] if at is not None else None,
                               "reason": reason(result.get("error", "")), "error": result.get("error")})
        else:
            stats["games_replayed_fully"] += 1
        for ply, note in game["notes"]:
            stats["notes_total"] += 1
            if ply == 0 or ply > result["played"]:
                stats["notes_unlinked_replay_failed" if ply else "notes_before_first_move"] += 1
                continue
            if len(note.split()) < 12:
                stats["notes_too_short"] += 1
                continue
            stats["notes_linked"] += 1
            records.append(make_record(
                "capablanca-chess-fundamentals-1921", LICENCE, "commentary", note,
                [{"role": "after", "fen": result["fens"][ply - 1]}],
                {"adapter": ADAPTER, "source_sha256": source_sha256, "game": index, "ply": ply,
                 "last_san": game["sans"][ply - 1], "notation": "descriptive"}))
    sequences = [s for kind, value in items if kind == "prose" for s in inline_sequences(value)]
    stats["inline_sequences"] = len(sequences)
    stats["inline_plies"] = sum(len(s["sans"]) for s in sequences)
    from_start = [s for s in sequences if s["number"] == 1 and not s["black_first"]]
    stats["inline_sequences_from_move_1"] = len(from_start)
    stats["inline_sequences_no_start_position"] = len(sequences) - len(from_start)
    inline = replay(binary, [{"id": i, "notation": "descriptive", "sans": s["sans"]} for i, s in enumerate(from_start)], timeout)
    for sequence, result in zip(from_start, inline, strict=True):
        stats["inline_from_start_replayed_fully"] += result["ok"]
        if not result["ok"]:
            quarantine.append({"stage": "inline", "san": sequence["sans"][result["failed_at"]] if result["failed_at"] is not None else None,
                               "reason": reason(result.get("error", "")), "error": result.get("error")})
    stats["san_like_tokens_in_prose"] = len(re.findall(r"\b(?:[NBRQK]?[a-h]?x?[a-h][1-8](?:=[NBRQ])?[+#]?)\b", body))
    stats["quarantined"] = len(quarantine)
    return records, dict(stats), quarantine


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--text", type=Path, required=True, help="Project Gutenberg plain text")
    parser.add_argument("--knightlm", type=Path, default=Path("target/release/knightlm"))
    parser.add_argument("--records", type=Path)
    parser.add_argument("--stats", type=Path, required=True)
    args = parser.parse_args()
    from model.corpus.pipeline import sha256_file

    digest = sha256_file(args.text)
    records, stats, quarantine = build(args.text.read_text(encoding="utf-8"), args.knightlm, digest)
    if args.records:
        write_records(args.records, records)
    reasons = Counter(q["reason"] for q in quarantine)
    report = {"source": {"file": args.text.name, "sha256": digest}, "licence": LICENCE, "stats": stats,
              "quarantine_reasons": dict(reasons), "quarantine_sample": quarantine[:25]}
    with args.stats.open("x") as stream:
        json.dump(report, stream, indent=2)
        stream.write("\n")


if __name__ == "__main__":
    main()
