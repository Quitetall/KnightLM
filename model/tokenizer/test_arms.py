"""Arms T1/T3 prompts: the ctok position block tokenises to exactly the id-level encoding, and the trainer's
and the server's tokenisation of an arm prompt are the same ids (no GPU, no training)."""

import copy
import unittest
from pathlib import Path

from model import prompt
from model.eval.fakes import fixture_rows
from model.tokenizer import arms, cgt, hf, position, vocab
from model.tokenizer.errors import TokenizerError

try:
    import torch  # noqa: F401
    from model.tokenizer.test_hf import REAL_TOKENIZER, host_tokenizer
except ImportError:  # pragma: no cover
    host_tokenizer = None

try:
    BINARY = prompt.knightlm_binary()
except FileNotFoundError:  # pragma: no cover
    BINARY = None

PACKETS = [r["packet"] for r in fixture_rows()[:6]]


@unittest.skipIf(BINARY is None, "knightlm binary not built")
class Strings(unittest.TestCase):
    def test_text_form_round_trips_to_the_id_form(self):
        for kind in arms.KINDS:
            for packet in PACKETS:
                text = cgt_for(kind, packet)
                ids = arms.position_ids(kind, packet, text)
                self.assertEqual(arms.tokens_to_ids(arms.ids_to_text(ids)), ids)

    def test_t1_is_the_position_record_and_t3_the_compiler_text(self):
        for packet in PACKETS:
            self.assertEqual(position.decode(arms.position_ids("play-t1", packet)),
                             position.packet_to_record(packet) | {"repetition_count": packet["facts"].get("repetition_count")})
            for kind, variant in arms.CGT_VARIANT.items():
                text = prompt.cgt_text(packet, variant)
                self.assertEqual(cgt.decode(arms.position_ids(kind, packet, text)), text)

    def test_prompt_is_the_play_prefix_label_and_position(self):
        for kind in arms.KINDS:
            packet = PACKETS[0]
            text = prompt.play_prompt(kind, packet)
            self.assertTrue(text.startswith(prompt.prefix("play") + arms.LABEL + "<pos>" if kind == "play-t1"
                                            else prompt.prefix("play") + arms.LABEL + "<cgt/1>"))
            self.assertNotIn(packet["fen"], text)
            self.assertEqual(prompt.qa_prompt(kind, packet, "Whose turn is it?"), text + "\nQuestion: Whose turn is it?")

    def test_stray_text_in_a_position_block_is_refused(self):
        for bad in ("<pos> x", "<nope>", "<pos", "text"):
            with self.assertRaises(TokenizerError):
                arms.tokens_to_ids(bad)
        with self.assertRaises(TokenizerError):
            arms.position_ids("play-t3", PACKETS[0])  # no compiler text given
        with self.assertRaises(TokenizerError):
            arms.position_ids("play", PACKETS[0])

    def test_play_rows_render_and_materialize_in_every_arm(self):
        import sys
        sys.path.insert(0, str(Path(__file__).parents[2] / "apps/blut-knightlm/python"))
        from knightlm_blut import materialize
        from model.corpus import render

        row = {"packet": PACKETS[0], "reasoning": "White to move.", "target": PACKETS[0]["legal_moves"][0]}
        for kind in prompt.PLAY_KINDS:
            pair = materialize.convert_row(row, prompt, render, kind)
            self.assertEqual(pair["prompt"], prompt.play_prompt(kind, PACKETS[0]))
            self.assertEqual(pair["completion"], prompt.format_play_completion("White to move.", row["target"]))
        self.assertEqual(materialize.convert_row(row, prompt, render)["prompt"], prompt.play_text(PACKETS[0]))

    def test_replies_parse_with_the_ordinary_play_parser(self):
        reply = prompt.format_play_completion("White to move.", "e2e4")
        self.assertEqual(prompt.parse_play_completion(reply)["move"], "e2e4")


def cgt_for(kind, packet):
    return prompt.cgt_text(packet, arms.CGT_VARIANT[kind]) if kind in arms.CGT_VARIANT else None


@unittest.skipIf(BINARY is None or host_tokenizer is None, "needs the binary and torch/transformers")
class Tokenisation(unittest.TestCase):
    def check(self, tokenizer):
        base = hf.extend_tokenizer(tokenizer)
        for kind in arms.KINDS:
            for packet in PACKETS:
                ids = arms.position_ids(kind, packet, cgt_for(kind, packet))
                head = prompt.prefix("play") + arms.LABEL
                whole = tokenizer.encode(prompt.play_prompt(kind, packet), add_special_tokens=False)
                self.assertEqual(whole, tokenizer.encode(head, add_special_tokens=False) + [base + i for i in ids], kind)
                self.assertFalse(any(t >= base for t in tokenizer.encode(head, add_special_tokens=False)))
                question = "\nQuestion: Which pieces attack the white pawn on e4? (pins ignored)"
                qa = tokenizer.encode(prompt.qa_prompt(kind, packet, question[len("\nQuestion: "):]), add_special_tokens=False)
                self.assertEqual(qa, whole + tokenizer.encode(question, add_special_tokens=False))
                self.assertEqual(tokenizer.decode(whole[len(tokenizer.encode(head, add_special_tokens=False)):]),
                                 arms.ids_to_text(ids))

    def test_tiny_tokenizer(self):
        self.check(host_tokenizer())

    @unittest.skipUnless(REAL_TOKENIZER.is_dir(), "SmolVLM snapshot not on disk")
    def test_smolvlm_tokenizer_lands_at_49280(self):
        from transformers import AutoTokenizer

        tokenizer = AutoTokenizer.from_pretrained(REAL_TOKENIZER, local_files_only=True)
        self.assertEqual(len(tokenizer), 49280)
        self.check(tokenizer)

    @unittest.skipUnless(REAL_TOKENIZER.is_dir(), "SmolVLM snapshot not on disk")
    def test_trainer_and_server_tokenise_an_arm_prompt_identically(self):
        """The BLUT trainer's `encode` and the Predictor's chat-template path give one set of prompt ids, and the
        ctok ids in them are the id-level encoding. Uses the cookbook's data module when it is on disk."""
        from model.train import preflight, ctok
        from transformers import AutoProcessor

        backend = preflight.locate_backend_python()
        if backend is None:
            self.skipTest("cookbook trainer python not on disk")
        data, _ = preflight.load_backend(backend)
        processor = AutoProcessor.from_pretrained(REAL_TOKENIZER, local_files_only=True)
        base = ctok.extend_processor(processor)
        from model.train import run

        for kind in arms.KINDS:
            rows = []
            for r in fixture_rows()[:3]:
                rendered = prompt.qa_prompt(kind, r["packet"], "Whose turn is it?")
                rows.append({"packet": r["packet"], "game_id": r["game_id"], "prompt_kind": kind,
                             "qa": {"question": "Whose turn is it?", "answer": "White to move."}})
                rows[-1]["prompt"], rows[-1]["completion"] = rendered, prompt.format_play_completion("White to move.", "e2e4")
            examples = [data.encode(r["prompt"], r["completion"], processor) for r in rows]
            run.check_ctok_tokens(rows, "play", examples, processor, base)
            tampered = copy.deepcopy(examples)
            first = next(i for i, t in enumerate(tampered[0][0]) if t >= base)
            tampered[0][0][first] = base + (tampered[0][0][first] - base + 1) % vocab.SIZE
            with self.assertRaises(run.Refused):
                run.check_ctok_tokens(rows, "play", tampered, processor, base)


if __name__ == "__main__":
    unittest.main()
