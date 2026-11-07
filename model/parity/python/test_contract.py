import unittest
from knightlm_contract import square_map


class ContractTests(unittest.TestCase):
    def test_full_board(self):
        board = square_map("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR")
        self.assertEqual(len(board), 64)
        self.assertEqual(board[0], "r")
        self.assertEqual(board[63], "R")

    def test_invalid_placements(self):
        for text in ("8/8", "9/8/8/8/8/8/8/8", "x7/8/8/8/8/8/8/8", "7/8/8/8/8/8/8/8"):
            with self.assertRaises(ValueError):
                square_map(text)


if __name__ == "__main__":
    unittest.main()
