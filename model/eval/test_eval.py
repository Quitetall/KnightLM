import contextlib
import hashlib
import io
import json
import shutil
import tempfile
import unittest
from pathlib import Path

from model.eval import compare, fakes, games, run, tactics
from model.eval.corpus import load_holdout
from model.eval.engine import StockfishScorer, find_binary, find_stockfish
from model.eval.predictors import load_predictor
from model.eval.scorecard import attach_cpl, build_scorecard, predict_rows

MODES = ("raw", "constrained")


def make_corpus(root, rows_file=fakes.FIXTURE):
    root.mkdir(parents=True, exist_ok=True)
    shard = root / "holdout-00000.jsonl"
    shutil.copy(rows_file, shard)
    games_count = len(shard.read_text().splitlines())
    manifest = {"format": "knightlm-lichess-v2", "shards": {"holdout": {"files": [
        {"file": shard.name, "games": games_count, "sha256": hashlib.sha256(shard.read_bytes()).hexdigest()}]}}}
    (root / "manifest.json").write_text(json.dumps(manifest))
    return root


class FakeScorer:
    """Hermetic stand-in for Stockfish: the human move loses 10 cp, anything else 80."""

    depth = 12

    def __init__(self):
        self.targets = {r["packet"]["fen"]: r["target"] for r in fakes.fixture_rows()}

    def score_moves(self, pairs):
        return {(fen, mv): {"status": "scored", "cpl": 10 if self.targets[fen] == mv else 80,
                            "best_move": self.targets[fen]} for fen, mv in set(pairs)}

    def rank(self, fens, lines=2, depth=None):
        out = {}
        for k, fen in enumerate(sorted(set(fens))):
            margin = 200 if k % 2 == 0 else 20
            out[fen] = {"status": "ranked", "margin_cp": margin, "best_move": self.targets[fen], "engine": "fake",
                        "lines": [{"clipped_cp": 100}, {"clipped_cp": 100 - margin}]}
        return out

    def info(self):
        return {"engine": "FakeScorer", "depth": 12, "cpl_clip_cp": 1000}


def card(factory, timeout_s=0.3, scorer=None, rows=None):
    rows = rows or fakes.fixture_rows()
    records = predict_rows(rows, factory(), MODES, timeout_s)
    if scorer:
        attach_cpl(records, scorer, MODES)
    return records, build_scorecard(records, MODES, scorer.info() if scorer else None, None, len(rows))


class AxisTests(unittest.TestCase):
    def test_perfect_predictor(self):
        _, c = card(fakes.perfect)
        a = c["axes"]
        self.assertEqual((a["raw_legality"]["legal"], a["raw_legality"]["n"]), (6, 6))
        self.assertTrue(a["raw_legality"]["gate"]["met"])
        self.assertEqual(a["played_move_match"]["raw"]["matches"], 6)
        self.assertEqual(a["failures"]["raw"]["total"], 0)
        self.assertIsNone(c["composite_score"])
        self.assertNotIn("composite", json.dumps(a))

    def test_always_illegal_is_all_failures(self):
        _, c = card(fakes.always_illegal)
        a = c["axes"]
        self.assertEqual(a["raw_legality"]["rate"], 0.0)
        self.assertFalse(a["raw_legality"]["gate"]["met"])
        self.assertEqual(a["constrained_legality"]["legal"], 0)
        self.assertEqual(a["failures"]["raw"]["by_kind"]["illegal"], 6)
        # the predictor claimed legal=True on every row; the harness does not believe it
        self.assertEqual(a["failures"]["raw"]["predictor_legal_claim_disagrees"], 6)
        self.assertEqual(a["played_move_match"]["raw"]["matches"], 0)

    def test_garbled_replies_are_failures_not_skips(self):
        records, c = card(fakes.garbled)
        f = c["axes"]["failures"]["raw"]
        self.assertEqual(f["total"], 6)
        self.assertEqual(sum(f["by_kind"].values()), 6)
        self.assertGreaterEqual(f["by_kind"]["unparseable"], 1)
        self.assertGreaterEqual(f["by_kind"]["empty"], 1)
        self.assertEqual(c["axes"]["raw_legality"]["n"], 6)
        self.assertIn(records[0]["raw"]["raw_reply"], ("I think Nf3 is best!",))

    def test_timeouts_stay_in_the_denominator(self):
        _, c = card(fakes.timeout)
        a = c["axes"]
        self.assertEqual(a["failures"]["raw"]["by_kind"]["timeout"], 3)
        self.assertEqual((a["raw_legality"]["legal"], a["raw_legality"]["n"]), (3, 6))
        self.assertEqual(a["played_move_match"]["raw"]["rate"], 0.5)
        self.assertEqual(a["latency_ms"]["raw"]["n"], 3)  # no latency invented for timeouts

    def test_crashes_are_exceptions(self):
        records, c = card(fakes.crashing)
        self.assertEqual(c["axes"]["failures"]["raw"]["by_kind"]["exception"], 3)
        self.assertIn("RuntimeError", records[1]["raw"]["error"])

    def test_partial_prediction_objects(self):
        class Bare:
            move = "e2e4"

        class Predictor:
            def predict(self, packet, mode):
                return Bare()

        records = predict_rows(fakes.fixture_rows(), Predictor(), MODES, 1)
        self.assertEqual(records[0]["raw"]["input_tokens"], None)
        c = build_scorecard(records, MODES, None, None, 6)
        self.assertEqual(c["axes"]["token_cost"]["raw"]["rows_reporting"], 0)
        self.assertEqual(c["axes"]["raw_legality"]["n"], 6)

    def test_missing_records_cannot_be_dropped(self):
        records = predict_rows(fakes.fixture_rows(), fakes.perfect(), MODES, 1)
        with self.assertRaises(ValueError):
            build_scorecard(records[:5], MODES, None, None, 6)

    def test_worked_example(self):
        records, c = card(fakes.worked_example, scorer=FakeScorer())
        a = c["axes"]
        self.assertEqual((a["raw_legality"]["legal"], a["raw_legality"]["n"], a["raw_legality"]["rate"]),
                         (2, 6, 0.3333))
        self.assertFalse(a["raw_legality"]["gate"]["met"])
        self.assertEqual(a["constrained_legality"]["rate"], 1.0)
        self.assertEqual(a["played_move_match"]["raw"]["matches"], 1)
        self.assertEqual(a["played_move_match"]["constrained"]["matches"], 1)
        self.assertEqual(a["failures"]["raw"]["by_kind"],
                         {"illegal": 1, "unparseable": 1, "empty": 0, "exception": 1, "timeout": 1, "missing": 0})
        cpl = a["centipawn_loss"]["raw"]
        self.assertEqual((cpl["rows_scored"], cpl["rows_unscored"], cpl["mean"]), (2, 4, 45.0))
        self.assertEqual(a["centipawn_loss"]["constrained"]["mean"], round((10 + 5 * 80) / 6, 4))
        self.assertEqual(a["latency_ms"]["raw"]["n"], 4)
        self.assertEqual(a["token_cost"]["raw"]["output"]["max"], 8)
        self.assertEqual([r["raw"]["status"] for r in records],
                         ["ok", "ok", "illegal", "unparseable", "timeout", "exception"])

    def test_percentiles_are_nearest_rank(self):
        from model.eval.scorecard import percentile
        self.assertEqual([percentile([1, 2, 3, 4], p) for p in (50, 90, 100)], [2, 4, 4])
        self.assertIsNone(percentile([], 50))


class TacticsTests(unittest.TestCase):
    def test_derive_load_and_score(self):
        with tempfile.TemporaryDirectory() as tmp:
            rows, _ = load_holdout(make_corpus(Path(tmp) / "c"))
            tset = tactics.derive(rows, FakeScorer(), depth=16, margin=150)
            self.assertEqual(tset["rows_in_set"], 3)
            path = Path(tmp) / "t.json"
            tactics.write(path, tset)
            loaded = tactics.load_tactics(path, rows)
            records = predict_rows(rows, fakes.perfect(), MODES, 1)
            attach_cpl(records, FakeScorer(), MODES)
            c = build_scorecard(records, MODES, FakeScorer().info(), loaded, 6)
            t = c["axes"]["tactical"]
            self.assertEqual((t["raw"]["n"], t["raw"]["correct"]), (3, 3))
            bad = json.loads(path.read_text())
            bad["items"][0]["fen"] = "8/8/8/8/8/8/8/K6k w - - 0 1"
            path.write_text(json.dumps(bad))
            with self.assertRaises(ValueError):
                tactics.load_tactics(path, rows)

    def test_failures_count_against_tactical_accuracy(self):
        with tempfile.TemporaryDirectory() as tmp:
            rows, _ = load_holdout(make_corpus(Path(tmp) / "c"))
            path = Path(tmp) / "t.json"
            tactics.write(path, tactics.derive(rows, FakeScorer()))
            records = predict_rows(rows, fakes.always_illegal(), MODES, 1)
            c = build_scorecard(records, MODES, None, tactics.load_tactics(path, rows), 6)
            self.assertEqual((c["axes"]["tactical"]["raw"]["n"], c["axes"]["tactical"]["raw"]["correct"]), (3, 0))


class CorpusTests(unittest.TestCase):
    def test_checksum_mismatch_refuses(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = make_corpus(Path(tmp) / "c")
            (root / "holdout-00000.jsonl").write_text("tampered\n")
            with self.assertRaises(ValueError):
                load_holdout(root)

    def test_too_many_rows_requested(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(ValueError):
                load_holdout(make_corpus(Path(tmp) / "c"), 7)


class CliTests(unittest.TestCase):
    def run_cli(self, tmp, label, spec, extra=()):
        argv = ["--predictor", spec, "--label", label, "--rows", "6", "--corpus", str(make_corpus(Path(tmp) / "c")),
                "--out-root", str(Path(tmp) / "out"), "--no-stockfish", "--no-tactics", "--timeout-s", "1", *extra]
        with contextlib.redirect_stdout(io.StringIO()):
            return run.main(argv)

    def test_outputs_and_determinism(self):
        with tempfile.TemporaryDirectory() as tmp:
            a = self.run_cli(tmp, "a", "model.eval.fakes:perfect")
            b = self.run_cli(tmp, "b", "model.eval.fakes:perfect")
            for name in ("scorecard.json", "scorecard.md", "predictions.jsonl", "inputs.json"):
                self.assertTrue((a / name).exists(), name)
            ca, cb = (json.loads((d / "scorecard.json").read_text()) for d in (a, b))
            for c in (ca, cb):
                c.pop("run")
                c["inputs"]["settings"].pop("label")
            ca["inputs"]["git"] = cb["inputs"]["git"] = None
            self.assertEqual(ca, cb)
            self.assertEqual((a / "predictions.jsonl").read_bytes(), (b / "predictions.jsonl").read_bytes())
            inputs = json.loads((a / "inputs.json").read_text())
            self.assertEqual(inputs["corpus"]["corpus_manifest_sha256"],
                             hashlib.sha256((Path(tmp) / "c/manifest.json").read_bytes()).hexdigest())
            self.assertEqual(inputs["predictor_manifest"], {"fake": "Perfect"})
            self.assertEqual(len(inputs["git"]["commit"]), 40)

    def test_compare_prints_deltas_and_no_composite(self):
        with tempfile.TemporaryDirectory() as tmp:
            a = self.run_cli(tmp, "bad", "model.eval.fakes:always_illegal")
            b = self.run_cli(tmp, "good", "model.eval.fakes:perfect")
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                self.assertEqual(compare.main([str(a), str(b)]), 0)
            text = out.getvalue()
            line = next(ln for ln in text.splitlines() if ln.startswith("raw_legality.rate"))
            self.assertEqual(line.split()[1:4], ["0", "1", "1"])
            self.assertIn("No composite", text)
            self.assertNotIn("NOT COMPARABLE", text)
            # same-label tools refuse to guess: a different row count is flagged
            c = json.loads((b / "scorecard.json").read_text())
            c["inputs"]["corpus"]["rows_used"] = 99
            self.assertTrue(any("rows differs" in n for n in compare.comparability(
                json.loads((a / "scorecard.json").read_text()), c)))

    def test_planted_failures_visible_in_report(self):
        with tempfile.TemporaryDirectory() as tmp:
            d = self.run_cli(tmp, "w", "model.eval.fakes:worked_example")
            md = (d / "scorecard.md").read_text()
            self.assertIn("Failure accounting", md)
            self.assertIn("| raw | 4 | 0.6667 | 1 | 1 | 0 | 1 | 1 |", md)
            self.assertNotIn("composite score:", md.lower())

    def test_python_spec_errors(self):
        with self.assertRaises(ValueError):
            load_predictor("nocolon")


def stockfish_available():
    try:
        find_binary()
        find_stockfish()
        return True
    except FileNotFoundError:
        return False


@unittest.skipUnless(stockfish_available(), "needs the knightlm binary and Stockfish")
class StockfishIntegration(unittest.TestCase):
    def test_cpl_through_rust_tool(self):
        scorer = StockfishScorer(depth=8, workers=2)
        rows = fakes.fixture_rows()
        records = predict_rows(rows, fakes.worked_example(), MODES, 1)
        attach_cpl(records, scorer, MODES)
        c = build_scorecard(records, MODES, scorer.info(), None, 6)
        cpl = c["axes"]["centipawn_loss"]
        self.assertEqual(cpl["raw"]["rows_scored"], 2)
        self.assertEqual(cpl["constrained"]["rows_scored"], 6)
        self.assertIn("Stockfish 19", scorer.info()["engine"])
        again = StockfishScorer(depth=8, workers=3).score_moves({(r["packet"]["fen"], r["target"]) for r in rows})
        first = scorer.score_moves({(r["packet"]["fen"], r["target"]) for r in rows})
        self.assertEqual(again, first)  # worker count and order do not change a score

    def test_games_with_planted_illegal_and_legal_players(self):
        def play(factory):
            server, url = games.serve_predictor(factory())
            try:
                return games.run_games(url, 2, opponent_depth=1)
            finally:
                server.shutdown()
                server.server_close()

        illegal = play(fakes.always_illegal)
        self.assertEqual((illegal["games"], illegal["losses"]), (2, 2))
        self.assertEqual(illegal["by_termination"]["illegal move by subject"]["loss"], 2)
        crashing = play(fakes.crashing)  # off-fixture positions index as -1, which crashes
        self.assertEqual(crashing["by_termination"]["player failure by subject"]["loss"], 2)
        legal = play(fakes.first_legal)
        self.assertEqual(sum(sum(c.values()) for c in legal["by_termination"].values()), 2)
        self.assertEqual(legal["wins"] + legal["draws"] + legal["losses"], 2)
        self.assertEqual({g["subject_color"] for g in legal["per_game"]}, {"white", "black"})
