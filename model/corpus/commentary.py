"""Pilot adapter: annotated PGN / Lichess studies -> commentary records.

Replay and FEN validation are done by `knightlm corpus-commentary` (the
notation crate), so a comment is attached to a position only if every move
before it replayed. Games that fail to replay are quarantined.
"""

import argparse
from collections import Counter
import json
from pathlib import Path
import re
import subprocess

from model.corpus.records import make_record, write_records

ADAPTER = "pgn-commentary-v1"
COMMAND = re.compile(r"\[%[^\]]*\]")
URL = re.compile(r"https?://\S+")
ENGINE_TEMPLATE = re.compile(
    r"^(Inaccuracy|Mistake|Blunder)\. .* was best\.$|^(White|Black) (resigns|offers draw|is checkmated|wins)|^(Checkmate|Stalemate|Draw|Game drawn)\b",
    re.I,
)
MIN_CHARS, MIN_WORDS, MIN_ALPHA, MIN_ASCII = 20, 4, 0.6, 0.9


def clean_comment(text):
    """Drop [%clk]-style commands and URLs; collapse whitespace."""
    return " ".join(URL.sub(" ", COMMAND.sub(" ", text)).split())


def reject_reason(text):
    """Return why cleaned text is not usable commentary, or None."""
    if len(text) < MIN_CHARS or len(text.split()) < MIN_WORDS:
        return "too_short"
    if ENGINE_TEMPLATE.match(text):
        return "engine_template"
    letters = sum(c.isalpha() for c in text)
    if letters / len(text) < MIN_ALPHA:
        return "not_prose"
    if sum(c.isascii() for c in text) / len(text) < MIN_ASCII:
        return "non_english_or_symbols"
    return None


def origin(game_id):
    """Source family from the PGN Site URL: broadcast, study chapter, or plain game link."""
    return "broadcast" if "/broadcast/" in game_id else "study" if "/study/" in game_id else "game_link"


def run_replay(binary, pgn, source, timeout):
    with pgn.open("rb") as stream:
        result = subprocess.run(
            [str(binary), "corpus-commentary", "--source", source],
            stdin=stream, capture_output=True, timeout=timeout,
        )
    if result.returncode:
        raise ValueError(result.stderr.decode().strip())
    return [json.loads(line) for line in result.stdout.splitlines()]


def extract(rows, source_id, licence, source_sha256):
    """Turn replay rows into (records, stats, quarantine). Duplicate text+position is dropped."""
    stats = Counter()
    quarantine = []
    records = []
    seen = set()
    lengths = []
    by_origin = Counter()
    for row in rows:
        kind = row["kind"]
        if kind == "quarantine":
            stats["quarantined_games"] += 1
            quarantine.append(row)
        elif kind == "game":
            stats["games"] += 1
            stats["plies"] += row["plies"]
            stats["plies_replayed"] += row["replayed"]
            stats["games_replay_failed"] += row["failed"]
            stats["comments_all"] += row["all_comments"]
            stats["comments_mainline"] += row["mainline_comments"]
        elif kind == "comment":
            stats["comment_rows"] += 1
            if row["ply"] == 0:
                stats["comment_rows_before_first_move"] += 1
            text = clean_comment(row["comment"])
            reason = reject_reason(text)
            if reason:
                stats[f"rejected_{reason}"] += 1
                continue
            fen = row.get("fen_after") or row["fen_before"]
            key = (text, " ".join(fen.split()[:4]))
            if key in seen:
                stats["rejected_duplicate"] += 1
                continue
            seen.add(key)
            positions = [{"role": "before", "fen": row["fen_before"]}]
            if row.get("fen_after"):
                positions.append({"role": "after", "fen": row["fen_after"]})
            records.append(make_record(
                source_id, licence, "commentary", text, positions,
                {"adapter": ADAPTER, "source_sha256": source_sha256, "game": row["game_id"],
                 "ply": row["ply"], "san": row["san"], "uci": row.get("uci"), "nags": row["nags"],
                 "start_fen": row["start_fen"]},
            ))
            lengths.append(len(text.split()))
            by_origin[origin(row["game_id"])] += 1
            by_origin["words>=10:" + origin(row["game_id"])] += len(text.split()) >= 10
    stats["records"] = len(records)
    stats["records_by_origin"] = dict(by_origin)
    return records, dict(stats), quarantine, sorted(lengths)


def length_summary(lengths):
    if not lengths:
        return {}
    pick = lambda q: lengths[min(len(lengths) - 1, int(q * len(lengths)))]
    bins = Counter()
    for n in lengths:
        bins["4-9" if n < 10 else "10-24" if n < 25 else "25-49" if n < 50 else "50-99" if n < 100 else "100+"] += 1
    return {"words_p10": pick(0.1), "words_p50": pick(0.5), "words_p90": pick(0.9),
            "words_max": lengths[-1], "bins": dict(bins)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pgn", type=Path, required=True)
    parser.add_argument("--source-id", required=True)
    parser.add_argument("--licence", required=True)
    parser.add_argument("--knightlm", type=Path, default=Path("target/release/knightlm"))
    parser.add_argument("--records", type=Path, help="new JSONL of accepted records")
    parser.add_argument("--stats", type=Path, required=True, help="new JSON yield report")
    parser.add_argument("--timeout", type=int, default=600)
    args = parser.parse_args()
    from model.corpus.pipeline import sha256_file

    digest = sha256_file(args.pgn)
    rows = run_replay(args.knightlm, args.pgn, args.pgn.name, args.timeout)
    records, stats, quarantine, lengths = extract(rows, args.source_id, args.licence, digest)
    if args.records:
        write_records(args.records, records)
    report = {"source": {"file": args.pgn.name, "bytes": args.pgn.stat().st_size, "sha256": digest},
              "licence": args.licence, "stats": stats, "length": length_summary(lengths),
              "quarantine_sample": quarantine[:20]}
    with args.stats.open("x") as stream:
        json.dump(report, stream, indent=2)
        stream.write("\n")


if __name__ == "__main__":
    main()
