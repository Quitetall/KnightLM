"""Corpus rows -> board-encoder rows (P1 alignment and the T4/T5 task trainer).

`model.encoder.align` reads JSONL rows `{fen, repetition?, prompt, completion}`; `model.encoder.task` reads the same
rows. Nothing in the corpora writes them, so this converter does. It takes the rows of an S0 rules-QA corpus
(`model/corpus/s0.py`: one question per position, the answer in the `reasoning` of a play call) or of a reasoning
corpus (Run 1 reasoning, reasoning v2) and gives, for each:

  fen, repetition   what `model.encoder.planes.packet_inputs` reads from the packet (the very function the server's
                    `_soft_tokens` uses), so training sees the planes serving will build
  prompt            the board-encoder prompt of the arm, `play-enc` (T4) or `play-enc-cgt` (T5): 64 `<image>`
                    slots and the rule facts, no FEN and no ledger; an S0 row's question is appended as
                    `model.prompt.qa_prompt` appends it
  completion        the row's own completion (the unchanged play call), or one rendered from its reasoning
  packet, qa, segments   carried through (the parity checks and the loss mask need them)

The S0 corpora are matched across arms (same positions, questions and answers), so the converted rows of T4 and T5
differ from each other only in the prompt kind.

    python3 -m model.encoder.rows --corpus DIR --kind play-enc --out rows.jsonl [--split train] [--limit N]
"""

import argparse
import json
from pathlib import Path
import sys

from model import prompt as prompt_module
from model.corpus.render import render_completion
from model.encoder.planes import packet_inputs

FORMAT = "knightlm.board-rows/v1"


class RowError(ValueError):
    """A corpus row that cannot become a board-encoder row."""


def convert_row(row, kind, binary=None, n_board_tokens=prompt_module.BOARD_TOKENS):
    if kind not in prompt_module.ENC_KINDS:
        raise RowError(f"kind {kind!r} is not a board-encoder kind {prompt_module.ENC_KINDS}")
    packet = row.get("packet")
    if packet is None:
        raise RowError(f"row {row.get('id')}: no packet")
    fen, repetition = packet_inputs(packet)
    prompt = prompt_module.play_prompt(kind, packet, binary=binary, n_board_tokens=n_board_tokens)
    out = {"id": row.get("id"), "game_id": row.get("game_id"), "kind": kind, "fen": fen, "repetition": repetition}
    if "qa" in row:
        question = row["qa"]["question"]
        if "\n" in question:
            raise RowError(f"row {row.get('id')}: a question is one line")
        prompt += prompt_module.QA_LABEL + question
        out["qa"] = {k: row["qa"][k] for k in ("type", "question", "answer")}
    if "completion" in row:
        completion = row["completion"]
    elif "reasoning" in row and "target" in row:
        completion = render_completion(row["reasoning"], row["target"])
    else:
        raise RowError(f"row {row.get('id')}: no completion and no reasoning to render one from")
    out.update(prompt=prompt, completion=completion, packet=packet)
    if row.get("segments"):
        if "".join(s["text"] for s in row["segments"]) != completion:
            raise RowError(f"row {row.get('id')}: segments do not spell the completion")
        out["segments"] = row["segments"]
    return out


def convert(rows, kind, binary=None, n_board_tokens=prompt_module.BOARD_TOKENS):
    return [convert_row(r, kind, binary, n_board_tokens) for r in rows]


def write_jsonl(path, rows):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as stream:
        for row in rows:
            stream.write(json.dumps(row, ensure_ascii=False, separators=(",", ":")) + "\n")
    return path


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--corpus", type=Path, required=True, help="an audited corpus directory (S0 rules QA or reasoning)")
    ap.add_argument("--kind", choices=prompt_module.ENC_KINDS, default="play-enc")
    ap.add_argument("--split", choices=("train", "holdout"), default="train")
    ap.add_argument("--limit", type=int, help="first N rows of the split")
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--knightlm", type=Path, help="compiler binary (play-enc-cgt only)")
    args = ap.parse_args(argv)
    from model.train.run import Refused, check_corpus, corpus_path
    try:
        train, holdout, audit = check_corpus(corpus_path(args.corpus))
    except Refused as error:
        print(f"REFUSED: {error}", file=sys.stderr)
        return 2
    rows = (train if args.split == "train" else holdout)[:args.limit]
    try:
        converted = convert(rows, args.kind, args.knightlm)
    except RowError as error:
        print(f"REFUSED: {error}", file=sys.stderr)
        return 2
    write_jsonl(args.out, converted)
    print(json.dumps({"format": FORMAT, "rows": len(converted), "kind": args.kind, "split": args.split,
                      "corpus_manifest_sha256": audit["manifest_sha256"], "out": str(args.out)}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
