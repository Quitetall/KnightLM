"""S0 board-grounding corpus: rules QA whose answers come only from the compiler.

One question per position, drawn from positions of an audited reasoning corpus (its train and
holdout splits, so the split rules and the leak audit are inherited unchanged). Every answer is
derived from compiler output: the cgt-full text (`knightlm cgt`, which refuses text it cannot
parse back), the packet's legal-move list and `knightlm corpus-facts`. No engine, no model.

Question types: side_to_move, piece_on_square, attackers_of, defenders_of, legal_moves_of,
in_check, captures, material. "Attack" and "defend" are the cgt/1 edges (docs/model/graph-text.md):
geometric, pins ignored. The prompt is the play prompt of the arm (`play` = T0 FEN packet,
`play-cgt` = T2 cgt-lite, `play-cgt-full`) plus one question line (model/prompt.py `qa_prompt`);
the completion is the play call whose `reasoning` field is the answer and whose `move` is the
move the human played in the source row (a legal move, so the play parser accepts the row).
Rows of the two arms share ids, questions and answers; only the prompt differs. `matched_sha256`
in each manifest hashes (id, type, question, answer) over all rows, so equality proves the match.

    python -m model.corpus.s0 build --parent DIR --output DIR --arm t0|t2 [--train N --holdout N]
    python -m model.corpus.pipeline audit DIR [--knightlm BIN] [--allow-rules-drift]
"""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from model import prompt as prompt_module
from model.corpus import pipeline

FORMAT = "knightlm-s0-rules-qa-v1"
GENERATOR_VERSION = "s0-rules-qa-v1"
TYPES = ("side_to_move", "piece_on_square", "attackers_of", "defenders_of", "legal_moves_of",
         "in_check", "captures", "material")
ARMS = {"t0": "play", "t2": "play-cgt", "t2-full": "play-cgt-full"}
NAMES = {"K": "king", "Q": "queen", "R": "rook", "B": "bishop", "N": "knight", "P": "pawn"}
POINTS = {"P": 1, "N": 3, "B": 3, "R": 5, "Q": 9, "K": 0}
ORDER = "KQRBNP"
ATTACK_NOTE = "(pins ignored)"
MAX_ANSWER_CHARS = 400
SQUARES = [f + r for r in "12345678" for f in "abcdefgh"]
RULES_CRATES = ("crates/core", "crates/notation")


def generator_sha256():
    return hashlib.sha256(Path(__file__).read_bytes()).hexdigest()


def h(*key):
    return int.from_bytes(hashlib.sha256("|".join(map(str, key)).encode()).digest()[:8], "big")


def pick(options, *key):
    return options[h(*key) % len(options)]


# --- rules revision ------------------------------------------------------------------------

def _git(repo, *args):
    done = subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True, timeout=60)
    if done.returncode:
        raise ValueError(f"git {' '.join(args)} failed: {done.stderr.strip()}")
    return done.stdout.strip()


def rules_revision(repo=None):
    """What the compiler's rules are: git tree ids of the rules-layer crates at HEAD, plus whether
    those crates have uncommitted changes. Unrelated commits do not change the tree ids."""
    repo = Path(repo or prompt_module._REPO)
    found = {"commit": _git(repo, "rev-parse", "HEAD")}
    for crate in RULES_CRATES:
        found[crate.split("/")[1] + "_tree"] = _git(repo, "rev-parse", f"HEAD:{crate}")
    found["dirty"] = bool(_git(repo, "status", "--porcelain", "--", *RULES_CRATES))
    return found


def check_rules(recorded, current):
    """Refuse a corpus whose recorded rules differ from the rules in use."""
    if current.get("dirty"):
        raise ValueError("the rules-layer crates have uncommitted changes; commit them before building or auditing")
    for key in ("core_tree", "notation_tree"):
        if recorded.get(key) != current.get(key):
            raise ValueError(
                f"rules revision differs ({key}: corpus {recorded.get(key)} vs in use {current.get(key)}); "
                "rebuild the corpus or pass --allow-rules-drift")


# --- facts from the compiler ---------------------------------------------------------------

def parse_cgt(text):
    """Sections of a cgt text: board (node->square), atk, def (target node -> sorted attackers), check."""
    lines = text.split("\n")
    board, edges, section, check = {}, {"atk": {}, "def": {}}, None, []
    for line in lines:
        if line in ("W:", "B:") or line.startswith(("W: ", "B: ")):
            for node in line[2:].split():
                board[node[1:]] = node[0]
            continue
        if line.endswith(":") and line[:-1] in ("moves", "atk", "def", "pin", "ksafe"):
            section = line[:-1]
            continue
        if line.startswith("hang:"):
            section = None
            continue
        if line.startswith("check:"):
            section = None
            check = [] if line.strip() == "check: -" else line.split()[1:]
            continue
        if section in edges and ": " in line:
            source, targets = line.split(": ", 1)
            for target in targets.split():
                edges[section].setdefault(target, []).append(source)
    return {"board": board, "atk": edges["atk"], "def": edges["def"], "check": check}


def node_key(node):
    piece = node[0]
    return (0 if piece.isupper() else 1, ORDER.index(piece.upper()), node[1], node[2])


def describe(node):
    colour = "white" if node[0].isupper() else "black"
    return f"{colour} {NAMES[node[0].upper()]} on {node[1:]}"


def piece_words(node):
    colour = "white" if node[0].isupper() else "black"
    return f"{colour} {NAMES[node[0].upper()]}"


def is_capture(board, turn_white, uci):
    src, dst = uci[:2], uci[2:4]
    if dst in board:
        return True
    return board.get(src, "").upper() == "P" and src[0] != dst[0]  # pawn changing file onto an empty square: en passant


# --- question builders ----------------------------------------------------------------------

def build_qa(packet, cgt, facts, target):
    """The (type, question, answer) for one position, or raise if the compiler facts disagree."""
    fen = packet["fen"]
    turn_white = fen.split()[1] == "w"
    nodes = cgt["board"]  # square -> piece letter (upper case white)
    key = packet["fen"]
    if bool(cgt["check"]) != bool(facts["in_check"]) or (facts["turn"] == "white") != turn_white:
        raise ValueError(f"cgt and corpus-facts disagree on side or check for {fen}")
    start = h(key, "type") % len(TYPES)
    for step in range(len(TYPES)):
        kind = TYPES[(start + step) % len(TYPES)]
        qa = _make(kind, key, packet, cgt, facts, nodes, turn_white)
        if qa:
            return qa
    raise ValueError(f"no question possible for {fen}")


def _make(kind, key, packet, cgt, facts, nodes, turn_white):
    side = "white" if turn_white else "black"
    if kind == "side_to_move":
        q = pick(["Which side is to move?", "Whose turn is it?"], key, "q")
        return {"type": kind, "question": q, "answer": f"{side.capitalize()} to move."}
    if kind == "in_check":
        q = pick(["Is the side to move in check?", "Is the king of the side to move in check?"], key, "q")
        yes = bool(cgt["check"])
        return {"type": kind, "question": q,
                "answer": f"Yes, {side} is in check." if yes else f"No, {side} is not in check."}
    if kind == "material":
        q = pick(["What is the material count in points (pawn 1, knight 3, bishop 3, rook 5, queen 9)?",
                  "Count each side's material in points (pawn 1, knight 3, bishop 3, rook 5, queen 9)."], key, "q")
        white = sum(POINTS[p.upper()] for p in nodes.values() if p.isupper())
        black = sum(POINTS[p.upper()] for p in nodes.values() if p.islower())
        if (white, black) != (facts["points"]["white"], facts["points"]["black"]):
            raise ValueError(f"material from cgt differs from corpus-facts for {packet['fen']}")
        return {"type": kind, "question": q, "answer": f"White {white}, black {black}."}
    if kind == "captures":
        q = pick(["Which legal moves capture a piece? Give the count and the moves.",
                  "List the capturing legal moves with their count."], key, "q")
        caps = sorted(m for m in packet["legal_moves"] if is_capture(nodes, turn_white, m))
        if len(caps) != facts["legal_capture_count"]:
            raise ValueError(f"capture count from rules differs from corpus-facts for {packet['fen']}")
        listing = f" ({' '.join(caps)})" if caps else ""
        return {"type": kind, "question": q, "answer": f"Captures: {len(caps)}{listing}"}
    if kind == "piece_on_square":
        want_piece = h(key, "occ") % 2 == 0
        pool = [s for s in SQUARES if (s in nodes) == want_piece] or SQUARES
        square = pick(pool, key, "sq")
        q = pick([f"What is on {square}?", f"Which piece, if any, stands on {square}?"], key, "q")
        if square in nodes:
            return {"type": kind, "question": q, "answer": f"There is a {describe_square(nodes, square)}."}
        return {"type": kind, "question": q, "answer": f"{square} is empty."}
    if kind in ("attackers_of", "defenders_of"):
        edges = cgt["atk"] if kind == "attackers_of" else cgt["def"]
        occupied = sorted(nodes)
        if not occupied:
            return None
        linked = [s for s in occupied if node_name(nodes, s) in edges]
        pool = linked if (linked and h(key, "link") % 4) else occupied
        square = pick(pool, key, "sq")
        target = node_name(nodes, square)
        sources = sorted(edges.get(target, []), key=node_key)
        verb = "attack" if kind == "attackers_of" else "defend"
        noun = "Attackers" if kind == "attackers_of" else "Defenders"
        q = pick([f"Which pieces {verb} the {describe(target)}? {ATTACK_NOTE}",
                  f"List the {noun.lower()} of the {describe(target)}. {ATTACK_NOTE}"], key, "q")
        listing = ", ".join(describe(s) for s in sources) if sources else "none"
        return {"type": kind, "question": q, "answer": f"{noun}: {listing}."}
    if kind == "legal_moves_of":
        by_from = {}
        for move in packet["legal_moves"]:
            by_from.setdefault(move[:2], []).append(move)
        mine = sorted(by_from)
        if not mine:
            return None
        square = pick(mine, key, "sq")
        moves = sorted(by_from[square])
        q = pick([f"List the legal moves of the {describe(node_name(nodes, square))}.",
                  f"What are the legal moves of the {describe(node_name(nodes, square))}?"], key, "q")
        return {"type": kind, "question": q, "answer": f"Legal moves: {' '.join(moves)}"}
    raise ValueError(kind)


def node_name(nodes, square):
    return nodes[square] + square


def describe_square(nodes, square):
    return describe(node_name(nodes, square))


# --- rows -----------------------------------------------------------------------------------

def compile_inputs(binary, rows, timeout=3600):
    """cgt-full text and corpus-facts for each row's position."""
    from model.corpus import reasoning

    facts = reasoning.compute_facts(binary, [{"fen": r["packet"]["fen"], "move": r["target"]} for r in rows], timeout)
    out = []
    for row, fact in zip(rows, facts, strict=True):
        if not fact.get("ok"):
            raise ValueError(f"corpus-facts failed for {row['packet']['fen']}: {fact.get('error')}")
        text = prompt_module.cgt_text(row["packet"], "full", binary)
        out.append((parse_cgt(text), fact))
    return out


def make_row(index, split, row, qa, arm, binary):
    kind = ARMS[arm]
    prompt = prompt_module.qa_prompt(kind, row["packet"], qa["question"], binary=binary)
    completion = prompt_module.format_play_completion(qa["answer"], row["target"])
    return {
        "id": f"s0-{split}-{index:06}", "family": row["family"], "game_id": row["game_id"],
        "packet": row["packet"], "ply": row["ply"], "position_key": row["position_key"],
        "source": row["source"], "target": row["target"], "utc_date": row["utc_date"],
        "utc_time": row["utc_time"], "qa": qa, "arm": arm, "prompt_kind": kind,
        "prompt": prompt, "completion": completion,
    }


def build_rows(binary, rows, split, arm):
    derived = compile_inputs(binary, rows)
    out = []
    for index, (row, (cgt, facts)) in enumerate(zip(rows, derived, strict=True)):
        qa = build_qa(row["packet"], cgt, facts, row["target"])
        out.append(make_row(index, split, row, qa, arm, binary))
    return out


def matched_sha256(train, holdout):
    digest = hashlib.sha256()
    for row in (*train, *holdout):
        digest.update(json.dumps([row["id"], row["qa"]["type"], row["qa"]["question"], row["qa"]["answer"]],
                                 separators=(",", ":")).encode() + b"\n")
    return digest.hexdigest()


# --- validation and audit ---------------------------------------------------------------------

def validate_rows(rows, source, arm):
    kind = ARMS[arm]
    seen = set()
    for row in rows:
        packet, qa = row["packet"], row["qa"]
        if (
            row["source"] != source
            or row["arm"] != arm
            or row["prompt_kind"] != kind
            or row["id"] in seen
            or packet["schema_version"] != 1
            or packet["ruleset"] != "standard-untimed-v1"
            or packet["history_complete"] is not True
            or row["position_key"] != " ".join(packet["fen"].split()[:4])
            or row["target"] not in packet["legal_moves"]
            or qa["type"] not in TYPES
            or not qa["answer"]
            or len(qa["answer"]) > MAX_ANSWER_CHARS
            or "\n" in qa["question"]
        ):
            raise ValueError(f"invalid S0 row {row.get('id')}")
        seen.add(row["id"])
        if row["completion"] != prompt_module.format_play_completion(qa["answer"], row["target"]):
            raise ValueError(f"completion of {row['id']} is not the play call carrying its answer")
        parsed = prompt_module.parse_play_completion(row["completion"])
        if parsed["reasoning"] != qa["answer"] or parsed["move"] != row["target"]:
            raise ValueError(f"completion of {row['id']} does not parse back to its answer")


def verify_rows(rows, binary, arm):
    """Re-derive every question, answer, prompt and completion from the compiler; refuse any difference."""
    derived = compile_inputs(binary, rows)
    for row, (cgt, facts) in zip(rows, derived, strict=True):
        qa = build_qa(row["packet"], cgt, facts, row["target"])
        if qa != row["qa"]:
            raise ValueError(f"{row['id']}: question or answer differs from the rules derivation")
        split = row["id"].split("-")[1]
        again = make_row(int(row["id"].rsplit("-", 1)[1]), split, row, qa, arm, binary)
        if again != row:
            raise ValueError(f"{row['id']}: prompt or completion differs from model.prompt")


def read_corpus(root, allow_rules_drift=False, current=None):
    """(train, holdout) after every manifest claim, the split audit and the rules-revision check."""
    root = Path(root)
    manifest = json.loads((root / pipeline.MANIFEST).read_text())
    if manifest["format"] != FORMAT:
        raise ValueError("not an S0 rules-QA corpus")
    if manifest["arm"] not in ARMS or manifest["prompt_kind"] != ARMS[manifest["arm"]]:
        raise ValueError("manifest arm and prompt kind disagree")
    if not allow_rules_drift:
        check_rules(manifest["rules"], current or rules_revision())
    listed = {s["file"] for name in pipeline.SPLITS for s in manifest["shards"][name]["files"]}
    present = {p.name for p in root.iterdir()} - {pipeline.MANIFEST}
    if listed != present:
        raise ValueError(f"shards not accounted for by the manifest: {sorted(listed ^ present)[:3]}")
    splits = []
    for name in pipeline.SPLITS:
        entry = manifest["shards"][name]
        rows = []
        for shard in entry["files"]:
            contents = (root / shard["file"]).read_bytes()
            if hashlib.sha256(contents).hexdigest() != shard["sha256"]:
                raise ValueError(f"{shard['file']} checksum mismatch")
            shard_rows = [json.loads(line) for line in contents.splitlines()]
            if not shard_rows or len(shard_rows) != shard["games"]:
                raise ValueError(f"{shard['file']} game count mismatch")
            rows.extend(shard_rows)
        if len(rows) != entry["games"]:
            raise ValueError(f"{name} game count mismatch")
        validate_rows(rows, manifest["source"]["file"], manifest["arm"])
        stamps = sorted(map(pipeline.timestamp, rows))
        if (pipeline.utc(stamps[0]), pipeline.utc(stamps[-1]), pipeline.months(rows)) != (
                entry["first_utc"], entry["last_utc"], entry["months"]):
            raise ValueError(f"{name} time range does not match the manifest")
        splits.append(rows)
    pipeline.audit_splits(*splits)
    if manifest["reserved_families"] != sorted({r["family"] for r in splits[1]}):
        raise ValueError("reserved families do not match the holdout")
    if manifest["matched_sha256"] != matched_sha256(*splits):
        raise ValueError("matched_sha256 does not match the rows")
    return splits


def type_counts(rows):
    counts = {t: 0 for t in TYPES}
    for row in rows:
        counts[row["qa"]["type"]] += 1
    return counts


# --- build -----------------------------------------------------------------------------------

def sha256_file(path):
    return pipeline.sha256_file(Path(path))


def write_corpus(output, train, holdout, arm, binary, current, source, parent_meta, split, shard_games):
    """Write audited S0 splits as shards plus a manifest; return the manifest."""
    pipeline.audit_splits(train, holdout)
    output.mkdir()
    manifest = {
        "format": FORMAT,
        "arm": arm,
        "prompt_kind": ARMS[arm],
        "source": source,
        "parent": parent_meta,
        "split": split,
        "rules": {**{k: v for k, v in current.items() if k != "dirty"},
                  "crates": list(RULES_CRATES),
                  "binary_sha256": sha256_file(binary)},
        "compiler_sha256": sha256_file(binary),
        "generator": {"version": GENERATOR_VERSION, "sha256": generator_sha256(),
                      "question_types": list(TYPES),
                      "oracle": "knightlm cgt --variant full, knightlm corpus-facts, packet legal_moves",
                      "attack_semantics": "cgt/1 atk/def edges: geometric, pins ignored",
                      "prompt": "model.prompt.qa_prompt(prompt_kind, packet, question)",
                      "completion": "one submit_move call: reasoning is the answer, move is the source row's played move"},
        "shard_games": shard_games,
        "reserved_families": sorted({r["family"] for r in holdout}),
        "matched_sha256": matched_sha256(train, holdout),
        "question_counts": {"train": type_counts(train), "holdout": type_counts(holdout)},
        "shards": {name: pipeline.write_shards(output, name, rows, shard_games)
                   for name, rows in zip(pipeline.SPLITS, (train, holdout), strict=True)},
    }
    (output / pipeline.MANIFEST).write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest


def build(args, parser):
    if args.arm not in ARMS:
        parser.error(f"--arm must be one of {sorted(ARMS)}")
    if args.output.exists():
        parser.error("output must be a new directory")
    binary = Path(args.knightlm).resolve()
    try:
        parent_train, parent_holdout = pipeline.read_corpus(args.parent)
        parent = json.loads((args.parent / pipeline.MANIFEST).read_text())
        current = rules_revision()
        check_rules(current, current)  # refuses uncommitted changes to the rules crates
        if len(parent_train) < args.train or len(parent_holdout) < args.holdout:
            raise ValueError("parent corpus has too few rows")
        train = build_rows(binary, parent_train[: args.train], "train", args.arm)
        holdout = build_rows(binary, parent_holdout[: args.holdout], "holdout", args.arm)
        parent_meta = {"corpus": args.parent.name, "format": parent["format"],
                       "manifest_sha256": sha256_file(args.parent / pipeline.MANIFEST),
                       "train_rows_used": args.train, "holdout_rows_used": args.holdout,
                       "selection": "first N rows of the parent's train and holdout shards, in shard order"}
        write_corpus(args.output, train, holdout, args.arm, binary, current, parent["source"], parent_meta,
                     parent["split"], args.shard_games)
        for rows in read_corpus(args.output):
            verify_rows(rows, binary, args.arm)
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        parser.exit(1, f"S0 build failed: {error}\n")
    print(f"Verified {len(train)} train and {len(holdout)} holdout S0 rows ({args.arm}) in {args.output}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    commands = parser.add_subparsers(dest="command", required=True)
    make = commands.add_parser("build")
    make.add_argument("--parent", type=Path, required=True, help="audited reasoning corpus to draw positions from")
    make.add_argument("--output", type=Path, required=True)
    make.add_argument("--arm", required=True, choices=sorted(ARMS))
    make.add_argument("--knightlm", type=Path, default=None)
    make.add_argument("--train", type=int, default=20000)
    make.add_argument("--holdout", type=int, default=1000)
    make.add_argument("--shard-games", type=int, default=10000)
    make.set_defaults(run=build)
    args = parser.parse_args()
    if args.knightlm is None:
        args.knightlm = prompt_module.knightlm_binary()
    args.run(args, parser)


if __name__ == "__main__":
    main()
