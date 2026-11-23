"""ctok/1: exact round trips against the compiler's own rendering, one token per legal
move, distinct positions stay distinct, corrupted sequences are refused.

Positions come from the Rust compiler (`knightlm playouts`: deterministic random
playouts and the cgt fixture FENs, with legal moves from shakmaty) and from the
Run 1 holdout when it is on disk; CGT text is the compiler's `knightlm cgt`.
Needs the built `knightlm` binary (cargo build -p knightlm-cli); skipped without it.
"""

import json
import random
import re
import subprocess
import unittest
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from model import prompt
from model.tokenizer import cgt, moves, play, position, vocab
from model.tokenizer.errors import TokenizerError

ROOT = Path(__file__).resolve().parents[2]
BINARY = next((p for p in (ROOT / "target/debug/knightlm", ROOT / "target/release/knightlm") if p.exists()), None)
HOLDOUT = Path("/mnt/4tb/artifacts/KnightLM/expire/corpus/run1-reasoning-2026-09-a/holdout-00000.jsonl")
# The FEN fixtures of crates/notation/tests/cgt.rs (promotion, en passant, mate, stalemate, castling, Chess960).
FIXTURE_FENS = [
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
    "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
    "rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 3",
    "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3",
    "7k/5Q2/6K1/8/8/8/8/8 b - - 0 1",
    "4k3/P7/8/8/8/8/8/4K3 w - - 0 1",
    "4k3/8/8/8/1b6/8/3r4/4K3 w - - 0 1",
]
CHESS960 = "bqnb1rkr/pp3ppp/3ppn2/2p5/5P2/P2P4/NPP1P1PP/BQ1BNRKR w HFhf - 2 9"
_cache = {}


def _run(*args, stdin=None):
    return subprocess.run([str(BINARY), *args], input=stdin, check=True, capture_output=True, text=True,
                          timeout=120).stdout


def records():
    """Distinct compiler records {fen, legal_moves}: playouts, fixtures and holdout packets."""
    if "records" not in _cache:
        rows = _run("playouts", "--games", "30", "--plies", "110", "--seed", "7", "--stdin-fens",
                    stdin="\n".join(FIXTURE_FENS) + "\n").splitlines()
        by_fen = {json.loads(r)["fen"]: json.loads(r) for r in rows}
        if HOLDOUT.exists():
            with open(HOLDOUT) as handle:
                for line in handle:
                    if len(by_fen) >= 4000:
                        break
                    packet = json.loads(line)["packet"]
                    by_fen.setdefault(packet["fen"], {"fen": packet["fen"], "legal_moves": packet["legal_moves"]})
        _cache["records"] = list(by_fen.values())
    return _cache["records"]


def cgt_text(fen, variant, rep=1):
    cmd = ["cgt", "--fen", fen, "--variant", variant] + (["--rep", str(rep)] if rep is not None else [])
    return _run(*cmd).rstrip("\n")


def cgt_texts(variant, rep=1, limit=None):
    key = ("cgt", variant, rep, limit)
    if key not in _cache:
        recs = records()[:limit]
        with ThreadPoolExecutor(8) as pool:
            _cache[key] = list(pool.map(lambda r: cgt_text(r["fen"], variant, rep), recs))
    return _cache[key]


class Vocabulary(unittest.TestCase):
    def test_shape(self):
        self.assertEqual(len(set(vocab.TOKENS)), vocab.SIZE)
        self.assertEqual(vocab.SIZE % 64, 0)
        self.assertGreaterEqual(len(range(*vocab.RANGES["reserved"])), vocab.RESERVED_MIN)
        self.assertEqual(vocab.RANGES["reserved"][1], vocab.SIZE)
        self.assertEqual(len(range(*vocab.RANGES["square"])), 64)

    def test_move_space_is_derived_not_listed(self):
        uci = moves.UCI
        promos = [m for m in uci if len(m) == 5]
        self.assertEqual((len(uci), len(promos), len(uci) - len(promos)), (1968, 176, 1792))
        self.assertEqual(len(set(uci)), len(uci))
        self.assertIn("e1g1", uci)  # castling is a king move on a rook line
        self.assertIn("e7e8q", uci)
        self.assertNotIn("e7e8", uci[len(uci) - 176:])
        self.assertNotIn("e2e2", uci)
        self.assertNotIn("a1b3q", uci)

    def test_manifest_is_pinned(self):
        m = vocab.manifest()
        self.assertEqual(m["spec_version"], "ctok/1")
        self.assertEqual(m["cgt_version"], "cgt/1")
        self.assertEqual(m["vocab_size"], 2496)
        # Changing any token, its order or the move space changes this hash: bump the spec version.
        self.assertEqual(m["vocab_sha256"], vocab.vocab_sha256())
        self.assertEqual(m["vocab_sha256"][:16], PINNED_HASH_PREFIX)

    def test_rules_revision_names_the_compiler_shakmaty(self):
        lock = (ROOT / "Cargo.lock").read_text()
        version = re.search(r'name = "shakmaty"\nversion = "([^"]+)"', lock).group(1)
        self.assertIn(f"shakmaty-{version}", vocab.RULES_REVISION)


PINNED_HASH_PREFIX = "7f24af530c7525d7"


@unittest.skipIf(BINARY is None, "knightlm binary not built")
class Moves(unittest.TestCase):
    def test_every_legal_move_has_exactly_one_token(self):
        seen = set()
        recs = records()
        self.assertGreaterEqual(len(recs), 3000)
        for rec in recs:
            for uci in rec["legal_moves"]:
                tid = moves.move_to_id(uci)
                self.assertEqual(moves.id_to_move(tid), uci)
                self.assertEqual(vocab.TOKENS[tid], f"<m:{uci}>")
                seen.add(uci)
        self.assertGreater(len(seen), 1500)
        # one token per move and one move per token over the whole space
        ids = [moves.move_to_id(u) for u in moves.UCI]
        self.assertEqual(ids, list(range(*vocab.RANGES["move"])))

    def test_fixture_covers_every_promotion_letter_and_castling(self):
        seen = {u for rec in records() for u in rec["legal_moves"]}
        for letter in "qrbn":
            self.assertTrue(any(u.endswith(letter) and len(u) == 5 for u in seen), letter)
        self.assertTrue({"e1g1", "e1c1"} & seen or {"e8g8", "e8c8"} & seen)

    def test_refuses_what_is_not_a_move_token(self):
        for bad in ("e2e9", "e2e2", "a1b3q", "e7e8k", "", "e2e4 ", "E2E4", None, 5):
            with self.assertRaises(TokenizerError):
                moves.move_to_id(bad)
        for bad in (-1, 0, vocab.RANGES["move"][1], vocab.SIZE, True, "x"):
            with self.assertRaises(TokenizerError):
                moves.id_to_move(bad)


@unittest.skipIf(BINARY is None, "knightlm binary not built")
class PositionTokens(unittest.TestCase):
    def test_exact_round_trip_thousands_of_positions(self):
        recs = records()
        for rec in recs:
            for with_moves in (True, False):
                r = {"fen": rec["fen"], "repetition_count": 1}
                if with_moves:
                    r["legal_moves"] = rec["legal_moves"]
                self.assertEqual(position.decode(position.encode(r)), r)
        r = {"fen": recs[3]["fen"], "repetition_count": None}
        self.assertEqual(position.decode(position.encode(r)), r)

    def test_packet_path_and_length(self):
        packet = {"fen": FIXTURE_FENS[6], "legal_moves": ["h8g8"], "facts": {"repetition_count": 2, "terminal_reason": "stalemate"}}
        r = position.packet_to_record(packet)
        self.assertEqual(position.decode(position.encode(r)), r)
        start = {"fen": "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", "repetition_count": 1}
        self.assertLess(len(position.encode(start)), 80)

    def test_distinct_positions_never_encode_equal(self):
        seen = {}
        for rec in records():
            for rep in (1, 2, None):
                r = {"fen": rec["fen"], "repetition_count": rep, "legal_moves": rec["legal_moves"]}
                ids = tuple(position.encode(r))
                self.assertNotIn(ids, seen, (rec["fen"], rep))
                seen[ids] = rec["fen"]
        base = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"
        variants = [base, base.replace(" w ", " b "), base.replace("KQkq", "Kkq"), base.replace("KQkq", "-"),
                    base.replace(" 0 1", " 1 1"), base.replace(" 0 1", " 0 2"), base.replace("R3K2R", "R3K2N"),
                    base.replace("2Q1p", "2Q1P"), base.replace(" - 0 1", " e3 0 1").replace(" w ", " b ")]
        encoded = {tuple(position.encode({"fen": v, "repetition_count": 1})) for v in variants}
        self.assertEqual(len(encoded), len(variants))

    def test_corruption_is_refused(self):
        rec = next(r for r in records() if len(r["legal_moves"]) > 20)
        ids = position.encode({"fen": rec["fen"], "repetition_count": 1, "legal_moves": rec["legal_moves"]})
        for cut in range(len(ids)):
            with self.assertRaises(TokenizerError, msg=f"prefix {cut}"):
                position.decode(ids[:cut])
        with self.assertRaises(TokenizerError):
            position.decode(ids + [vocab.ID["<nl>"]])
        for bad in (-1, vocab.SIZE, 2.5, None, True):
            with self.assertRaises(TokenizerError):
                position.decode(ids[:5] + [bad] + ids[6:])
        board = ids.index(vocab.ID["<board>"])
        for i in range(board + 1, board + 65):  # the board is exactly 64 tokens: no deletion or insert survives
            for mutated in (ids[:i] + ids[i + 1:], ids[:i] + [vocab.ID["<nl>"]] + ids[i:]):
                with self.assertRaises(TokenizerError, msg=f"index {i}"):
                    position.decode(mutated)
        for i in range(board + 1):  # header: refused, or the canonical encoding of a different record
            for mutated in (ids[:i] + ids[i + 1:], ids[:i] + [vocab.ID["<nl>"]] + ids[i:]):
                try:
                    out = position.decode(mutated)
                except TokenizerError:
                    continue
                self.assertEqual(position.encode(out), mutated)
                self.assertNotEqual(out["fen"] + str(out["repetition_count"]),
                                    rec["fen"] + "1", f"index {i}")
        moves_at = ids.index(vocab.ID["<moves>"])
        swapped = ids[:]
        swapped[moves_at + 1], swapped[moves_at + 2] = swapped[moves_at + 2], swapped[moves_at + 1]
        dup = ids[:moves_at + 2] + ids[moves_at + 1:]
        for bad in (swapped, dup):
            with self.assertRaises(TokenizerError):
                position.decode(bad)

    def test_unknown_stays_unknown_and_unsupported_is_refused(self):
        with self.assertRaises(TokenizerError):
            position.encode({"fen": "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -"})
        with self.assertRaises(TokenizerError):
            position.encode({"fen": CHESS960})
        with self.assertRaises(TokenizerError):  # ep square on the wrong rank for the side to move
            position.encode({"fen": "rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d3 0 3"})
        with self.assertRaises(TokenizerError):
            position.encode({"fen": FIXTURE_FENS[0], "repetition_count": 10})
        with self.assertRaises(TokenizerError):  # non-compact placement
            position.encode({"fen": "rnbqkbnr/pppppppp/44/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"})

    def test_moves_agree_with_the_compiler_cgt(self):
        """The legal-move set the tokens carry is the one the compiler's CGT lists for the same position."""
        for rec in random.Random(3).sample(records(), 60):
            ids = position.encode({"fen": rec["fen"], "repetition_count": 1, "legal_moves": rec["legal_moves"]})
            decoded = position.decode(ids)["legal_moves"]
            lite = cgt_text(rec["fen"], "lite")
            n_cgt = sum(len(line.split(": ", 1)[1].split(" ")) for line in lite.split("moves:\n", 1)[-1].split("\n")
                        if ": " in line and line.split(":")[0][0] in "KQRBNPkqrbnp" and len(line.split(":")[0]) == 3)
            self.assertEqual(len(decoded), n_cgt, rec["fen"])


@unittest.skipIf(BINARY is None, "knightlm binary not built")
class CgtTokens(unittest.TestCase):
    def test_exact_round_trip_lite_and_full(self):
        total = 0
        for variant in ("lite", "full"):
            for text in cgt_texts(variant):
                ids = cgt.encode(text)
                self.assertEqual(cgt.decode(ids), text)
                self.assertEqual(cgt.encode(cgt.decode(ids)), ids)
                total += 1
        self.assertGreaterEqual(total, 6000)

    def test_unknown_repetition_and_terminal_lines(self):
        for fen in (FIXTURE_FENS[5], FIXTURE_FENS[6]):
            text = cgt_text(fen, "full", rep=None)
            self.assertIn("rep ?", text)
            self.assertIn("\nterm ", text)
            self.assertEqual(cgt.decode(cgt.encode(text)), text)

    def test_vocabulary_is_structural(self):
        text = cgt_text(FIXTURE_FENS[1], "full")
        ids = cgt.encode(text)
        names = {vocab.TOKENS[i] for i in ids}
        for must in ("<cgt/1>", "<v:full>", "<k:atk>", "<k:ksafe>", "<g:open>", "<pc:K>", "<sq:a5>", "<mv:x>", "<nl>", "</cgt>"):
            self.assertIn(must, names)
        self.assertTrue(all(i < vocab.RANGES["move"][0] for i in ids), "CGT never uses the from-to move tokens")

    def test_noncanonical_text_is_refused(self):
        text = cgt_text(FIXTURE_FENS[1], "full")
        lines = text.split("\n")
        bad_texts = [text + "\n", text.replace("turn w", "turn  w"), text.replace("cgt/1", "cgt/2"),
                     "\n".join(lines[:4] + [lines[5], lines[4]] + lines[6:]), "\n".join(lines + [lines[-1]]),
                     text.replace("moves:\n", "moves:\n\n"), text.replace(": ", ":", 1), text.replace("W: ", "W: Kz9 "),
                     text.replace("\nksafe:", ""), text.replace("clock 0 1", "clock 00 1"), text.replace("clock 0 1", "clock 0 01"),
                     text.replace("rep 1", "rep 10"), text.replace("\r", "") + "\r", ""]
        for bad in bad_texts:
            with self.assertRaises(TokenizerError, msg=bad[:40]):
                cgt.encode(bad)
        with self.assertRaises(TokenizerError):  # Chess960 castling letters are not claimed
            cgt.encode(cgt_text(CHESS960, "lite").replace("castle KQkq", "castle HFhf"))
        lite = cgt_text(FIXTURE_FENS[1], "lite")
        with self.assertRaises(TokenizerError):  # a lite header over a full body
            cgt.encode(text.replace("cgt/1 full", "cgt/1 lite", 1))
        with self.assertRaises(TokenizerError):
            cgt.encode(lite.replace("cgt/1 lite", "cgt/1 full", 1))

    def test_corrupted_sequences_are_refused(self):
        for text in (cgt_texts("full")[5], cgt_texts("full")[400], cgt_texts("lite")[900]):
            ids = cgt.encode(text)
            for cut in range(len(ids)):  # every strict prefix
                with self.assertRaises(TokenizerError, msg=f"prefix {cut}"):
                    cgt.decode(ids[:cut])
            for bad in (-1, vocab.SIZE, 1.5, None):
                with self.assertRaises(TokenizerError):
                    cgt.decode(ids[:9] + [bad] + ids[10:])
            with self.assertRaises(TokenizerError):
                cgt.decode(ids + [ids[-1]])
            # Any single deletion, insertion or substitution either is refused or is itself the
            # canonical encoding of a different document: never silently the original text.
            rng = random.Random(11)
            refused = 0
            attempts = 0
            for _ in range(400):
                i = rng.randrange(len(ids))
                kind = rng.choice(("delete", "insert", "replace", "swap"))
                m = ids[:]
                if kind == "delete":
                    del m[i]
                elif kind == "insert":
                    m.insert(i, rng.randrange(vocab.SIZE))
                elif kind == "replace":
                    m[i] = rng.randrange(vocab.SIZE)
                else:
                    j = rng.randrange(len(m))
                    m[i], m[j] = m[j], m[i]
                if m == ids:
                    continue
                attempts += 1
                try:
                    out = cgt.decode(m)
                except TokenizerError:
                    refused += 1
                    continue
                self.assertNotEqual(out, text)
                self.assertEqual(cgt.encode(out), m)  # what it accepted is canonical
            self.assertGreater(refused, attempts * 0.8)

    def test_a_well_formed_lie_is_the_compilers_to_catch(self):
        """A swapped destination square is well formed: ctok does not claim to catch it, parse_cgt does."""
        text = cgt_texts("lite")[0]
        ids = cgt.encode(text)
        i = max(k for k, t in enumerate(ids) if ids[k - 1] != vocab.ID["<mv:x>"] and t in range(*vocab.RANGES["square"]))
        lie = ids[:]
        lie[i] = vocab.ID["<sq:e5>"] if lie[i] != vocab.ID["<sq:e5>"] else vocab.ID["<sq:e6>"]
        try:
            out = cgt.decode(lie)
        except TokenizerError:
            return  # order rules happened to catch it; also fine
        self.assertNotEqual(out, text)


class PlayCall(unittest.TestCase):
    BASE = 49280

    def test_round_trip_and_text_form(self):
        reasoning = [100, 200, 300]
        ids = play.build_call(reasoning, "e2e4", self.BASE)
        self.assertEqual(play.split_call(ids, self.BASE), (reasoning, "e2e4"))
        self.assertEqual(ids[2:5], reasoning)
        text = play.completion_text("x", "e2e4")
        self.assertTrue(text.startswith("<tool_call>") and '"reasoning":"x","move":"e2e4"' in text)

    def test_refuses_malformed_calls(self):
        ids = play.build_call([5, 6], "g1f3", self.BASE)
        no_close = ids[:-4] + ids[-3:]  # drops </reasoning>
        for bad in (ids[:-1], ids[1:], ids + [ids[-1]], no_close, ids[:-2] + [ids[-1]], ids[:2] + ids[-4:]):
            with self.assertRaises(TokenizerError):
                play.split_call(bad, self.BASE)
        with self.assertRaises(TokenizerError):
            play.build_call([], "e2e4", self.BASE)
        with self.assertRaises(TokenizerError):
            play.build_call([self.BASE + 3], "e2e4", self.BASE)
        with self.assertRaises(TokenizerError):
            play.build_call([5], "e2e9", self.BASE)

    def test_prompt_module_is_unchanged_for_existing_kinds(self):
        self.assertTrue(prompt.INSTRUCTION.startswith("Choose a legal chess move."))


class Manifest(unittest.TestCase):
    def test_cli_prints_the_manifest(self):
        out = subprocess.run(["python3", "-m", "model.tokenizer", "manifest"], cwd=ROOT, check=True,
                             capture_output=True, text=True, timeout=60).stdout
        self.assertEqual(json.loads(out), vocab.manifest())


if __name__ == "__main__":
    unittest.main()
