import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from model.corpus.book import compact, games, inline_sequences, paragraphs_and_rows
from model.corpus.commentary import clean_comment, extract, reject_reason
from model.corpus.records import audit_records, make_record

BINARY = Path(__file__).resolve().parents[2] / "target" / "release" / "knightlm"
START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
AFTER_E4 = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1"


def record(source, text, fen, kind="commentary"):
    return make_record(source, "CC0", kind, text, [{"role": "after", "fen": fen}], {"adapter": "t"})


def comment_row(text, ply=1):
    return {"kind": "comment", "game_id": "https://lichess.org/study/a/b", "ply": ply, "san": "e4",
            "uci": "e2e4", "nags": [], "start_fen": START, "fen_before": START, "fen_after": AFTER_E4,
            "comment": text}


class CommentaryFilterTests(unittest.TestCase):
    def test_commands_and_urls_are_removed(self):
        self.assertEqual(clean_comment("Good [%clk 0:01:00] move https://x.org/a [%eval 0.3]"), "Good move")

    def test_filters(self):
        self.assertEqual(reject_reason("e4"), "too_short")
        self.assertEqual(reject_reason("Inaccuracy. Nf3 was best."), "engine_template")
        self.assertEqual(reject_reason("+0.3 +0.5 -1.0 =0.0 +2.5 3.1"), "not_prose")
        self.assertIsNone(reject_reason("White grabs the centre and opens the bishop."))

    def test_extract_counts_quarantine_and_duplicates(self):
        text = "White grabs the centre and opens the bishop."
        rows = [comment_row(text), comment_row(text), comment_row("[%clk 0:01:00]"),
                {"kind": "quarantine", "game_id": "g", "ply": 3, "stage": "replay", "san": "Ke7", "error": "illegal SAN"},
                {"kind": "game", "game_id": "g", "plies": 3, "replayed": 2, "failed": True,
                 "mainline_comments": 3, "all_comments": 3}]
        records, stats, quarantine, _ = extract(rows, "src", "CC0", "0" * 64)
        self.assertEqual(len(records), 1)
        self.assertEqual((stats["rejected_duplicate"], stats["rejected_too_short"], stats["games_replay_failed"]), (1, 1, 1))
        self.assertEqual(quarantine[0]["san"], "Ke7")
        self.assertEqual([p["fen"] for p in records[0]["positions"]], [START, AFTER_E4])


@unittest.skipUnless(BINARY.exists(), "release knightlm binary not built")
class ReplayBinaryTests(unittest.TestCase):
    def run_pgn(self, pgn):
        done = subprocess.run([str(BINARY), "corpus-commentary", "--source", "t.pgn"], input=pgn.encode(),
                              capture_output=True, timeout=60, check=True)
        return [json.loads(line) for line in done.stdout.splitlines()]

    def test_corrupted_move_is_quarantined_and_nothing_after_it_is_emitted(self):
        rows = self.run_pgn('[Event "x"]\n\n1. e4 {A long enough comment here.} e5 2. Ke7 {Bad move comment text here.} Nc6 3. Nf3 {Never emitted at all.} *\n')
        self.assertEqual([r["kind"] for r in rows], ["comment", "quarantine", "game"])
        self.assertEqual(rows[1]["san"], "Ke7")
        self.assertEqual(rows[2]["replayed"], 2)

    def test_descriptive_replay_refuses_illegal(self):
        request = json.dumps({"id": 1, "notation": "descriptive", "sans": ["P - K 4", "P - K 4", "Q - R 8"]})
        done = subprocess.run([str(BINARY), "replay-lines"], input=request.encode(), capture_output=True, timeout=60, check=True)
        reply = json.loads(done.stdout)
        self.assertEqual((reply["ok"], reply["played"], reply["failed_at"]), (False, 2, 2))


class BookParsingTests(unittest.TestCase):
    BOOK = (
        "Intro text.\n\n        1. P - K 4                P - K 4\n        2. Kt - K B 3             Kt - Q B 3\n\n"
        "White develops and attacks the pawn on {12} the e file while keeping options open.\n\n"
        "        3. B - Kt 5               ........\n        4. B x Kt\n\n"
        "   1. QUEEN'S GAMBIT DECLINED (MATCH, 1909)        159\n\n"
        "        7. R - B 1                P - Q Kt 3\n"
    )

    def test_rows_notes_and_unknown_start(self):
        found = games(list(paragraphs_and_rows(self.BOOK)))
        self.assertEqual(found[0]["sans"], ["P - K 4", "P - K 4", "Kt - K B 3", "Kt - Q B 3", "B - Kt 5", "B x Kt"])
        self.assertEqual(found[0]["notes"][0][0], 4)
        self.assertNotIn("{12}", found[0]["notes"][0][1])
        self.assertEqual((found[1]["start"], found[1]["first_number"]), ("unknown", 7))
        self.assertEqual(len(found), 2)

    def test_inline_sequences(self):
        found = inline_sequences("It goes 4 K - K 4, K - Q 1; 5 K - Q 5, K - B 1; 6 K - Q 6.")
        self.assertEqual(found[0]["number"], 4)
        self.assertEqual(found[0]["sans"], ["K - K 4", "K - Q 1", "K - Q 5", "K - B 1", "K - Q 6"])
        self.assertEqual(compact(" P - K 4;"), "P - K 4")


class AuditTests(unittest.TestCase):
    def test_clean_split_passes(self):
        audit_records([record("a", "one", START)], [record("b", "two", AFTER_E4)])

    def test_planted_leaks_are_refused(self):
        train, hold = record("a", "one", START), record("b", "two", AFTER_E4)
        cases = {
            "source_id": dict(hold, source_id="a"),
            "content_hash": dict(hold, content_hash=train["content_hash"]),
            "position": dict(hold, positions=[{"role": "after", "fen": START.replace(" 0 1", " 5 9")}]),
        }
        for name, bad in cases.items():
            with self.subTest(name), self.assertRaisesRegex(ValueError, "cross-split"):
                audit_records([train], [bad])

    def test_position_leak_from_any_kind(self):
        text_row = record("t", "a book paragraph", AFTER_E4, kind="text")
        with self.assertRaisesRegex(ValueError, "position leakage"):
            audit_records([text_row], [record("h", "holdout study", AFTER_E4)])


if __name__ == "__main__":
    unittest.main()
