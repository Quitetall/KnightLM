"""Reasoning v2 (text and tool-quoting variants) and the rejection-sampling loop, with fake predictors."""

import json
from pathlib import Path
import re
import tempfile
import unittest
from unittest import mock

from model import prompt
from model.corpus import pipeline, reasoning, reasoning2 as r2, rejection
from model.corpus.test_reasoning import BANNED, FIXTURES, walk
from model.corpus.test_s0 import CURRENT, SOURCE, split_rows
from model.eval import explain

BINARY = prompt.knightlm_binary()
PARENT = {"source": SOURCE, "split": "fixture split"}


def fixture_corpus(root, variant):
    train_src, holdout_src = split_rows()
    train = r2.build_rows(BINARY, train_src, "train", variant)
    holdout = r2.build_rows(BINARY, holdout_src, "holdout", variant)
    out = Path(root) / f"r2{variant}"
    r2.write_corpus(out, train, holdout, variant, BINARY, CURRENT, PARENT, {"corpus": "fixture"}, 10)
    return out, train, holdout


def pseudo_row(fen, facts, uci):
    packet = {"schema_version": 1, "ruleset": "standard-untimed-v1", "history_complete": True, "fen": fen,
              "legal_moves": facts["legal_moves"]}
    return {"family": "A0", "game_id": "g", "packet": packet, "ply": 24, "position_key": " ".join(fen.split()[:4]),
            "source": "s", "target": uci, "utc_date": "2026.09.01", "utc_time": "00:00:00"}


class Generated(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.positions = walk(games=50, plies=70)
        cls.built = {}
        facts = [p[1] for p in cls.positions]
        ucis = [p[2] for p in cls.positions]
        for variant in "ab":
            cls.built[variant] = r2.derive(BINARY, facts, ucis, variant)

    def test_zero_failed_claims_and_nothing_unchecked(self):
        self.assertGreaterEqual(len(self.positions), 1500)
        for variant in "ab":
            results = []
            for (fen, facts, uci), (plan, calls, rendered, text) in zip(self.positions, self.built[variant], strict=True):
                result = explain.check_explanation(fen, facts["legal_moves"], text, uci)
                self.assertEqual(result["failures"], [], (variant, fen, uci, text))
                results.append(result)
            card = explain.aggregate(results)
            self.assertEqual(card["failed_total"], 0)
            self.assertEqual(card["explanations_without_checkable_claims"], 0)
            self.assertEqual(sum(card["unchecked_claims"].values()), 0)
            for kind in ("piece_on_square", "move_legal", "material", "capture_target", "move_check"):
                self.assertGreater(card["claim_types"][kind]["checkable"], 0, (variant, kind))

    def test_short_fixed_and_free_of_judgement(self):
        for variant in "ab":
            texts = [t for *_, t in self.built[variant]]
            for (fen, facts, uci), text in zip(self.positions, texts, strict=True):
                self.assertLessEqual(len(text), r2.LIMITS[variant]["chars"], text)
                self.assertLessEqual(len(text.split()), r2.LIMITS[variant]["words"], text)
                self.assertIsNone(BANNED.search(text), text)
                self.assertIn(uci, text)
                self.assertNotRegex(text, r"\d+ legal|\bcaptures\b.*\bchecks\b")  # no unchecked counts
            self.assertLess(sum(map(len, texts)) / len(texts), 330)
        v1 = [len(reasoning.reasoning_text(f, u)) for _, f, u in self.positions[:300]]
        a = [len(t) for *_, t in self.built["a"][:300]]
        self.assertLess(sum(a), sum(v1))  # shorter than reasoning-v1

    def test_facts_cover_the_planned_kinds_and_zero_to_three_per_row(self):
        kinds, counts = set(), set()
        for plan, *_ in self.built["a"]:
            counts.add(len(plan))
            kinds |= {f["kind"] for f in plan}
        self.assertEqual(counts, {0, 1, 2, 3})
        self.assertEqual(kinds, {"victim", "hanging", "loose"})

    def test_variant_b_uses_every_read_tool_and_results_are_the_compilers(self):
        used = set()
        for (fen, facts, uci), (plan, calls, rendered, text) in zip(self.positions, self.built["b"], strict=True):
            used |= {n for n, _ in calls}
            self.assertLessEqual(len(calls), r2.MAX_READS)
            self.assertEqual(r2.compute_reads(BINARY, [{"fen": fen, "calls": calls}])[0], rendered)
            for number, text_ in enumerate(rendered, 1):
                self.assertTrue(text_.startswith(f'<tool_result>{{"call":{number},"ok":true'), text_)
        self.assertEqual(used, set(r2.READ_TOOLS))

    def test_transcript_round_trips_and_masks_results(self):
        for (fen, facts, uci), (plan, calls, rendered, text) in list(zip(self.positions, self.built["b"], strict=True))[:200]:
            completion, segments = r2.transcript(calls, rendered, text, uci)
            reads, final = r2.parse_transcript(completion)
            self.assertEqual(final, {"reasoning": text, "move": uci})
            self.assertEqual([(n, a) for n, a, _ in reads], [(n, a) for n, a in calls])
            self.assertEqual("".join(s["text"] for s in segments), completion)
            self.assertEqual([s["loss"] for s in segments], [True, False] * len(calls) + [True])
            self.assertEqual(rejection.segments_of(completion), segments)
            r2.check_quotes(text, reads)
            # the model-written spans contain no tool result
            self.assertFalse(any("<tool_result>" in s["text"] for s in segments if s["loss"]))

    def test_independent_oracle_catches_a_wrong_compiler_answer(self):
        fen, facts, uci = next(p for p in self.positions if r2.plan_facts(p[1]))
        with mock.patch.object(r2, "attackers_py", return_value=["a1"]), mock.patch.object(r2, "defenders_py", return_value=["a1"]):
            with self.assertRaisesRegex(ValueError, "independent"):
                r2.derive(BINARY, [facts], [uci], "a")

    def test_fixture_positions_castle_en_passant_promotion_check(self):
        text = {(f, m): t for (f, _, m), (_, _, _, t) in zip(self.positions, self.built["a"], strict=True)}
        for fen, move in FIXTURES:
            self.assertIn(move, text[(fen, move)])
        joined = " ".join(text.values())
        for needle in ("castling short", "en passant", "promoting to", "giving check", "is in check", "undefended", "defended by",
                       "is attacked by", "is ahead by"):
            self.assertIn(needle, joined, needle)

    def test_a_row_with_a_failed_claim_is_refused(self):
        fen, facts, uci = self.positions[10]
        row = pseudo_row(fen, facts, uci)
        plan, calls, rendered, text = self.built["a"][10]
        self.assertTrue(r2.make_row(0, "train", row, "a", plan, calls, rendered, text))
        bad = text.replace("I choose", "The knight on a1 stays. I choose", 1)
        self.assertIn("a1", bad)
        with self.assertRaisesRegex(ValueError, "failed of"):
            r2.make_row(0, "train", row, "a", plan, calls, rendered, bad)


class Corpus(unittest.TestCase):
    def test_build_read_audit_and_tamper(self):
        for variant in "ab":
            with tempfile.TemporaryDirectory() as tmp:
                out, train, holdout = fixture_corpus(tmp, variant)
                train2, holdout2 = r2.read_corpus(out, current=CURRENT)
                self.assertEqual((train, holdout), (train2, holdout2))
                for rows in (train2, holdout2):
                    r2.verify_rows(rows, BINARY, variant)
                manifest = json.loads((out / "manifest.json").read_text())
                self.assertEqual(manifest["claims"]["train"]["failed_claims"], 0)
                self.assertEqual(manifest["claims"]["holdout"]["rows_with_a_failed_claim"], 0)
                self.assertEqual(manifest["rules"]["core_tree"], "aaa")
                self.assertEqual(manifest["generator"]["version"], r2.GENERATOR_VERSION)
                # a text edit that keeps every check passing is still caught by re-derivation
                row = json.loads(json.dumps(train2[0]))
                row["reasoning"] = row["reasoning"].replace("to move.", "to move now.", 1)
                row["completion"] = row["completion"].replace("to move.", "to move now.", 1)
                if variant == "b":
                    for segment in row["segments"]:
                        segment["text"] = segment["text"].replace("to move.", "to move now.", 1)
                with self.assertRaisesRegex(ValueError, "differs from the compiler"):
                    r2.verify_rows([row], BINARY, variant)

    def test_a_planted_false_claim_in_a_shard_is_refused_by_the_audit(self):
        with tempfile.TemporaryDirectory() as tmp:
            out, train, _ = fixture_corpus(tmp, "a")
            bad = json.loads(json.dumps(train[0]))
            bad["claims"]["failed"] = 1
            with self.assertRaisesRegex(ValueError, "invalid reasoning-v2 row"):
                r2.validate_rows([bad], bad["source"], "a")
            lie = json.loads(json.dumps(train[0]))
            lie["reasoning"] = "The knight on a1 is undefended. " + lie["reasoning"]
            lie["completion"] = prompt.format_play_completion(lie["reasoning"], lie["move"])
            with self.assertRaisesRegex(ValueError, "differs from the compiler"):
                r2.verify_rows([lie], BINARY, "a")

    def test_unchanged_prompt_and_move_are_the_played_move(self):
        with tempfile.TemporaryDirectory() as tmp:
            _, train, holdout = fixture_corpus(tmp, "b")
        for row in train + holdout:
            self.assertEqual(row["prompt"], prompt.play_text(row["packet"]))
            self.assertEqual(row["move"], row["target"])
            self.assertIn(row["move"], row["packet"]["legal_moves"])
            self.assertEqual(row["claims"]["failed"], 0)

    def test_variants_share_positions_and_moves(self):
        with tempfile.TemporaryDirectory() as tmp:
            _, ta, ha = fixture_corpus(tmp, "a")
            _, tb, hb = fixture_corpus(tmp, "b")
        self.assertEqual(r2.matched_sha256(ta, ha), r2.matched_sha256(tb, hb))
        for a, b in zip(ta + ha, tb + hb, strict=True):
            self.assertEqual((a["packet"], a["move"]), (b["packet"], b["move"]))
            short, long = sorted((a["facts"], b["facts"]), key=len)
            self.assertEqual(long[: len(short)], short)  # the two limits trim the same priority list

    def test_trainer_readers_accept_the_format(self):
        from model.tests import _payload  # noqa: F401
        from knightlm_blut import materialize

        for variant in "ab":
            with tempfile.TemporaryDirectory() as tmp:
                out, train, _ = fixture_corpus(tmp, variant)
                self.assertEqual(pipeline.read_corpus(out, allow_rules_drift=True)[0], train)
                for row in train:
                    self.assertEqual(materialize.convert_row(row, prompt), {"prompt": row["prompt"], "completion": row["completion"]})

    def test_rules_drift_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            out, _, _ = fixture_corpus(tmp, "a")
            with self.assertRaisesRegex(ValueError, "rules revision differs"):
                r2.read_corpus(out, current={**CURRENT, "core_tree": "zzz"})
            r2.read_corpus(out, allow_rules_drift=True)


class FakePredictors(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        cls.rows = {}
        for variant in "ab":
            _, train, _ = fixture_corpus(cls.tmp.name, variant)
            cls.rows[variant] = train

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def run_with(self, variant, make, n=4, **kwargs):
        rows = self.rows[variant]
        return rejection.run(rows, lambda row, count: make(row)[:count], n, variant, BINARY, **kwargs)

    def test_good_samples_are_kept_once_and_carry_claims(self):
        for variant in "ab":
            kept, stats = self.run_with(variant, lambda row: [row["completion"], row["completion"]])
            self.assertEqual((stats["kept"], stats["duplicates"], stats["rows_with_a_kept_sample"]),
                             (len(self.rows[variant]),) * 3)
            self.assertEqual(sum(stats["rejected"].values()), 0)
            for row, source in zip(kept, self.rows[variant], strict=True):
                self.assertEqual(row["origin"], "rejection_sampling")
                self.assertEqual(row["claims"]["failed"], 0)
                self.assertEqual(row["completion"], source["completion"])
                self.assertEqual(row["move"], source["target"])
                self.assertEqual(row.get("segments"), source.get("segments"))

    def test_each_failure_is_rejected_with_its_reason(self):
        a = self.rows["a"][0]
        legal = a["packet"]["legal_moves"]
        other = next(m for m in legal if m != a["target"])
        play = prompt.format_play_completion
        text = a["reasoning"]
        cases = {
            "format": ["not a call", play("", a["target"]), text],
            "illegal_move": [play(text, "a1a1")],
            "failed_claim": [play("The knight on a1 is undefended. " + text, a["target"])],
            "no_checkable_claim": [play("A calm position.", a["target"])],
            "move_disagrees": [play(text.replace(a["target"], other), other)],
        }
        for reason, completions in cases.items():
            if reason == "format":
                completions = completions[:2]
            kept, stats = self.run_with("a", lambda row, c=completions: c if row is a else [], n=3)
            self.assertEqual(kept, [], reason)
            self.assertEqual(stats["rejected"][reason], len(completions), (reason, stats["rejected"]))
        unchecked = play("White to move. After the exchange then the queen on d1 is undefended. " + text, a["target"])
        kept, stats = self.run_with("a", lambda row: [unchecked] if row is a else [], allow_unchecked=False)
        self.assertEqual(stats["rejected"]["unchecked_claim"] + stats["rejected"]["failed_claim"], 1)

    def test_chosen_move_from_a_policy_head_replaces_the_played_move(self):
        a = self.rows["a"][0]
        other = next(m for m in a["packet"]["legal_moves"] if m != a["target"])
        done = prompt.format_play_completion(a["reasoning"].replace(a["target"], other), other)
        kept, stats = rejection.run([a], lambda row, n: [done], 1, "a", BINARY, chosen=lambda row: other)
        self.assertEqual(stats["kept"], 1, stats)
        kept, stats = rejection.run([a], lambda row, n: [done], 1, "a", BINARY)
        self.assertEqual(stats["rejected"]["move_disagrees"], 1)

    def test_variant_b_rejects_tampered_results_misquotes_and_other_tools(self):
        b = next(r for r in self.rows["b"] if re.search(r"\((defenders|attackers)_of", r["reasoning"]))
        text = b["completion"]
        first_result = re.search(r"<tool_result>.*?</tool_result>", text).group(0)
        tampered = text.replace(first_result, first_result.replace('"ok":true', '"ok":true,"score":1'), 1)
        wrong_square = re.sub(r"\((defenders_of|attackers_of) ([a-h][1-8]): ([^)]*)\)",
                              lambda m: f"({m.group(1)} {m.group(2)}: a1)", b["reasoning"], count=1)
        misquote = text.replace(b["reasoning"], wrong_square)
        engine = text.replace('"name":"' + b["reads"][0]["name"] + '"', '"name":"analyze"', 1)
        two_finals = text + text[text.index("<tool_call>{\"name\":\"submit_move"):]
        no_reads = prompt.format_play_completion(b["reasoning"], b["target"])
        cases = {"results_differ": tampered, "misquote": misquote, "format": [engine, two_finals, no_reads]}
        for reason, bad in cases.items():
            for completion in bad if isinstance(bad, list) else [bad]:
                kept, stats = self.run_with("b", lambda row, c=completion: [c] if row is b else [], n=1)
                self.assertEqual((len(kept), stats["rejected"][reason]), (0, 1), (reason, stats["rejected"]))

    def test_pass_rate_statistics_over_a_mixed_predictor(self):
        rows = self.rows["a"]
        play = prompt.format_play_completion

        def predictor(row, n):
            return [row["completion"], play("The knight on a1 is undefended. " + row["reasoning"], row["target"]), "oops"][:n]

        kept, stats = rejection.run(rows, predictor, 3, "a", BINARY)
        self.assertEqual(stats["samples"], 3 * len(rows))
        self.assertEqual(stats["kept"], len(rows))
        self.assertEqual(stats["rejected"]["failed_claim"], len(rows))
        self.assertEqual(stats["rejected"]["format"], len(rows))
        self.assertEqual(stats["kept_per_row"], {1: len(rows)})
        for row in kept:
            self.assertEqual(explain.check_explanation(row["packet"]["fen"], row["packet"]["legal_moves"],
                                                       row["reasoning"], row["move"])["failed_total"], 0)


class Cli(unittest.TestCase):
    def test_cli_runs_a_predictor_by_name_and_refuses_to_write_holdout(self):
        with tempfile.TemporaryDirectory() as tmp:
            out, _, _ = fixture_corpus(tmp, "a")
            kept, stats = Path(tmp) / "kept.jsonl", Path(tmp) / "stats.json"
            rejection.main(["run", "--corpus", str(out), "--predictor", "model.corpus.rejection:reference_predictor",
                            "--allow-rules-drift", "--n", "2", "--out", str(kept), "--stats", str(stats), "--knightlm", str(BINARY)])
            lines = kept.read_text().splitlines()
            self.assertEqual(len(lines), json.loads(stats.read_text())["kept"])
            self.assertTrue(lines)
            with self.assertRaises(SystemExit):
                rejection.main(["run", "--corpus", str(out), "--predictor", "model.corpus.rejection:reference_predictor",
                                "--allow-rules-drift", "--split", "holdout", "--out", str(Path(tmp) / "x.jsonl")])


if __name__ == "__main__":
    unittest.main()
