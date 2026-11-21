import ast
import contextlib
import io
import json
import os
import shutil
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest import mock

from model.eval import compare, fakes, oracles, run
from model.eval.engine import REPO, find_binary
from model.eval.oracles import OracleUnavailable
from model.eval.scorecard import predict_rows
from model.eval.test_eval import make_corpus

MODES = ("raw", "constrained")
FAKE = Path(__file__).parent / "fixtures" / "fake_uci.py"
ROWS = fakes.fixture_rows()


def knightlm_binary():
    """The Rust tool, release or debug (CI builds debug); None when it is not built."""
    for candidate in (os.environ.get("KNIGHTLM_BIN"), REPO / "target/release/knightlm", REPO / "target/debug/knightlm"):
        if candidate and Path(candidate).is_file():
            return Path(candidate).resolve()
    try:
        return find_binary()
    except FileNotFoundError:
        return None


BINARY = knightlm_binary()


def other(i):
    return next(m for m in ROWS[i]["packet"]["legal_moves"] if m != ROWS[i]["target"])


def script(path, maia_other_for=(), **extra):
    """lc0 best = the human move at 30 cp, then another move at 10. Maia (weights path has 'maia') plays
    the human move except on `maia_other_for` rows."""
    lc0, maia = {}, {}
    for i, row in enumerate(ROWS):
        fen = row["packet"]["fen"]
        lc0[fen] = [[row["target"], 30], [other(i), 10]]
        maia[fen] = [[other(i) if i in maia_other_for else row["target"], 50]]
    path.write_text(json.dumps({"name": "FakeLc0 0", "positions": lc0, "maia_positions": maia, **extra}))
    return path


@unittest.skipUnless(BINARY, "needs the knightlm binary (cargo build)")
class FakeUciOracleTests(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.weights = self.tmp / "net.pb.gz"
        self.weights.write_bytes(b"not a real network")
        self.log = self.tmp / "uci.log"
        env = {"FAKE_UCI_SCRIPT": str(script(self.tmp / "s.json", strict=True)), "FAKE_UCI_LOG": str(self.log)}
        patcher = mock.patch.dict(os.environ, env)
        patcher.start()
        self.addCleanup(patcher.stop)

    def oracle(self, **kw):
        return oracles.Lc0Oracle("lc0", FAKE, self.weights, 800, workers=2, binary=BINARY, **kw)

    def test_lc0_cpl_through_the_rust_driver_with_nodes_and_weights(self):
        records = predict_rows(ROWS, fakes.worked_example(), MODES, 1)
        axis = oracles.lc0_axis(records, MODES, self.oracle())
        self.assertEqual(axis["status"], "scored")
        # raw: rows 0 (best, cpl 0) and 1 (other, cpl 30 - 0); constrained: row 0 best, rows 1-5 other
        self.assertEqual((axis["raw"]["rows_scored"], axis["raw"]["rows_unscored"], axis["raw"]["mean"]), (2, 4, 15.0))
        self.assertEqual((axis["constrained"]["rows_scored"], axis["constrained"]["mean"]), (6, 25.0))
        self.assertEqual(records[1]["raw"]["lc0_best_move"], ROWS[1]["target"])
        commands = self.log.read_text()
        self.assertIn("go nodes 800", commands)
        self.assertNotIn("go depth", commands)
        self.assertIn(f"setoption name WeightsFile value {self.weights.resolve()}", commands)
        self.assertNotIn("setoption name Hash", commands)  # the strict fake exits on Hash, as lc0 has no such option
        self.assertEqual(axis["engine"]["nodes"], 800)
        self.assertEqual(len(axis["engine"]["weights_sha256"]), 64)

    def test_backend_is_pinned_by_option_or_env_and_recorded(self):
        records = predict_rows(ROWS, fakes.worked_example(), MODES, 1)
        axis = oracles.lc0_axis(records, MODES, self.oracle())
        self.assertEqual(axis["engine"]["backend"], "lc0 default")
        self.assertNotIn("name Backend", self.log.read_text())
        with mock.patch.dict(os.environ, {"LC0_BACKEND": "blas"}):
            axis = oracles.lc0_axis(records, MODES, self.oracle())
        self.assertEqual(axis["engine"]["backend"], "blas")
        self.assertIn("setoption name Backend value blas", self.log.read_text())
        self.assertEqual(self.oracle(backend="eigen").info()["backend"], "eigen")

    def test_maia_match_counts_every_row_and_reports_the_human_reference(self):
        with mock.patch.dict(os.environ, {"FAKE_UCI_SCRIPT": str(script(self.tmp / "m.json", maia_other_for=(2,)))}):
            weights = self.tmp / "maia-1500.pb.gz"
            weights.write_bytes(b"maia")
            maia = oracles.Lc0Oracle("maia-1500", FAKE, weights, 1, binary=BINARY)
            for factory, expected in ((fakes.perfect, 5), (fakes.always_illegal, 0)):
                records = predict_rows(ROWS, factory(), MODES, 1)
                axis = oracles.maia_axis(records, MODES, maia, 1500)
                self.assertEqual(axis["raw"], {"matches": expected, "n": 6, "rate": round(expected / 6, 4)})
                self.assertEqual(axis["reference_human_move_matches_maia"]["matches"], 5)
            self.assertIn("go nodes 1", self.log.read_text())
            self.assertEqual(records[2]["maia_move"], other(2))

    def test_a_failed_reply_is_a_miss_not_a_dropped_row(self):
        with mock.patch.dict(os.environ, {"FAKE_UCI_SCRIPT": str(script(self.tmp / "m2.json"))}):
            weights = self.tmp / "maia-1500.pb.gz"
            weights.write_bytes(b"maia")
            records = predict_rows(ROWS, fakes.worked_example(), MODES, 1)
            axis = oracles.maia_axis(records, MODES, oracles.Lc0Oracle("maia-1500", FAKE, weights, 1, binary=BINARY), 1500)
            self.assertEqual(axis["raw"], {"matches": 1, "n": 6, "rate": 0.1667})  # 4 failed rows are misses

    def test_unusable_engine_is_an_error_not_a_zero(self):
        broken = oracles.Lc0Oracle("lc0", self.tmp / "nope", self.weights, 800, binary=BINARY)
        records = predict_rows(ROWS, fakes.perfect(), MODES, 1)
        with self.assertRaises(RuntimeError):
            oracles.lc0_axis(records, MODES, broken)

    def test_scorecard_wiring_and_compare(self):
        def go(label, predictor, extra_env=None):
            env = {"LC0_PATH": str(FAKE), "LC0_WEIGHTS": str(self.weights), "KNIGHTLM_BIN": str(BINARY),
                   "MAIA_WEIGHTS_DIR": str(self.tmp)}
            (self.tmp / "maia-1500.pb.gz").write_bytes(b"maia")
            with mock.patch.dict(os.environ, env | (extra_env or {})), contextlib.redirect_stdout(io.StringIO()):
                return run.main(["--predictor", predictor, "--label", label, "--rows", "6", "--no-stockfish",
                                 "--no-tactics", "--timeout-s", "1", "--corpus", str(make_corpus(self.tmp / "c")),
                                 "--out-root", str(self.tmp / "out"), "--oracles", "lc0", "maia",
                                 "--lc0-nodes", "400", "--explain"])

        good = go("good", "model.eval.fakes:explaining")
        bad = go("bad", "model.eval.fakes:always_illegal")
        card = json.loads((good / "scorecard.json").read_text())
        axes = card["axes"]
        self.assertEqual(axes["lc0_strength"]["raw"]["mean"], 0.0)
        self.assertEqual(axes["maia_human_likeness"]["raw"]["matches"], 6)
        self.assertEqual(axes["maia_human_likeness"]["rating"], 1500)
        expl = axes["explanation"]
        self.assertEqual(expl["claim_types"]["piece_on_square"], {"checkable": 2, "failed": 1})
        self.assertEqual(expl["claim_types"]["move_legal"], {"checkable": 1, "failed": 1})
        self.assertEqual((expl["explanations_missing"], expl["explanations_with_a_failed_claim"]), (1, 1))
        self.assertIsNone(card["composite_score"])
        self.assertIn("lc0 centipawn loss", (good / "scorecard.md").read_text())
        self.assertIn("Move match to Maia 1500", (good / "scorecard.md").read_text())
        self.assertIn("Claims by type", (good / "scorecard.md").read_text())
        self.assertEqual(card["inputs"]["oracles"]["lc0"]["nodes"], 400)
        records = [json.loads(line) for line in (good / "predictions.jsonl").read_text().splitlines()]
        self.assertIn("explain failed", records[2]["explanation"]["error"])
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(compare.main([str(bad), str(good)]), 0)
        text = out.getvalue()
        line = next(ln for ln in text.splitlines() if ln.startswith("maia_human_likeness.raw.rate"))
        self.assertEqual(line.split()[1:4], ["0", "1", "1"])
        self.assertTrue(line.endswith("up"))
        self.assertNotIn("NOT COMPARABLE", text)
        self.assertIn("No composite", text)
        failed = next(ln for ln in text.splitlines() if ln.startswith("explanation.claim_types.move_legal.failed"))
        self.assertTrue(failed.endswith("down") or failed.split()[-1] == "down")


class SkipTests(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, True)
        env = {k: v for k, v in os.environ.items() if k not in ("LC0_PATH", "LC0_WEIGHTS", "MAIA_WEIGHTS_DIR")}
        for patcher in (mock.patch.dict(os.environ, env, clear=True),
                        mock.patch.object(oracles, "LC0_DEFAULT", self.tmp / "absent-lc0"),
                        mock.patch.object(oracles.shutil, "which", return_value=None)):
            patcher.start()
            self.addCleanup(patcher.stop)

    def test_maia_weights_dir_accepts_flat_and_stored_layouts(self):
        flat, stored, empty = self.tmp / "flat", self.tmp / "stored", self.tmp / "empty"
        for d in (flat, stored / "1500", empty):
            d.mkdir(parents=True)
        (flat / "maia-1500.pb.gz").write_bytes(b"f")
        (stored / "1500" / "maia-1500.pb.gz").write_bytes(b"s")
        self.assertEqual(oracles.find_maia_weights(1500, directory=flat), (flat / "maia-1500.pb.gz").resolve())
        self.assertEqual(oracles.find_maia_weights(1500, directory=stored), (stored / "1500" / "maia-1500.pb.gz").resolve())
        with mock.patch.dict(os.environ, {"MAIA_WEIGHTS_DIR": str(stored)}):
            self.assertEqual(oracles.find_maia_weights(1500).name, "maia-1500.pb.gz")
        with self.assertRaises(OracleUnavailable):
            oracles.find_maia_weights(1500, directory=empty)
        with self.assertRaises(OracleUnavailable):  # the stored layout has no 1100 here
            oracles.find_maia_weights(1100, directory=stored)

    def test_named_reasons(self):
        with self.assertRaisesRegex(OracleUnavailable, "lc0 binary not found"):
            oracles.find_lc0()
        with self.assertRaisesRegex(OracleUnavailable, "lc0 network weights not found"):
            oracles.find_lc0_weights()
        with self.assertRaisesRegex(OracleUnavailable, "Maia 1500 weights not found"):
            oracles.find_maia_weights(1500)
        with self.assertRaisesRegex(OracleUnavailable, "no Maia net at rating 1234"):
            oracles.find_maia_weights(1234)
        (self.tmp / "maia-1500.pb.gz").write_bytes(b"x")
        self.assertEqual(oracles.find_maia_weights(1500, directory=self.tmp).name, "maia-1500.pb.gz")

    def test_run_skips_cleanly_with_a_reason(self):
        argv = ["--predictor", "model.eval.fakes:perfect", "--label", "s", "--rows", "6", "--no-stockfish",
                "--no-tactics", "--timeout-s", "1", "--corpus", str(make_corpus(self.tmp / "c")),
                "--out-root", str(self.tmp / "out"), "--oracles", "lc0", "maia", "--explain"]
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()) as err:
            out = run.main(argv)
        axes = json.loads((out / "scorecard.json").read_text())["axes"]
        self.assertEqual(axes["lc0_strength"], {"status": "skipped", "reason": "lc0 binary not found (set LC0_PATH or --lc0-path)"})
        self.assertEqual(axes["maia_human_likeness"]["status"], "skipped")
        self.assertIn("lc0 binary not found", axes["maia_human_likeness"]["reason"])
        self.assertEqual(axes["explanation"]["status"], "skipped")  # fakes:perfect has no explain()
        self.assertIn("no explain(packet)", axes["explanation"]["reason"])
        self.assertIn("oracle lc0 skipped", err.getvalue())
        md = (out / "scorecard.md").read_text()
        self.assertIn("Skipped: lc0 binary not found", md)
        # the other axes still score
        self.assertEqual(axes["raw_legality"]["legal"], 6)

    def test_explanations_file_is_scored_without_a_predictor_method(self):
        explanations = self.tmp / "e.jsonl"
        explanations.write_text("".join(json.dumps({"fen": r["packet"]["fen"], "text": "The queen on d4."}) + "\n"
                                        for r in ROWS[:5]))  # row 5 deliberately missing
        argv = ["--predictor", "model.eval.fakes:perfect", "--label", "f", "--rows", "6", "--no-stockfish",
                "--no-tactics", "--timeout-s", "1", "--corpus", str(make_corpus(self.tmp / "c")),
                "--out-root", str(self.tmp / "out"), "--explanations", str(explanations)]
        with contextlib.redirect_stdout(io.StringIO()):
            out = run.main(argv)
        expl = json.loads((out / "scorecard.json").read_text())["axes"]["explanation"]
        self.assertEqual(expl["explanations_missing"], 1)
        self.assertEqual(expl["n"], 6)
        self.assertEqual(expl["source"], "file")


INFERENCE_PY = ("model/serve", "model/prompt.py")
INFERENCE_CRATES = ("knightlm-core", "knightlm-model", "knightlm-player", "knightlm-notation", "rbc-ai")
ORACLE_CRATES = ("knightlm-engine", "knightlm-play")
ORACLE_MODULES = ("model.eval", "chess.engine")


def forbidden_python_imports(path):
    """Imports of the evaluator or an engine client in one Python file."""
    bad = []
    for node in ast.walk(ast.parse(Path(path).read_text())):
        names = []
        if isinstance(node, ast.Import):
            names = [a.name for a in node.names]
        elif isinstance(node, ast.ImportFrom):
            names = [node.module or ""]
            if node.module in ("model", "chess"):
                names += [f"{node.module}.{a.name}" for a in node.names]
        bad += [n for n in names if any(n == m or n.startswith(m + ".") for m in ORACLE_MODULES)]
    return bad


def crate_graph():
    graph = {}
    for manifest in (REPO / "crates").glob("*/Cargo.toml"):
        data = tomllib.loads(manifest.read_text())
        deps = {}
        for table in ("dependencies", "dev-dependencies", "build-dependencies"):
            deps.update(data.get(table) or {})
        graph[data["package"]["name"]] = (manifest.parent, set(deps))
    return graph


def reachable(graph, roots):
    seen, stack = set(), list(roots)
    while stack:
        name = stack.pop()
        if name in seen or name not in graph:
            continue
        seen.add(name)
        stack.extend(graph[name][1])
    return seen


class EngineZeroTests(unittest.TestCase):
    """KLM-WAR-0005: oracles are used only by the evaluator, after the model has committed."""

    def test_inference_python_imports_no_oracle_or_evaluator(self):
        files = []
        for entry in INFERENCE_PY:
            path = REPO / entry
            files += sorted(path.rglob("*.py")) if path.is_dir() else [path]
        self.assertGreaterEqual(len(files), 3)
        for file in files:
            if file.name.startswith("test_"):
                continue
            with self.subTest(str(file.relative_to(REPO))):
                self.assertEqual(forbidden_python_imports(file), [])

    def test_the_scan_catches_a_planted_import(self):
        with tempfile.TemporaryDirectory() as tmp:
            for line in ("import model.eval.oracles", "from model.eval import oracles", "from model.eval.oracles import x",
                         "import chess.engine", "from chess import engine"):
                file = Path(tmp) / "planted.py"
                file.write_text(line + "\n")
                self.assertTrue(forbidden_python_imports(file), line)
            file.write_text("import chess\nfrom model import prompt\nimport json\n")
            self.assertEqual(forbidden_python_imports(file), [])

    def test_inference_crates_do_not_reach_the_oracle_crates(self):
        graph = crate_graph()
        for name in INFERENCE_CRATES + ORACLE_CRATES:
            self.assertIn(name, graph)
        closure = reachable(graph, INFERENCE_CRATES)
        self.assertEqual(closure & set(ORACLE_CRATES), set(), "an inference crate depends on an oracle crate")
        for name in closure:
            for source in graph[name][0].rglob("*.rs"):
                text = source.read_text()
                for oracle in ("knightlm_engine", "knightlm_play"):
                    self.assertNotIn(oracle, text, f"{source} uses {oracle}")

    def test_the_closure_check_would_catch_a_planted_dependency(self):
        graph = crate_graph()
        graph["knightlm-player"] = (graph["knightlm-player"][0], graph["knightlm-player"][1] | {"knightlm-engine"})
        self.assertTrue(reachable(graph, INFERENCE_CRATES) & set(ORACLE_CRATES))

    def test_the_oracle_modules_are_only_imported_by_the_evaluator(self):
        importers = []
        for file in (REPO / "model").rglob("*.py"):
            if "oracles" in file.read_text() and "import" in file.read_text():
                importers.append(file.relative_to(REPO).parts[:2])
        self.assertTrue(all(parts[:2] == ("model", "eval") for parts in importers), importers)


if __name__ == "__main__":
    unittest.main()
