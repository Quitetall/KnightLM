"""Rules-derived reasoning text for the Run 1 reasoning corpus (generator `reasoning-v1`).

Every sentence is a board fact: side to move, check, legal-move, capture and check
counts, material, capturable and attacked pieces, and what the human move does.
The facts come from shakmaty on the notation crate's validated position, through
`knightlm corpus-facts` (same pattern as `compile_candidates` in pipeline.py).
No engine, no model, no search. The text never says a move is good or best: the
move is the one a human played. Phrasing varies by a hash of the position and
move, so the same input always gives the same text.

`model.eval.explain` is the check: the template words avoid its hypothetical
markers, and `test_reasoning.py` requires zero failed claims over many positions.
"""

import hashlib
import json
from pathlib import Path
import subprocess

GENERATOR_VERSION = "reasoning-v1"
MAX_CHARS = 400  # the submit_move `reasoning` bound (crates/contract/schemas/model_tools.json)
MAX_WORDS = 60
BACK_RANK = {"white": "1", "black": "8"}


def generator_sha256():
    return hashlib.sha256(Path(__file__).read_bytes()).hexdigest()


def pick(options, *key):
    digest = hashlib.sha256("|".join(map(str, key)).encode()).digest()
    return options[int.from_bytes(digest[:4], "big") % len(options)]


def plural(count, word):
    return f"{count} {word}" + ("" if count == 1 else "s")


def other(color):
    return "black" if color == "white" else "white"


def opening(facts, key):
    side = facts["turn"].capitalize()
    n = facts["legal_count"]
    legal = plural(n, "legal move")
    text = pick([f"{side} to move with {legal}.", f"{side} is to move; there {'is' if n == 1 else 'are'} {legal}.",
                 f"It is {side}'s turn with {legal}."], key, "open")
    if facts["in_check"]:
        text += pick([f" {side} is in check, with the king on {facts['king_square']}.",
                      f" The king on {facts['king_square']} is in check."], key, "check")
    return text


def material(facts, key):
    diff = facts["points"]["white"] - facts["points"]["black"]
    if diff == 0:
        return pick(["Material is equal.", "Material stays level.", "Material remains even."], key, "mat")
    leader, trailer = ("White", "Black") if diff > 0 else ("Black", "White")
    points = f"{abs(diff)} point" + ("" if abs(diff) == 1 else "s")
    return pick([f"{leader} is up {points}.", f"{leader} is ahead by {points}.", f"{trailer} is down {points}."],
                key, "mat")


def counts_sentence(facts, key):
    caps, checks = facts["legal_capture_count"], facts["legal_check_count"]
    return pick([f"{plural(caps, 'legal capture')} and {plural(checks, 'legal check')}.",
                 f"The legal moves hold {plural(caps, 'capture')} and {plural(checks, 'checking move')}.",
                 f"Legal captures number {caps}, and legal checks number {checks}."],
                key, "counts")


def captures_sentence(facts, key):
    """Capturable enemy pieces (most valuable first, at most two) as capture_target claims."""
    worth = {"queen": 9, "rook": 5, "bishop": 3, "knight": 3, "pawn": 1}
    found = sorted(facts["captures"], key=lambda c: (-worth[c["piece"]], c["square"]))[:2]
    if not found:
        return ""
    side = facts["turn"].capitalize()
    first = f"capture the {found[0]['piece']} on {found[0]['square']}"
    if len(found) == 1:
        return pick([f"{side} can {first}.", f"It is possible to {first}."], key, "caps")
    second = f"the {found[1]['piece']} on {found[1]['square']}"
    return pick([f"{side} can {first} or {second}.", f"It is possible to {first} or {second}."], key, "caps")


def undefended_sentence(facts, key):
    enemy = other(facts["turn"]).capitalize()
    loose = [c for c in facts["captures"] if not c["defended"]]
    if not loose:
        return ""
    worth = {"queen": 9, "rook": 5, "bishop": 3, "knight": 3, "pawn": 1}
    target = max(loose, key=lambda c: (worth[c["piece"]], c["square"]))
    return pick([f"{enemy}'s {target['piece']} on {target['square']} is undefended.",
                 f"The {enemy.lower()} {target['piece']} on {target['square']} has no defender."], key, "loose")


def attacked_sentence(facts, key):
    own = facts["own_attacked"]
    if not own:
        return ""
    worth = {"queen": 9, "rook": 5, "bishop": 3, "knight": 3, "pawn": 1}
    target = max(own, key=lambda c: (worth[c["piece"]], c["square"]))
    side = facts["turn"].capitalize()
    state = "defended" if target["defended"] else "undefended"
    return pick([f"{side}'s {target['piece']} on {target['square']} is attacked and {state}.",
                 f"The {facts['turn']} {target['piece']} on {target['square']} is attacked and {state}."],
                key, "attacked")


def move_sentence(facts, uci, key):
    move = facts["move"]
    lead = pick(["My move is", "I choose", "The move is"], key, "lead")
    if move["castle"]:
        text = f"{lead} {uci}, castling {move['castle']}"
    else:
        role, src, dst = move["role"], move["from"], move["to"]
        home = src[1] == BACK_RANK[facts["turn"]]
        if role in ("knight", "bishop") and home and dst[1] != BACK_RANK[facts["turn"]]:
            verb = "developing"
        elif role == "pawn" and not move["capture"] and not move["en_passant"]:
            verb = "pushing"
        else:
            verb = "moving"
        text = f"{lead} {uci}, {verb} the {role} on {src} to {dst}"
        if move["en_passant"]:  # the taken pawn is not on the destination square
            text += ", capturing a pawn en passant"
        elif move["capture"]:
            text += f", capturing the {move['capture']} on {dst}"
        if move["promotion"]:
            text += f", promoting to a {move['promotion']}"
    if move["gives_check"]:
        text += ", giving check"
    return text + "."


def reasoning_text(facts, uci):
    """The reasoning for one position and the human move `uci`, from `corpus-facts` output."""
    if not facts.get("ok") or facts.get("move") is None:
        raise ValueError("facts must be a successful corpus-facts reply that includes the move")
    key = (facts["fen"], uci)
    required = [opening(facts, key), material(facts, key), counts_sentence(facts, key)]
    tail = move_sentence(facts, uci, key)
    optional = [captures_sentence(facts, key), undefended_sentence(facts, key), attacked_sentence(facts, key)]
    optional = [s for s in optional if s]
    while True:
        text = " ".join([*required, *optional, tail])
        if len(text.split()) <= MAX_WORDS and len(text) <= MAX_CHARS:
            return text
        if not optional:
            raise ValueError(f"reasoning exceeds {MAX_WORDS} words / {MAX_CHARS} characters: {text}")
        optional.pop()  # drop the least important optional sentence (attacked, then undefended, ...)


def compute_facts(binary, items, timeout=3600):
    """Run `knightlm corpus-facts` on [{fen, move}] and return one reply per item, with the fen attached."""
    payload = "".join(json.dumps({"fen": i["fen"], "move": i["move"]}) + "\n" for i in items)
    result = subprocess.run([str(Path(binary).resolve()), "corpus-facts"], input=payload.encode(),
                            capture_output=True, timeout=timeout)
    if result.returncode:
        raise ValueError(result.stderr.decode().strip())
    replies = [json.loads(line) for line in result.stdout.splitlines()]
    if len(replies) != len(items):
        raise ValueError(f"corpus-facts returned {len(replies)} replies for {len(items)} positions")
    for item, reply in zip(items, replies, strict=True):
        reply["fen"] = item["fen"]
    return replies


def reasoning_for_rows(binary, rows, timeout=3600):
    """One reasoning string per corpus row (packet fen, played target)."""
    items = [{"fen": r["packet"]["fen"], "move": r["target"]} for r in rows]
    out = []
    for item, facts in zip(items, compute_facts(binary, items, timeout), strict=True):
        if not facts.get("ok"):
            raise ValueError(f"corpus-facts failed for {item['fen']}: {facts.get('error')}")
        out.append(reasoning_text(facts, item["move"]))
    return out
