"""The masked text-only adapter trainer (reasoning v2 variant b) on a tiny random host (CPU).

Uses the BLUT trainer's data module when it is on this machine and the stand-in of model/tests/fake_backend.py otherwise.
Nothing here says anything about chess strength.
"""

import json
import tempfile
import unittest
from pathlib import Path

from model import prompt
from model.encoder import rows as rows_module, task
from model.eval.fakes import fixture_rows
from model.policy_head.test_joint import Interrupted, Processor, real_backend
from model.serve import test_head as T
from model.tests import fake_backend
from model.train import masked, resume as resume_module, segments as segments_module

try:
    import torch
    import peft
except ImportError:
    torch = None

DATA = real_backend() or fake_backend


def b_rows(n=6, kind="play-reads"):
    out = []
    for i, row in enumerate(fixture_rows()[:n]):
        move = row["target"]
        reads = ('<tool_call>{"name":"gives_check","arguments":{"move":"' + move + '"}}</tool_call>\n',
                 '<tool_result>{"call":1,"ok":true,"result":{"gives_check":false}}</tool_result>\n')
        final = prompt.format_play_completion(f"White to move. I choose {move}.", move)
        segments = [{"text": reads[0], "loss": True}, {"text": reads[1], "loss": False}, {"text": final, "loss": True}]
        out.append({"id": f"r2b-train-{i:06d}", "variant": "b", "packet": row["packet"], "prompt_kind": kind,
                    "prompt": prompt.play_prompt(kind, row["packet"]), "segments": segments,
                    "completion": "".join(s["text"] for s in segments)})
    return out


class PromptKindTests(unittest.TestCase):
    def test_play_reads_changes_only_the_instruction_wording(self):
        row = fixture_rows()[0]
        reads, plain = prompt.play_prompt("play-reads", row["packet"]), prompt.play_prompt("play", row["packet"])
        self.assertIn("exactly one tool call", plain)
        self.assertNotIn("exactly one tool call", reads)
        self.assertEqual(reads.replace(prompt.PLAY_READS_INSTRUCTION, prompt.PLAY_INSTRUCTION, 1), plain)
        self.assertEqual(prompt.play_text(row["packet"]), plain)  # the older kinds keep their bytes

    def test_the_python_server_refuses_a_kind_that_needs_read_tools_between_turns(self):
        from model.serve.inference import Predictor
        predictor = object.__new__(Predictor)
        predictor.prompt_kind, predictor._board = "play-reads", None
        with self.assertRaisesRegex(ValueError, "training and scoring kind"):
            predictor._check_play_kind()


class SpacedProcessor(Processor):
    """The SmolVLM template writes `Assistant: <reply>`: one space between the generation prompt and the reply."""

    def apply_chat_template(self, chat, tokenize=False, add_generation_prompt=False):
        return super().apply_chat_template(chat, tokenize, add_generation_prompt).replace("<a>", "<a> ") \
            if not add_generation_prompt else super().apply_chat_template(chat, tokenize, True)


class SegmentTemplateTests(unittest.TestCase):
    def test_a_space_after_the_generation_prompt_belongs_to_the_first_span_and_the_masked_result_is_exact(self):
        row = b_rows(1)[0]
        proc = SpacedProcessor()
        prompt_ids, ids, labels = segments_module.encode(row["prompt"], row["segments"], proc, DATA)
        self.assertEqual(ids[len(prompt_ids)], ord(" "))  # the space is a completion token ...
        self.assertEqual(labels[len(prompt_ids)], ord(" "))  # ... trained, with the first (loss) span
        first, result = (len(row["segments"][0]["text"]), len(row["segments"][1]["text"]))
        masked_at = [i for i, x in enumerate(labels) if x == -100 and i >= len(prompt_ids)]
        self.assertEqual(masked_at, list(range(len(prompt_ids) + 1 + first, len(prompt_ids) + 1 + first + result)))
        self.assertEqual("".join(chr(ids[i]) for i in masked_at), row["segments"][1]["text"])


@unittest.skipIf(torch is None, "torch or peft not installed")
class MaskedTests(T.HostCase):
    CFG = dict(mode="joint", epochs=4, batch_size=2, grad_accum=1, seq_len=100000, lr=2e-3, projector_lr_mult=1.0,
               warmup_ratio=0.0, lr_scheduler="constant", max_grad_norm=1.0, seed=0, shuffle_buffer=1, max_steps=None,
               save_steps=0, logging_steps=1)

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.dir = Path(self.tmp.name)
        self.rows = b_rows()
        self.shard = rows_module.write_jsonl(self.dir / "train-00000.jsonl", self.rows)

    def peft_host(self):
        return peft.get_peft_model(self.host(0), peft.LoraConfig(
            r=4, lora_alpha=8, target_modules=r".*text_model.*\.(q_proj|v_proj)", bias="none"))

    def train(self, ckpt=None, resume=False, interrupt_at=None, **over):
        model = self.peft_host()
        events = []

        def emit(event):
            events.append(event)
            if interrupt_at and event.get("kind") == "step" and event["step"] == interrupt_at:
                raise Interrupted
        cfg = dict(self.CFG, checkpoint_dir=ckpt, resume=resume, **over)
        summary = task.train_loop(model, None, None, Processor(), DATA, [self.shard], cfg, emit)
        return model, summary, events

    def test_parity_reports_what_the_mask_removes_and_refuses_a_foreign_prompt(self):
        report = masked.check_serve_parity(self.rows, Processor(), DATA)
        self.assertEqual((report["status"], report["kinds"]), ("ok", ["play-reads"]))
        self.assertEqual(report["completion_tokens_masked"], sum(len(r["segments"][1]["text"]) for r in self.rows))
        bad = dict(self.rows[0], prompt=prompt.play_prompt("play", self.rows[0]["packet"]))
        with self.assertRaisesRegex(masked.Refused, "differs from model.prompt"):
            masked.check_serve_parity([bad], Processor(), DATA)
        with self.assertRaisesRegex(masked.Refused, "not a text prompt"):
            masked.check_serve_parity([dict(self.rows[0], prompt_kind="play-enc")], Processor(), DATA)

    def test_the_text_only_loop_trains_the_adapter_on_unmasked_tokens_only(self):
        before = {n: p.detach().clone() for n, p in self.peft_host().named_parameters() if not p.requires_grad}
        model, summary, _ = self.train()
        self.assertEqual((summary["steps"], summary["projector_parameters"]), (12, 0))
        self.assertLess(summary["last"]["lm_loss"], summary["first"]["lm_loss"])
        self.assertTrue(all(torch.equal(p, before[n]) for n, p in model.named_parameters() if n in before))
        batch = task.make_batch(self.rows[:2], Processor(), DATA, T.IMAGE, 0, 100000, "cpu", T.PAD)
        total = sum(len(DATA.encode(r["prompt"], r["completion"], Processor())[1])
                    - len(DATA.encode(r["prompt"], r["completion"], Processor())[0]) for r in self.rows[:2])
        self.assertEqual(total - batch["lm_tokens"], sum(len(r["segments"][1]["text"]) for r in self.rows[:2]))

    def test_an_interrupted_run_resumes_to_the_same_adapter(self):
        whole, whole_summary, _ = self.train(ckpt=self.dir / "whole", save_steps=4)
        with self.assertRaises(Interrupted):
            self.train(ckpt=self.dir / "cut", save_steps=4, interrupt_at=6)
        resumed, summary, events = self.train(ckpt=self.dir / "cut", save_steps=4, resume=True)
        self.assertEqual((events[0]["kind"], summary["resumed_at_step"], summary["steps"]), ("resumed", 4, 12))
        a, b = resume_module.trainable_state(whole), resume_module.trainable_state(resumed)
        self.assertTrue(a["host"])
        self.assertTrue(all(torch.equal(a["host"][n], b["host"][n]) for n in a["host"]))

    def test_the_adapter_reloads_to_the_same_logits(self):
        model, _, _ = self.train(max_steps=3)
        model.save_pretrained(self.dir / "adapter")
        worst = masked.reload_equivalence(self.host(0), self.dir / "adapter", model, self.rows[:2], Processor())
        self.assertLess(worst, 1e-5)


if __name__ == "__main__":
    unittest.main()
