import unittest

from model.corpus.pipeline import audit_splits, split_candidates


def game(number, family, minute):
    return {"game_id": str(number), "position_key": f"position-{number}",
            "family": family, "utc_date": "2026.09.01", "utc_time": f"00:{minute:02}:00"}


class CorpusSplitTests(unittest.TestCase):
    def test_family_reservation_and_temporal_order(self):
        rows = [game(i, "A0" if i % 2 else "B1", i) for i in range(20)]
        train, holdout = split_candidates(list(reversed(rows)), 5, 2)
        self.assertEqual((len(train), len(holdout)), (5, 2))
        self.assertEqual((train, holdout), split_candidates(rows, 5, 2))
        audit_splits(train, holdout)

    def test_planted_leaks(self):
        train = game(1, "A0", 0)
        holdout = game(2, "B1", 1)
        for key in ("game_id", "family", "position_key"):
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, f"cross-split {key}"):
                audit_splits([train], [dict(holdout, **{key: train[key]})])
        with self.assertRaisesRegex(ValueError, "strictly later"):
            audit_splits([train], [dict(holdout, utc_time=train["utc_time"])])

    def test_too_few_disjoint_games_refuses(self):
        rows = [game(i, "A0", i) for i in range(10)]
        with self.assertRaisesRegex(ValueError, "could only split"):
            split_candidates(rows, 1, 1)
