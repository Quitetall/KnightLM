"""S0 rules-QA corpus: matched arms, oracle independence, split audit, rules revision, tamper refusal."""

import copy
import json
import tempfile
import unittest
from pathlib import Path

from model import prompt
from model.corpus import pipeline, s0
from model.eval import explain
from model.eval.fakes import fixture_rows

BINARY = prompt.knightlm_binary()
CURRENT = {"commit": "c0ffee", "core_tree": "aaa", "notation_tree": "bbb", "dirty": False}
SOURCE = {"file": fixture_rows()[0]["source"], "bytes": 1, "sha256": "0" * 64}


def split_rows():
    rows = fixture_rows()
    holdout = [dict(r, utc_date="2026.09.02") for r in rows[:3]]
    train = [dict(r, family="Z" + str(i), game_id=f"train{i}", utc_date="2026.09.01", utc_time="00:00:0" + str(i))
             for i, r in enumerate(rows[3:])]
    return train, holdout


def make_corpus(root, arm="t0", mutate=None):
    train_src, holdout_src = split_rows()
    train = s0.build_rows(BINARY, train_src, "train", arm)
    holdout = s0.build_rows(BINARY, holdout_src, "holdout", arm)
    if mutate:
        mutate(train, holdout)
    out = Path(root) / f"s0-{arm}"
    s0.write_corpus(out, train, holdout, arm, BINARY, CURRENT, SOURCE, {"corpus": "fixture"}, "fixture split", 10)
    return out, train, holdout


class MatchedArms(unittest.TestCase):
    def test_arms_share_ids_questions_answers_and_differ_in_prompt(self):
        with tempfile.TemporaryDirectory() as tmp:
            _, t0, h0 = make_corpus(tmp, "t0")
            _, t2, h2 = make_corpus(tmp, "t2")
        for a, b in zip(t0 + h0, t2 + h2, strict=True):
            self.assertEqual((a["id"], a["qa"]), (b["id"], b["qa"]))
            self.assertEqual(a["completion"], b["completion"])
            self.assertNotEqual(a["prompt"], b["prompt"])
            self.assertIn(prompt.packet_json(a["packet"]), a["prompt"])
            self.assertNotIn("cgt/1", a["prompt"])
            self.assertIn("cgt/1 lite", b["prompt"])
            self.assertNotIn(a["packet"]["fen"], b["prompt"])
        self.assertEqual(s0.matched_sha256(t0, h0), s0.matched_sha256(t2, h2))

    def test_every_arm_shares_one_matched_hash_and_the_neutral_move(self):
        corpora = {}
        with tempfile.TemporaryDirectory() as tmp:
            for arm in s0.ARMS:
                _, train, holdout = make_corpus(tmp, arm)
                corpora[arm] = (train, holdout)
        hashes = {arm: s0.matched_sha256(*rows) for arm, rows in corpora.items()}
        self.assertEqual(len(set(hashes.values())), 1, hashes)
        self.assertEqual({"play", "play-cgt", "play-cgt-full", "play-t1", "play-t3", "play-t3-full"},
                         {rows[0][0]["prompt_kind"] for rows in corpora.values()})
        full = corpora["t2-full"][0][0]["prompt"]
        self.assertIn("atk:", full)
        self.assertNotIn("atk:", corpora["t2"][0][0]["prompt"])

    def test_completion_move_is_neutral_and_never_the_human_move(self):
        with tempfile.TemporaryDirectory() as tmp:
            _, train, holdout = make_corpus(tmp, "t0")
        differs = 0
        for row in train + holdout:
            self.assertEqual(row["move"], min(row["packet"]["legal_moves"]))
            self.assertEqual(prompt.parse_play_completion(row["completion"])["move"], row["move"])
            differs += row["move"] != row["target"]
        self.assertGreater(differs, 0, "fixture never exercises a human move that differs from the neutral move")

    def test_a_completion_carrying_the_human_move_is_refused(self):
        def swap(train, holdout):
            for row in train:
                if row["move"] != row["target"]:
                    row["completion"] = prompt.format_play_completion(row["qa"]["answer"], row["target"])
                    return

        with tempfile.TemporaryDirectory() as tmp:
            out, _, _ = make_corpus(tmp, mutate=swap)
            with self.assertRaisesRegex(ValueError, "is not the play call"):
                s0.read_corpus(out, current=CURRENT)

    def test_legacy_v1_corpus_still_reads_and_audits(self):
        train_src, holdout_src = split_rows()
        train = [s0.make_row(i, "train", r, s0.build_rows(BINARY, [r], "train", "t0")[0]["qa"], "t0", BINARY, s0.LEGACY_FORMAT)
                 for i, r in enumerate(train_src)]
        holdout = [s0.make_row(i, "holdout", r, s0.build_rows(BINARY, [r], "holdout", "t0")[0]["qa"], "t0", BINARY, s0.LEGACY_FORMAT)
                   for i, r in enumerate(holdout_src)]
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "legacy"
            s0.write_corpus(out, train, holdout, "t0", BINARY, CURRENT, SOURCE, {"corpus": "fixture"}, "fixture", 10)
            manifest = json.loads((out / pipeline.MANIFEST).read_text())
            manifest["format"] = s0.LEGACY_FORMAT
            (out / pipeline.MANIFEST).write_text(json.dumps(manifest))
            read_train, read_holdout = s0.read_corpus(out, current=CURRENT)
            s0.verify_rows(read_train + read_holdout, BINARY, "t0", s0.LEGACY_FORMAT)
        self.assertTrue(all("move" not in r and prompt.parse_play_completion(r["completion"])["move"] == r["target"]
                            for r in read_train))

    def test_prompts_are_the_shared_renderer(self):
        with tempfile.TemporaryDirectory() as tmp:
            _, train, _ = make_corpus(tmp, "t2")
        for row in train:
            self.assertEqual(row["prompt"], prompt.qa_prompt("play-cgt", row["packet"], row["qa"]["question"]))
            self.assertTrue(row["prompt"].startswith(prompt.play_prompt("play-cgt", row["packet"])))


class OracleIndependence(unittest.TestCase):
    def test_answers_agree_with_an_independent_board_reader(self):
        rows = fixture_rows() * 1
        built = s0.build_rows(BINARY, rows, "holdout", "t0")
        for row in built:
            board, qa = explain.Board(row["packet"]["fen"]), row["qa"]
            if qa["type"] == "piece_on_square":
                result = explain.check_explanation(row["packet"]["fen"], row["packet"]["legal_moves"], qa["answer"])
                self.assertEqual(result["failed_total"], 0, qa)
            if qa["type"] == "side_to_move":
                self.assertEqual(qa["answer"], ("White" if board.turn == "w" else "Black") + " to move.")
            if qa["type"] == "legal_moves_of":
                moves = qa["answer"].removeprefix("Legal moves: ").split()
                self.assertTrue(set(moves) <= set(row["packet"]["legal_moves"]))
                self.assertEqual({m[:2] for m in moves}.__len__(), 1)

    def test_every_type_builds_and_matches_the_independent_reader(self):
        rows = fixture_rows()
        for row, (cgt, facts) in zip(rows, s0.compile_inputs(BINARY, rows), strict=True):
            board = explain.Board(row["packet"]["fen"])
            for kind in s0.TYPES:
                qa = s0._make(kind, row["packet"]["fen"], row["packet"], cgt, facts, cgt["board"], board.turn == "w")
                self.assertEqual(qa["type"], kind)
                self.assertLessEqual(len(qa["answer"]), s0.MAX_ANSWER_CHARS)
                if kind == "in_check":
                    self.assertEqual(qa["answer"].startswith("Yes"), facts["in_check"])
                if kind == "captures":
                    self.assertTrue(qa["answer"].startswith(f"Captures: {facts['legal_capture_count']}"))
                if kind == "material":
                    self.assertIn(f"White {facts['points']['white']}, black {facts['points']['black']}.", qa["answer"])


class Audit(unittest.TestCase):
    def test_clean_corpus_is_accepted_and_rederived(self):
        with tempfile.TemporaryDirectory() as tmp:
            out, _, _ = make_corpus(tmp)
            train, holdout = s0.read_corpus(out, current=CURRENT)
            s0.verify_rows(train + holdout, BINARY, "t0")

    def test_rules_revision_drift_is_refused_unless_allowed(self):
        with tempfile.TemporaryDirectory() as tmp:
            out, _, _ = make_corpus(tmp)
            moved = dict(CURRENT, core_tree="different")
            with self.assertRaisesRegex(ValueError, "rules revision differs"):
                s0.read_corpus(out, current=moved)
            s0.read_corpus(out, allow_rules_drift=True, current=moved)
            with self.assertRaisesRegex(ValueError, "uncommitted"):
                s0.read_corpus(out, current=dict(CURRENT, dirty=True))

    def test_manifest_records_revision_and_binary_hash(self):
        with tempfile.TemporaryDirectory() as tmp:
            out, _, _ = make_corpus(tmp)
            manifest = json.loads((out / "manifest.json").read_text())
        self.assertEqual(manifest["rules"]["core_tree"], "aaa")
        self.assertEqual(manifest["rules"]["binary_sha256"], pipeline.sha256_file(BINARY))
        self.assertEqual(manifest["compiler_sha256"], manifest["rules"]["binary_sha256"])

    def test_planted_position_leak_is_refused(self):
        def leak(train, holdout):
            train[0]["position_key"] = holdout[0]["position_key"]

        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaisesRegex(ValueError, "leakage|invalid"):
                out, _, _ = make_corpus(tmp, mutate=leak)
                s0.read_corpus(out, current=CURRENT)

    def test_planted_game_leak_is_refused(self):
        def leak(train, holdout):
            train[0]["game_id"] = holdout[0]["game_id"]

        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaisesRegex(ValueError, "leakage"):
                make_corpus(tmp, mutate=leak)

    def test_tampered_answer_is_refused_by_rederivation(self):
        def tamper(train, holdout):
            row = train[0]
            row["qa"]["answer"] = row["qa"]["answer"] + " x"
            row["completion"] = prompt.format_play_completion(row["qa"]["answer"], row["move"])

        with tempfile.TemporaryDirectory() as tmp:
            out, _, _ = make_corpus(tmp, mutate=tamper)
            train, holdout = s0.read_corpus(out, current=CURRENT)  # shape is fine
            with self.assertRaisesRegex(ValueError, "differs from the rules derivation"):
                s0.verify_rows(train, BINARY, "t0")

    def test_tampered_prompt_is_refused_by_rederivation(self):
        def tamper(train, holdout):
            train[0]["prompt"] += " "

        with tempfile.TemporaryDirectory() as tmp:
            out, _, _ = make_corpus(tmp, mutate=tamper)
            train, _ = s0.read_corpus(out, current=CURRENT)
            with self.assertRaisesRegex(ValueError, "prompt or completion differs"):
                s0.verify_rows(train, BINARY, "t0")

    def test_shard_edit_is_a_checksum_failure(self):
        with tempfile.TemporaryDirectory() as tmp:
            out, _, _ = make_corpus(tmp)
            shard = out / "train-00000.jsonl"
            shard.write_text(shard.read_text().replace('"qa"', '"qa"', 1) + "\n")
            with self.assertRaisesRegex(ValueError, "checksum"):
                s0.read_corpus(out, current=CURRENT)

    def test_pipeline_dispatches_s0_with_the_drift_flag(self):
        with tempfile.TemporaryDirectory() as tmp:
            out, _, _ = make_corpus(tmp)
            with self.assertRaisesRegex(ValueError, "rules revision differs"):
                pipeline.read_corpus(out)  # the fixture revision is not this checkout's
            pipeline.read_corpus(out, allow_rules_drift=True)


class MaterializeAndTrain(unittest.TestCase):
    def test_convert_row_and_preflight_parity_use_the_stored_prompt(self):
        import sys
        sys.path.insert(0, str(Path(__file__).parents[2] / "apps/blut-knightlm/python"))
        from knightlm_blut import materialize
        from model.train import run as train_run

        with tempfile.TemporaryDirectory() as tmp:
            _, train, _ = make_corpus(tmp, "t2")
        record = materialize.convert_row(train[0], prompt)
        self.assertEqual(record, {"prompt": train[0]["prompt"], "completion": train[0]["completion"]})
        train_run.check_prompt_parity(train)
        broken = copy.deepcopy(train)
        broken[0]["prompt"] += "x"
        with self.assertRaises(train_run.Refused):
            train_run.check_prompt_parity(broken)


if __name__ == "__main__":
    unittest.main()
