import json
import unittest
from pathlib import Path

from model.eval import fakes
from model.eval.explain import CLAIM_TYPES, UNCHECKED_REASONS, Board, aggregate, check_explanation

FIXTURES = json.loads((Path(__file__).parent / "fixtures" / "explanations.json").read_text())["items"]
ROWS = fakes.fixture_rows()


def run(item):
    packet = ROWS[item["row"]]["packet"]
    return check_explanation(packet["fen"], packet["legal_moves"], item["text"], item["move"])


def counts(result):
    return {k: [v["checkable"], v["failed"]] for k, v in result["claim_types"].items() if v["checkable"]}


class FixtureTests(unittest.TestCase):
    def test_every_fixture_matches_its_expected_counts(self):
        for item in FIXTURES:
            with self.subTest(item["name"]):
                result = run(item)
                self.assertEqual(counts(result), item["expect"])
                self.assertEqual({k: v for k, v in result["unchecked"].items() if v}, item.get("unchecked", {}))

    def test_every_claim_type_has_a_planted_refusal(self):
        planted = {item["planted"] for item in FIXTURES if item["planted"]}
        self.assertEqual(planted, set(CLAIM_TYPES))
        for item in FIXTURES:
            if item["planted"]:
                with self.subTest(item["name"]):
                    self.assertGreaterEqual(run(item)["claim_types"][item["planted"]]["failed"], 1)

    def test_true_explanations_fail_nothing(self):
        for item in FIXTURES:
            if item["name"].startswith("true-") or item["name"] == "castling-and-promotion-forms":
                with self.subTest(item["name"]):
                    self.assertEqual(run(item)["failed_total"], 0)
                    self.assertGreater(run(item)["checkable_total"], 0)

    def test_failures_name_the_claim_and_the_reason(self):
        item = next(i for i in FIXTURES if i["name"] == "planted-piece-on-square")
        failure = run(item)["failures"][0]
        self.assertEqual(failure["type"], "piece_on_square")
        self.assertIn("d4", failure["claim"])
        self.assertIn("white pawn", failure["reason"])

    def test_unknown_inputs_do_not_crash(self):
        packet = ROWS[0]["packet"]
        self.assertEqual(check_explanation(packet["fen"], packet["legal_moves"], "", None)["checkable_total"], 0)
        self.assertEqual(check_explanation(packet["fen"], packet["legal_moves"], None, None)["checkable_total"], 0)
        with self.assertRaises(ValueError):
            check_explanation("not a fen", [], "text")


class AggregateTests(unittest.TestCase):
    def test_counts_per_type_and_no_composite(self):
        per_row = [run(item) for item in FIXTURES] + [None]
        axis = aggregate(per_row)
        self.assertEqual(axis["n"], len(FIXTURES) + 1)
        self.assertEqual(axis["explanations_missing"], 1)
        self.assertEqual(axis["claim_types"]["move_legal"], {
            "checkable": sum(i["expect"].get("move_legal", [0, 0])[0] for i in FIXTURES),
            "failed": sum(i["expect"].get("move_legal", [0, 0])[1] for i in FIXTURES)})
        self.assertEqual(axis["explanations_without_checkable_claims"], 3)
        self.assertEqual(set(axis["unchecked_claims"]), set(UNCHECKED_REASONS))
        flat = json.dumps(axis)
        self.assertNotIn("composite_score", flat)
        self.assertNotIn("score", axis)
        self.assertEqual(axis["checkable_total"], sum(v["checkable"] for v in axis["claim_types"].values()))


class BoardAgainstPythonChess(unittest.TestCase):
    """The small board here is checked against python-chess when it is installed (it is not a dependency)."""

    def test_check_capture_and_material_agree_for_every_legal_move(self):
        try:
            import chess
        except ImportError:
            self.skipTest("python-chess is not installed")
        values = {chess.PAWN: 1, chess.KNIGHT: 3, chess.BISHOP: 3, chess.ROOK: 5, chess.QUEEN: 9, chess.KING: 0}
        checked = 0
        for row in ROWS:
            packet = row["packet"]
            reference, board = chess.Board(packet["fen"]), Board(packet["fen"])
            self.assertEqual({m.uci() for m in reference.legal_moves}, set(packet["legal_moves"]))
            for move in reference.legal_moves:
                self.assertEqual(board.gives_check(move.uci()), reference.gives_check(move), move.uci())
                self.assertEqual(board.is_capture(move.uci()), reference.is_capture(move), move.uci())
                checked += 1
            totals = board.material()
            for color, key in ((chess.WHITE, "w"), (chess.BLACK, "b")):
                self.assertEqual(totals[key], sum(values[p.piece_type] for p in reference.piece_map().values()
                                                  if p.color == color))
        self.assertGreater(checked, 200)


if __name__ == "__main__":
    unittest.main()
