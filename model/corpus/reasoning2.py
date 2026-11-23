"""Reasoning v2: short reasoning built only from compiler facts about the position and the chosen move.

Run 1's reasoning (`reasoning-v1`) was rules-derived but long and broad: 295 of 300 sampled reasonings
held a false claim, and a third of them said things no checker reads ("attacked and defended"). v2 keeps
only what the compiler states and an independent oracle can re-check:

  side to move (and check), material, 0-3 facts about attackers, defenders and hanging pieces of the
  position, and the chosen move (its piece, squares, capture, check). No count, ranking, plan or
  judgement; the text never says a move is good. "Attack" and "defend" are the geometric edges of
  docs/model/graph-text.md (pins ignored), the same as the `attackers_of` / `defenders_of` read tools.

Two variants share the facts and the move sentence:

  a  text only. One `submit_move` call whose `reasoning` states the facts.
  b  tool quoting. The completion first makes play-schema read calls (`attackers_of`, `defenders_of`,
     `gives_check`, `what_if`, `list_legal_moves`, contract schemas in model_tools.json), each followed
     by the compiler's result exactly as the harness renders it (`<tool_result>...</tool_result>`,
     produced by `knightlm corpus-reads` over `RuleSession`), then one `submit_move` whose reasoning
     quotes those results as `(tool arg: squares)`. The row's `segments` mark which spans the model writes
     (loss) and which the harness inserts (no loss).

Every row is checked three ways before it is written or accepted: `model.eval.explain` finds zero failed
claims (and at least one checkable claim); every attacker/defender list the compiler returned equals an
independent Python recomputation; and the whole row equals a re-derivation from the compiler.

    python -m model.corpus.reasoning2 build --parent DIR --output DIR --variant a|b [--train N --holdout N]
    python -m model.corpus.reasoning2 audit DIR [--knightlm BIN] [--allow-rules-drift]
"""

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

from model import prompt as prompt_module
from model.corpus import pipeline, reasoning, s0
from model.eval import explain

GENERATOR_VERSION = "reasoning-v2"
FORMAT = "knightlm-lichess-v2-reasoning-v2"
VARIANTS = {"a": "text", "b": "tools"}
# The prompt kind a variant is built with. Variant b allows read calls before submit_move, so its prompt must not say
# "exactly one tool call" (`play-reads`). The first b pilot was built with `play` and still validates as such.
DEFAULT_KIND = {"a": "play", "b": "play-reads"}
ALLOWED_KINDS = {"a": ("play",), "b": ("play", "play-reads")}
LIMITS = {"a": {"chars": 330, "words": 55}, "b": {"chars": 400, "words": 75}}  # b is bounded by the schema's 400
MAX_FACTS = 3
MAX_READS = 8  # the harness's per-turn read budget (READ_BUDGET)
LIST_LEGAL_MAX = 5  # list_legal_moves is quoted only when the answer is this short
READ_TOOLS = ("list_legal_moves", "what_if", "attackers_of", "defenders_of", "gives_check")
WORTH = {"queen": 9, "rook": 5, "bishop": 3, "knight": 3, "pawn": 1}
COLOR = {"w": "white", "b": "black"}
TOOL_OPEN, TOOL_CLOSE = prompt_module.TOOL_CALL_OPEN, prompt_module.TOOL_CALL_CLOSE
RESULT_OPEN, RESULT_CLOSE = "<tool_result>", "</tool_result>"
QUOTE = re.compile(r"\((?:attackers_of|defenders_of|gives_check|what_if|list_legal_moves)\b[^)]*\)")
BLOCK = re.compile(r"<tool_call>(.*?)</tool_call>|<tool_result>(.*?)</tool_result>", re.S)


def generator_sha256():
    return hashlib.sha256(Path(__file__).read_bytes()).hexdigest()


# --- independent attack oracle -----------------------------------------------------------------

def attackers_py(board, square):
    """Squares of the pieces that attack the piece on `square` (opposing colour), geometric, pins ignored.

    Written separately from the Rust compiler so a disagreement is a finding, not a tautology."""
    target = board.pieces[square]
    by = "b" if target[0] == "w" else "w"
    return _attackers(board, square, by)


def defenders_py(board, square):
    return _attackers(board, square, board.pieces[square][0])


def _attackers(board, square, by):
    file, rank = explain.FILES.index(square[0]), int(square[1]) - 1
    found = []

    def name(f, r):
        return explain.FILES[f] + str(r + 1)

    def inside(f, r):
        return 0 <= f < 8 and 0 <= r < 8

    for table, kinds in ((explain.KNIGHT, "N"), (explain.KING, "K")):
        for df, dr in table:
            if inside(file + df, rank + dr) and board.pieces.get(name(file + df, rank + dr)) == (by, kinds):
                found.append(name(file + df, rank + dr))
    back = -1 if by == "w" else 1
    for df in (-1, 1):
        if inside(file + df, rank + back) and board.pieces.get(name(file + df, rank + back)) == (by, "P"):
            found.append(name(file + df, rank + back))
    for dirs, kinds in ((explain.ROOK_DIRS, "RQ"), (explain.BISHOP_DIRS, "BQ")):
        for df, dr in dirs:
            step = 1
            while inside(file + df * step, rank + dr * step):
                at = name(file + df * step, rank + dr * step)
                piece = board.pieces.get(at)
                if piece:
                    if piece[0] == by and piece[1] in kinds:
                        found.append(at)
                    break
                step += 1
    return sorted(found)


# --- facts the reasoning may state ---------------------------------------------------------------

def plan_facts(facts):
    """At most MAX_FACTS facts, in priority order: the capture victim's defence, the most valuable own
    piece the opponent attacks, the most valuable undefended enemy piece the side to move can capture."""
    move = facts["move"]
    plan = []
    if move["capture"] and not move["en_passant"] and not move["castle"]:
        plan.append({"kind": "victim", "square": move["to"], "piece": move["capture"], "side": "enemy"})
    own = facts["own_attacked"]
    if own:
        target = max(own, key=lambda c: (WORTH[c["piece"]], c["square"]))
        plan.append({"kind": "hanging", "square": target["square"], "piece": target["piece"], "side": "own"})
    loose = [c for c in facts["captures"] if not c["defended"] and c["square"] != move["to"]]
    if loose:
        target = max(loose, key=lambda c: (WORTH[c["piece"]], c["square"]))
        plan.append({"kind": "loose", "square": target["square"], "piece": target["piece"], "side": "enemy"})
    return plan[:MAX_FACTS]


def fact_calls(fact):
    sq = fact["square"]
    if fact["kind"] == "hanging":
        return [("attackers_of", {"square": sq}), ("defenders_of", {"square": sq})]
    return [("defenders_of", {"square": sq})]


def move_calls(facts, uci):
    calls = []
    if facts["legal_count"] <= LIST_LEGAL_MAX:
        calls.append(("list_legal_moves", {}))
    calls.append(("gives_check", {"move": uci}))
    if facts["move"]["gives_check"]:
        calls.append(("what_if", {"moves": [uci]}))
    return calls


def call_list(facts, uci, plan):
    """The read calls of variant b, deduplicated, at most MAX_READS."""
    calls = []
    for call in [*move_calls(facts, uci), *(c for f in plan for c in fact_calls(f))]:
        if call not in calls:
            calls.append(call)
    if len(calls) > MAX_READS:
        raise ValueError("more read calls than the harness budget")
    return calls


# --- compiler reads ---------------------------------------------------------------------------------

def compute_reads(binary, items, timeout=3600):
    """Run `knightlm corpus-reads` on [{fen, calls: [(name, arguments)]}]; return the rendered results."""
    payload = "".join(json.dumps({"fen": i["fen"], "calls": [{"name": n, "arguments": a} for n, a in i["calls"]]},
                                 separators=(",", ":")) + "\n" for i in items)
    done = subprocess.run([str(Path(binary).resolve()), "corpus-reads"], input=payload.encode(),
                          capture_output=True, timeout=timeout)
    if done.returncode:
        raise ValueError(done.stderr.decode().strip())
    replies = [json.loads(line) for line in done.stdout.splitlines()]
    if len(replies) != len(items):
        raise ValueError(f"corpus-reads returned {len(replies)} replies for {len(items)} positions")
    out = []
    for item, reply in zip(items, replies, strict=True):
        if not reply.get("ok") or len(reply["rendered"]) != len(item["calls"]):
            raise ValueError(f"corpus-reads failed for {item['fen']}: {reply.get('error')}")
        out.append(reply["rendered"])
    return out


def unwrap(rendered):
    """The reply object inside one `<tool_result>` string."""
    if not (rendered.startswith(RESULT_OPEN) and rendered.endswith(RESULT_CLOSE)):
        raise ValueError("not a tool result")
    reply = json.loads(rendered[len(RESULT_OPEN):-len(RESULT_CLOSE)])
    if not reply["ok"]:
        raise ValueError(f"tool call failed: {reply['error']}")
    return reply["result"]


# --- sentences ---------------------------------------------------------------------------------------

def other(color):
    return "black" if color == "white" else "white"


def pieces_phrase(board, squares):
    names = [f"the {explain.describe(board.pieces[s]).split()[-1]} on {s}" for s in squares]
    if len(names) > 3:
        return f"{len(names)} pieces"
    return names[0] if len(names) == 1 else ", ".join(names[:-1]) + " and " + names[-1]


def call_key(name, arguments):
    return (name, arguments.get("square") or arguments.get("move") or ",".join(arguments.get("moves", [])))


def quote_for(name, arguments, reply):
    """The exact quote of one read result inside a variant-b reasoning, from the compiler's reply."""
    if name in ("attackers_of", "defenders_of"):
        return f"({name} {arguments['square']}: {', '.join(reply['squares']) or 'none'})"
    if name == "gives_check":
        return f"({name} {arguments['move']}: {'true' if reply['gives_check'] else 'false'})"
    if name == "what_if":
        return f"({name} {' '.join(arguments['moves'])}: in_check {'true' if reply['in_check'] else 'false'})"
    return f"({name}: {', '.join(reply['moves'])})"


def tool_quote(results, name, arguments, quoting):
    return " " + quote_for(name, arguments, results[call_key(name, arguments)]) if quoting else ""


def fact_sentence(fact, board, results, side, quoting):
    sq = fact["square"]
    color = side if fact["side"] == "own" else other(side)
    subject = f"The {color} {fact['piece']} on {sq}"
    defenders = results[("defenders_of", sq)]["squares"]
    q_def = tool_quote(results, "defenders_of", {"square": sq}, quoting)
    defended = f"defended by {pieces_phrase(board, defenders)}" if defenders else "undefended"
    if fact["kind"] == "hanging":
        attackers = results[("attackers_of", sq)]["squares"]
        q_att = tool_quote(results, "attackers_of", {"square": sq}, quoting)
        return f"{subject} is attacked by {pieces_phrase(board, attackers)}{q_att} and {defended}{q_def}."
    return f"{subject} is {defended}{q_def}."


def move_sentence(facts, uci, results, quoting):
    move = facts["move"]
    if move["castle"]:
        text = f"I choose {uci}, castling {move['castle']}"
    else:
        role, src, dst = move["role"], move["from"], move["to"]
        text = f"I choose {uci}, moving the {role} on {src} to {dst}"
        if move["en_passant"]:
            text += ", capturing a pawn en passant"
        elif move["capture"]:
            text += f", capturing the {move['capture']} on {dst}"
        if move["promotion"]:
            text += f", promoting to a {move['promotion']}"
    if move["gives_check"]:
        text += ", giving check"
    text += tool_quote(results, "gives_check", {"move": uci}, quoting)
    if move["gives_check"]:
        text += tool_quote(results, "what_if", {"moves": [uci]}, quoting)
    return text + "."


def opening(facts, results, quoting):
    side = facts["turn"].capitalize()
    listing = ""
    if quoting and facts["legal_count"] <= LIST_LEGAL_MAX:
        listing = " " + quote_for("list_legal_moves", {}, results[("list_legal_moves", "")])
    return f"{side} to move{listing}." + (f" {side} is in check." if facts["in_check"] else "")


def build_text(facts, uci, plan, results, variant):
    """The reasoning for a fixed plan. `results` maps call_key(...) to the compiler's reply."""
    from model.eval.explain import Board

    board = Board(facts["fen"])
    quoting = variant == "b"
    sentences = [opening(facts, results, quoting), reasoning.material(facts, ()), *(
        fact_sentence(f, board, results, facts["turn"], quoting) for f in plan)]
    sentences.append(move_sentence(facts, uci, results, quoting))
    return " ".join(sentences)


def fits(text, variant):
    limit = LIMITS[variant]
    return len(text) <= limit["chars"] and len(text.split()) <= limit["words"]


# --- one row: plan, reads, text ----------------------------------------------------------------------

def result_map(calls, rendered, facts, board):
    """{call_key: reply} for every call, with attackers/defenders checked against the independent oracle."""
    out = {}
    for (name, args), text in zip(calls, rendered, strict=True):
        reply = unwrap(text)
        if name in ("attackers_of", "defenders_of"):
            sq = args["square"]
            want = (attackers_py if name == "attackers_of" else defenders_py)(board, sq)
            if reply["squares"] != want:
                raise ValueError(f"{name} {sq}: compiler {reply['squares']} vs independent {want} in {facts['fen']}")
        elif name == "gives_check" and reply["gives_check"] != facts["move"]["gives_check"]:
            raise ValueError(f"gives_check disagrees with corpus-facts in {facts['fen']}")
        elif name == "list_legal_moves" and sorted(reply["moves"]) != sorted(facts["legal_moves"]):
            raise ValueError(f"list_legal_moves disagrees with corpus-facts in {facts['fen']}")
        out[call_key(name, args)] = reply
    return out


def derive(binary, facts_list, ucis, variant):
    """(plan, calls, rendered, results, text) per position. Facts are dropped from the end until the text fits;
    variant b then reads exactly the calls of the kept plan, so the `call` numbers in the results are 1..n."""
    from model.eval.explain import Board

    plans, first_calls = [], []
    for facts in facts_list:
        plan = plan_facts(facts)
        plans.append(plan)
        first_calls.append(call_list(facts, ucis[len(first_calls)], plan))
    probe = compute_reads(binary, [{"fen": f["fen"], "calls": c} for f, c in zip(facts_list, first_calls, strict=True)])
    kept = []
    for facts, uci, plan, calls, rendered in zip(facts_list, ucis, plans, first_calls, probe, strict=True):
        board = Board(facts["fen"])
        results = result_map(calls, rendered, facts, board)
        while True:
            text = build_text(facts, uci, plan, results, variant)
            if fits(text, variant):
                break
            if not plan:
                raise ValueError(f"reasoning exceeds the {variant} limits with no fact: {text}")
            plan = plan[:-1]
        kept.append((plan, text))
    if variant == "a":
        return [(plan, [], [], text) for plan, text in kept]
    final_calls = [call_list(f, u, plan) for f, u, (plan, _) in zip(facts_list, ucis, kept, strict=True)]
    final = compute_reads(binary, [{"fen": f["fen"], "calls": c} for f, c in zip(facts_list, final_calls, strict=True)])
    out = []
    for facts, uci, (plan, text), calls, rendered in zip(facts_list, ucis, kept, final_calls, final, strict=True):
        results = result_map(calls, rendered, facts, Board(facts["fen"]))
        again = build_text(facts, uci, plan, results, variant)
        if again != text:
            raise ValueError("text changed between the probe and the final reads")
        out.append((plan, calls, rendered, text))
    return out


# --- completions -----------------------------------------------------------------------------------------

def call_text(name, arguments):
    return TOOL_OPEN + json.dumps({"name": name, "arguments": arguments}, separators=(",", ":"), ensure_ascii=False) + TOOL_CLOSE


def transcript(calls, rendered, text, uci):
    """(completion, segments) of variant b: call, result, ... then submit_move. Results are not loss."""
    segments = []
    for (name, arguments), result in zip(calls, rendered, strict=True):
        segments.append({"text": call_text(name, arguments) + "\n", "loss": True})
        segments.append({"text": result + "\n", "loss": False})
    segments.append({"text": prompt_module.format_play_completion(text, uci), "loss": True})
    return "".join(s["text"] for s in segments), segments


def _check_arguments(name, arguments):
    schema = next(t for t in prompt_module.load_tool_schemas()["modes"]["play"] if t["name"] == name)["parameters"]
    if set(arguments) - set(schema["properties"]) or any(k not in arguments for k in schema.get("required", [])):
        raise ValueError(f"{name}: arguments do not match the schema")
    for key, value in arguments.items():
        spec = schema["properties"][key]
        if spec["type"] == "string" and not isinstance(value, str):
            raise ValueError(f"{name}.{key} must be a string")
        if spec["type"] == "array" and not (isinstance(value, list) and spec["minItems"] <= len(value) <= spec["maxItems"]
                                            and all(isinstance(v, str) for v in value)):
            raise ValueError(f"{name}.{key} must be a list of 1 to 8 strings")


def parse_transcript(text):
    """A variant-b completion -> ([(name, arguments, result_text)], {"reasoning", "move"}); ValueError otherwise.

    Shape: read call, its result, ... then exactly one final submit_move and nothing else."""
    parts, position = [], 0
    for found in BLOCK.finditer(text):
        if text[position:found.start()].strip():
            raise ValueError("extra text outside calls and results")
        position = found.end()
        parts.append(("call", found.group(1)) if found.group(1) is not None else ("result", found.group(0)))
    if text[position:].strip() or not parts or parts[-1][0] != "call":
        raise ValueError("a transcript ends with one submit_move call")
    reads = []
    for index, (kind, body) in enumerate(parts[:-1]):
        if index % 2 == 0:
            if kind != "call":
                raise ValueError("a result without its call")
            try:
                call = json.loads(body)
            except ValueError:
                raise ValueError("malformed read call") from None
            if not isinstance(call, dict) or sorted(call) != ["arguments", "name"] or call["name"] not in READ_TOOLS \
                    or not isinstance(call["arguments"], dict):
                raise ValueError("not a play-schema read call")
            _check_arguments(call["name"], call["arguments"])
            reads.append([call["name"], call["arguments"], None])
        else:
            if kind != "result":
                raise ValueError("a call without its result")
            reads[-1][2] = body
    if len(parts) % 2 == 0 or len(reads) > MAX_READS:
        raise ValueError("calls and results do not alternate, or too many reads")
    final = prompt_module.parse_play_completion(TOOL_OPEN + parts[-1][1] + TOOL_CLOSE)
    return [tuple(r) for r in reads], final


def check_quotes(text, reads, require=True):
    """Every `(tool ...)` quote in a reasoning equals the compiler's result of a call in the transcript, and
    (with `require`) every call's result is quoted. Raises ValueError on a misquote."""
    expected = [quote_for(name, args, unwrap(result)) for name, args, result in reads]
    for found in QUOTE.findall(text):
        if found not in expected:
            raise ValueError(f"quote {found!r} is not a result of any call in the transcript")
    if require:
        for want in expected:
            if want not in text:
                raise ValueError(f"result {want!r} is not quoted")


# --- rows ---------------------------------------------------------------------------------------------------

def claim_stats(row_fen, legal, text, uci):
    result = explain.check_explanation(row_fen, legal, text, uci)
    return {"checkable": result["checkable_total"], "failed": result["failed_total"],
            "by_type": {k: v["checkable"] for k, v in result["claim_types"].items() if v["checkable"]},
            "unchecked": sum(result["unchecked"].values()), "failures": result["failures"]}


def make_row(index, split, row, variant, plan, calls, rendered, text, kind=None):
    kind = kind or DEFAULT_KIND[variant]
    uci = row["target"]
    if variant == "b":
        completion, segments = transcript(calls, rendered, text, uci)
    else:
        completion, segments = prompt_module.format_play_completion(text, uci), None
    claims = claim_stats(row["packet"]["fen"], row["packet"]["legal_moves"], text, uci)
    out = {
        "id": f"r2{variant}-{split}-{index:06}", "family": row["family"], "game_id": row["game_id"],
        "packet": row["packet"], "ply": row["ply"], "position_key": row["position_key"],
        "source": row["source"], "target": uci, "utc_date": row["utc_date"], "utc_time": row["utc_time"],
        "variant": variant, "prompt_kind": kind, "reasoning": text, "move": uci,
        "facts": [{k: f[k] for k in ("kind", "square", "piece")} for f in plan],
        "reads": [{"name": n, "arguments": a} for n, a in calls],
        "claims": {k: claims[k] for k in ("checkable", "failed", "by_type", "unchecked")},
        "prompt": prompt_module.play_prompt(kind, row["packet"]), "completion": completion,
    }
    if segments is not None:
        out["segments"] = segments
    if claims["failed"] or not claims["checkable"]:
        raise ValueError(f"{out['id']}: {claims['failed']} failed of {claims['checkable']} claims: {claims['failures'][:2]}")
    return out


def build_rows(binary, rows, split, variant, kind=None):
    items = [{"fen": r["packet"]["fen"], "move": r["target"]} for r in rows]
    facts_list = reasoning.compute_facts(binary, items)
    for fact, item in zip(facts_list, items, strict=True):
        if not fact.get("ok"):
            raise ValueError(f"corpus-facts failed for {item['fen']}: {fact.get('error')}")
    derived = derive(binary, facts_list, [i["move"] for i in items], variant)
    return [make_row(i, split, row, variant, *parts, kind=kind) for i, (row, parts) in enumerate(zip(rows, derived, strict=True))]


# --- validation and audit ---------------------------------------------------------------------------------------

def validate_rows(rows, source, variant):
    seen = set()
    for row in rows:
        packet = row["packet"]
        if (
            row["source"] != source or row["variant"] != variant or row["id"] in seen
            or packet["schema_version"] != 1 or packet["ruleset"] != "standard-untimed-v1"
            or packet["history_complete"] is not True
            or row["position_key"] != " ".join(packet["fen"].split()[:4])
            or row["target"] not in packet["legal_moves"] or row["move"] != row["target"]
            or row.get("prompt_kind") not in ALLOWED_KINDS[variant]
            or row["prompt"] != prompt_module.play_prompt(row["prompt_kind"], packet)
            or row["claims"]["failed"] or not row["claims"]["checkable"]
            or len(row["reasoning"]) > LIMITS[variant]["chars"] or "\n" in row["reasoning"]
        ):
            raise ValueError(f"invalid reasoning-v2 row {row.get('id')}")
        seen.add(row["id"])
        if variant == "a":
            parsed = prompt_module.parse_play_completion(row["completion"])
            reads, final = [], parsed
        else:
            reads, final = parse_transcript(row["completion"])
            if "".join(s["text"] for s in row["segments"]) != row["completion"] or len(reads) != len(row["reads"]):
                raise ValueError(f"{row['id']}: segments or reads do not match the completion")
            check_quotes(row["reasoning"], reads)
        if final != {"reasoning": row["reasoning"], "move": row["move"]}:
            raise ValueError(f"{row['id']}: completion does not carry the row's reasoning and move")


def verify_rows(rows, binary, variant):
    """Re-derive every row from the compiler; refuse any difference or any failed claim."""
    items = [{"fen": r["packet"]["fen"], "move": r["target"]} for r in rows]
    facts_list = reasoning.compute_facts(binary, items)
    derived = derive(binary, facts_list, [i["move"] for i in items], variant)
    for row, parts in zip(rows, derived, strict=True):
        split, index = row["id"].split("-")[1], int(row["id"].rsplit("-", 1)[1])
        if make_row(index, split, row, variant, *parts, kind=row["prompt_kind"]) != row:
            raise ValueError(f"{row['id']}: differs from the compiler derivation")


def matched_sha256(train, holdout):
    digest = hashlib.sha256()
    for row in (*train, *holdout):
        digest.update(json.dumps([row["id"][4:], row["position_key"], row["target"]], separators=(",", ":")).encode() + b"\n")
    return digest.hexdigest()


def read_corpus(root, allow_rules_drift=False, current=None):
    """(train, holdout) after every manifest claim, the split audit and the rules-revision check."""
    root = Path(root)
    manifest = json.loads((root / pipeline.MANIFEST).read_text())
    if manifest["format"] != FORMAT:
        raise ValueError("not a reasoning-v2 corpus")
    variant = manifest["variant"]
    if variant not in VARIANTS:
        raise ValueError("unknown reasoning-v2 variant")
    if not allow_rules_drift:
        s0.check_rules(manifest["rules"], current or s0.rules_revision())
    listed = {s["file"] for name in pipeline.SPLITS for s in manifest["shards"][name]["files"]}
    if listed != {p.name for p in root.iterdir()} - {pipeline.MANIFEST}:
        raise ValueError("shards not accounted for by the manifest")
    splits = []
    for name in pipeline.SPLITS:
        entry, rows = manifest["shards"][name], []
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
        validate_rows(rows, manifest["source"]["file"], variant)
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


def claim_totals(rows):
    totals = {"rows": len(rows), "checkable_claims": 0, "failed_claims": 0, "rows_with_a_failed_claim": 0,
              "by_type": {}, "unchecked_claims": 0}
    for row in rows:
        claims = row["claims"]
        totals["checkable_claims"] += claims["checkable"]
        totals["failed_claims"] += claims["failed"]
        totals["rows_with_a_failed_claim"] += claims["failed"] > 0
        totals["unchecked_claims"] += claims["unchecked"]
        for kind, n in claims["by_type"].items():
            totals["by_type"][kind] = totals["by_type"].get(kind, 0) + n
    return totals


def write_corpus(output, train, holdout, variant, binary, current, parent, parent_meta, shard_games):
    pipeline.audit_splits(train, holdout)
    output.mkdir()
    manifest = {
        "format": FORMAT,
        "variant": variant,
        "variant_name": VARIANTS[variant],
        "prompt_kind": DEFAULT_KIND[variant],
        "source": parent["source"],
        "parent": parent_meta,
        "split": parent["split"],
        "rules": {**{k: v for k, v in current.items() if k != "dirty"}, "crates": list(s0.RULES_CRATES),
                  "binary_sha256": pipeline.sha256_file(binary)},
        "compiler_sha256": pipeline.sha256_file(binary),
        "generator": {
            "version": GENERATOR_VERSION, "sha256": generator_sha256(), "limits": LIMITS[variant],
            "oracle": "knightlm corpus-facts, knightlm corpus-reads (RuleSession), model.eval.explain, independent attacker recomputation",
            "claims": "side to move, check, material, attackers/defenders/undefended facts, the chosen move; no counts, no judgement",
            "attack_semantics": "geometric (pins ignored), as the attackers_of / defenders_of read tools",
            "prompt": "model.prompt.play_prompt(prompt_kind, packet): `play` for a, `play-reads` for b (the instruction "
                      "allows read calls before submit_move)",
            "completion": "one submit_move call" if variant == "a" else
            "read calls each followed by its harness-rendered result, then one submit_move; segments mark loss spans",
            "tools": sorted(READ_TOOLS) if variant == "b" else [],
        },
        "shard_games": shard_games,
        "reserved_families": sorted({r["family"] for r in holdout}),
        "matched_sha256": matched_sha256(train, holdout),
        "claims": {"train": claim_totals(train), "holdout": claim_totals(holdout)},
        "shards": {name: pipeline.write_shards(output, name, rows, shard_games)
                   for name, rows in zip(pipeline.SPLITS, (train, holdout), strict=True)},
    }
    (output / pipeline.MANIFEST).write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest


def build(args, parser):
    if args.output.exists():
        parser.error("output must be a new directory")
    binary = Path(args.knightlm).resolve()
    try:
        parent_train, parent_holdout = pipeline.read_corpus(args.parent)
        parent = json.loads((args.parent / pipeline.MANIFEST).read_text())
        current = s0.rules_revision()
        s0.check_rules(current, current)
        if len(parent_train) < args.train or len(parent_holdout) < args.holdout:
            raise ValueError("parent corpus has too few rows")
        train = build_rows(binary, parent_train[: args.train], "train", args.variant)
        holdout = build_rows(binary, parent_holdout[: args.holdout], "holdout", args.variant)
        parent_meta = {"corpus": args.parent.name, "format": parent["format"],
                       "manifest_sha256": pipeline.sha256_file(args.parent / pipeline.MANIFEST),
                       "train_rows_used": args.train, "holdout_rows_used": args.holdout,
                       "selection": "first N rows of the parent's train and holdout shards, in shard order"}
        write_corpus(args.output, train, holdout, args.variant, binary, current, parent, parent_meta, args.shard_games)
        for rows in read_corpus(args.output):
            verify_rows(rows, binary, args.variant)
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        parser.exit(1, f"reasoning-v2 build failed: {error}\n")
    print(f"Verified {len(train)} train and {len(holdout)} holdout reasoning-v2{args.variant} rows in {args.output}", flush=True)


def audit(args, parser):
    try:
        train, holdout = read_corpus(args.corpus, args.allow_rules_drift)
        manifest = json.loads((args.corpus / pipeline.MANIFEST).read_text())
        if args.knightlm:
            if pipeline.sha256_file(args.knightlm) != manifest["rules"]["binary_sha256"] and not args.allow_rules_drift:
                raise ValueError("--knightlm differs from the binary that built the corpus; pass --allow-rules-drift")
            for split in (train, holdout):
                verify_rows(split, args.knightlm.resolve(), manifest["variant"])
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        parser.exit(1, f"reasoning-v2 audit failed: {error}\n")
    print(f"Accepted {len(train)} train and {len(holdout)} holdout rows in {args.corpus}: zero failed claims", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    commands = parser.add_subparsers(dest="command", required=True)
    make = commands.add_parser("build")
    make.add_argument("--parent", type=Path, required=True)
    make.add_argument("--output", type=Path, required=True)
    make.add_argument("--variant", required=True, choices=sorted(VARIANTS))
    make.add_argument("--knightlm", type=Path, default=None)
    make.add_argument("--train", type=int, default=20000)
    make.add_argument("--holdout", type=int, default=1000)
    make.add_argument("--shard-games", type=int, default=10000)
    make.set_defaults(run=build)
    check = commands.add_parser("audit")
    check.add_argument("corpus", type=Path)
    check.add_argument("--knightlm", type=Path, default=None)
    check.add_argument("--allow-rules-drift", action="store_true")
    check.set_defaults(run=audit)
    args = parser.parse_args()
    if args.command == "build" and args.knightlm is None:
        args.knightlm = prompt_module.knightlm_binary()
    args.run(args, parser)


if __name__ == "__main__":
    main()
