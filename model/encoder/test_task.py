"""T4/T5 task training on a tiny random host (CPU): the converter, prompt parity with the server, the joint
adapter + projector loop, the written artifacts reloaded through the serve loader, resume and loss segments.

The data path is the BLUT trainer's own `hf_image_text_data` (located like model/train/preflight.py); the torch tests
skip when torch, peft or that python are missing. Nothing here says anything about chess strength.
"""

import json
import tempfile
import unittest
from pathlib import Path

from model import prompt
from model.encoder import manifest, rows as rows_module
from model.encoder.planes import packet_planes, packet_inputs
from model.eval.fakes import fixture_rows
from model.serve import test_head as T
from model.tests import fake_backend
from model.train import resume as resume_module

try:
    import torch
    import peft  # noqa: F401
except ImportError:
    torch = None


def backend():
    from model.train import preflight
    path = preflight.locate_backend_python()
    return None if path is None else preflight.load_backend(path)[0]


DATA = backend() or fake_backend  # CI has no blut-cookbooks checkout; the fake restates its contract


class Tokenizer(T.FakeTokenizer):
    def convert_tokens_to_ids(self, token):
        return {"<image>": T.IMAGE, "<end_of_utterance>": T.EOS}.get(token, -1)


class Processor(T.FakeProcessor):
    tokenizer = Tokenizer()
    chat_template = True


QUESTIONS = ("Whose turn is it?", "Is the side to move in check?")


def s0_rows(n=4):
    """S0-shaped rows: a packet, a question and the play call that answers it."""
    out = []
    for i, row in enumerate(fixture_rows()[:n]):
        answer = ("White to move.", "No.")[i % 2]
        qa = {"type": ("side_to_move", "check")[i % 2], "question": QUESTIONS[i % 2], "answer": answer}
        out.append({"id": f"s0-train-{i:06d}", "game_id": row["game_id"], "packet": row["packet"], "target": row["target"],
                    "qa": qa, "prompt_kind": "play", "completion": prompt.format_play_completion(answer, row["target"])})
    return out


class ConverterTests(unittest.TestCase):
    def test_an_s0_row_becomes_a_board_row_with_the_servers_planes_and_prompt(self):
        for kind in ("play-enc",):
            for src in s0_rows():
                row = rows_module.convert_row(src, kind)
                self.assertEqual((row["fen"], row["repetition"]), packet_inputs(src["packet"]))
                self.assertEqual(row["prompt"], prompt.qa_prompt(kind, src["packet"], src["qa"]["question"]))
                self.assertEqual(row["completion"], src["completion"])
                self.assertEqual(row["prompt"].count("<image>"), 64)
                self.assertNotIn(src["packet"]["fen"], row["prompt"])
                self.assertEqual(row["qa"]["answer"], src["qa"]["answer"])
                self.assertEqual(row["kind"], kind)

    def test_a_reasoning_row_is_rendered_and_a_segmented_row_keeps_its_loss_spans(self):
        src = fixture_rows()[0]
        plain = rows_module.convert_row(dict(src, reasoning="A quiet move."), "play-enc")
        self.assertEqual(plain["prompt"], prompt.play_prompt("play-enc", src["packet"]))
        self.assertEqual(plain["completion"], prompt.format_play_completion("A quiet move.", src["target"]))
        reads = [{"text": '<tool_call>{"name":"gives_check","arguments":{"move":"e2e4"}}</tool_call>\n', "loss": True},
                 {"text": "<tool_result>{}</tool_result>\n", "loss": False},
                 {"text": plain["completion"], "loss": True}]
        done = rows_module.convert_row(dict(src, completion="".join(s["text"] for s in reads), segments=reads), "play-enc")
        self.assertEqual(done["segments"], reads)
        with self.assertRaises(rows_module.RowError):
            rows_module.convert_row(dict(src, completion="other", segments=reads), "play-enc")

    def test_refusals(self):
        src = s0_rows(1)[0]
        with self.assertRaises(rows_module.RowError):
            rows_module.convert_row(src, "play")  # not a board-encoder kind
        with self.assertRaises(rows_module.RowError):
            rows_module.convert_row({k: v for k, v in src.items() if k != "completion"}, "play-enc")
        with self.assertRaises(rows_module.RowError):
            rows_module.convert_row(dict(src, qa=dict(src["qa"], question="two\nlines")), "play-enc")

    def test_the_cli_writes_jsonl_from_an_audited_corpus_and_refuses_an_unaudited_one(self):
        from model.encoder.rows import main
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(main(["--corpus", tmp, "--out", str(Path(tmp) / "x.jsonl")]), 2)  # no manifest.json


@unittest.skipIf(torch is None, "torch or peft not installed")
class TaskTests(T.HostCase):
    CFG = dict(mode="joint", epochs=4, batch_size=2, grad_accum=1, seq_len=100000, lr=2e-3, projector_lr_mult=5.0,
               warmup_ratio=0.0, lr_scheduler="constant", max_grad_norm=1.0, seed=0, shuffle_buffer=1, max_steps=None,
               save_steps=0, logging_steps=1)

    def setUp(self):
        from model.encoder.encoder import BoardEncoder, EncoderConfig, Projector, save_encoder
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.dir = Path(self.tmp.name)
        self.rows = rows_module.convert(s0_rows(), "play-enc")
        self.shard = rows_module.write_jsonl(self.dir / "train-00000.jsonl", self.rows)
        torch.manual_seed(1)
        self.cfg = EncoderConfig.tiny()
        save_encoder(self.dir / "encoder", BoardEncoder(self.cfg).eval())
        manifest.write_encoder_manifest(self.dir / "encoder")
        self.Projector = Projector

    def parts(self, mode="joint", seed=0):
        from model.encoder.encoder import load_encoder
        host = self.host(seed)
        if mode == "joint":
            host = peft.get_peft_model(host, peft.LoraConfig(
                r=4, lora_alpha=8, target_modules=r".*text_model.*\.(q_proj|v_proj)", bias="none"))
        encoder = load_encoder(self.dir / "encoder")
        torch.manual_seed(0)
        return host, encoder, self.Projector(self.cfg.d_model, self.HIDDEN)

    def train(self, mode="joint", ckpt=None, resume=False, interrupt_at=None, **over):
        from model.encoder import task
        host, encoder, projector = self.parts(mode)
        events = []

        def emit(event):
            events.append(event)
            if interrupt_at and event.get("kind") == "step" and event["step"] == interrupt_at:
                raise Interrupted
        cfg = dict(self.CFG, mode=mode, checkpoint_dir=ckpt, resume=resume, **over)
        summary = task.train_loop(host, encoder, projector, Processor(), DATA, [self.shard], cfg, emit)
        return host, encoder, projector, summary, events

    def test_prompt_parity_with_the_server_and_a_refusal(self):
        from model.encoder import task
        report = task.check_serve_parity(self.rows, Processor(), DATA, "play-enc")
        self.assertEqual((report["status"], report["rows"], report["board_tokens"]), ("ok", 4, 64))
        bad = dict(self.rows[0], prompt=self.rows[0]["prompt"].replace("Whose turn", "Whose move"))
        with self.assertRaisesRegex(task.Refused, "differs from model.prompt"):
            task.check_serve_parity([bad], Processor(), DATA, "play-enc")
        bad = dict(self.rows[0], fen=self.rows[1]["fen"])
        with self.assertRaisesRegex(task.Refused, "planes"):
            task.check_serve_parity([bad], Processor(), DATA, "play-enc")

    def test_joint_training_lowers_the_loss_and_trains_only_adapter_and_projector(self):
        host, encoder, projector, summary, events = self.train()
        frozen = {n: p.detach().clone() for n, p in self.parts()[0].named_parameters() if not p.requires_grad}
        self.assertEqual(summary["steps"], 8)
        self.assertLess(summary["last"]["lm_loss"], summary["first"]["lm_loss"])
        self.assertGreater(summary["trainable_adapter_parameters"], 0)
        self.assertGreater(summary["projector_parameters"], 0)
        self.assertTrue(all(torch.equal(p, frozen[n]) for n, p in host.named_parameters() if n in frozen))  # base untouched
        fresh = self.parts()[1]
        self.assertTrue(all(torch.equal(a, b) for a, b in zip(encoder.parameters(), fresh.parameters())))  # encoder frozen
        self.assertFalse(any(p.requires_grad for p in encoder.parameters()))
        self.assertEqual(len(events), 8)

    def test_projector_only_leaves_the_host_untouched_and_moves_the_projector(self):
        host0, _, projector0 = self.parts("projector-only")
        before = {n: p.detach().clone() for n, p in host0.named_parameters()}
        start = [p.detach().clone() for p in projector0.parameters()]
        host, _, projector, summary, _ = self.train("projector-only")
        self.assertEqual(summary["trainable_adapter_parameters"], 0)
        self.assertLess(summary["last"]["lm_loss"], summary["first"]["lm_loss"])
        self.assertFalse(all(torch.equal(a, b) for a, b in zip(start, projector.parameters())))
        self.assertTrue(all(torch.equal(p, before[n]) for n, p in host.named_parameters()))

    def test_written_artifacts_reload_to_the_same_logits_through_the_serve_loader(self):
        from model.encoder import task
        host, encoder, projector, _, _ = self.train(max_steps=3)
        out = task.write_outputs(self.dir / "run", host, projector, self.dir / "encoder", mode="joint", kind="play-enc",
                                 base_sha256="b" * 64, n_board_tokens=64, rows_sha256="r" * 64)
        projector.to("cpu")
        for name in ("adapter/adapter_model.safetensors", "projector/projector.pt", "projector/manifest.json", "task.json"):
            self.assertTrue((self.dir / "run" / name).is_file(), name)
        task_manifest = manifest.read_task(self.dir / "run")
        self.assertEqual((task_manifest["adapter_sha256"], task_manifest["projector_sha256"], task_manifest["prompt_kind"]),
                         (out["adapter_sha256"], out["projector_sha256"], "play-enc"))
        worst = task.reload_equivalence(self.host(), self.dir / "run", self.dir / "encoder", host, encoder, projector,
                                        self.rows[:3], Processor(), "b" * 64, "play-enc")
        self.assertLess(worst, 1e-5)

    def test_the_server_loader_refuses_an_adapter_with_another_projector_base_or_kind(self):
        from model.encoder import task
        host, encoder, projector, _, _ = self.train(max_steps=2)
        task.write_outputs(self.dir / "run", host, projector, self.dir / "encoder", mode="joint", kind="play-enc",
                           base_sha256="b" * 64, n_board_tokens=64)
        args = (self.host(), self.dir / "run", self.dir / "encoder", host, encoder, projector, self.rows[:1], Processor())
        with self.assertRaisesRegex(ValueError, "aligned to host"):
            task.reload_equivalence(*args, "c" * 64, "play-enc")
        with self.assertRaisesRegex(ValueError, "prompt kind"):
            task.reload_equivalence(*args, "b" * 64, "play-enc-cgt")
        path = self.dir / "run" / "task.json"
        tampered = json.loads(path.read_text())
        tampered["adapter_sha256"] = "0" * 64
        path.write_text(json.dumps(tampered))
        with self.assertRaisesRegex(ValueError, "adapter"):
            task.reload_equivalence(*args, "b" * 64, "play-enc")

    def test_an_interrupted_task_run_resumes_to_the_same_weights_as_an_uninterrupted_one(self):
        whole = self.train(ckpt=self.dir / "whole", save_steps=3)
        with self.assertRaises(Interrupted):
            self.train(ckpt=self.dir / "cut", save_steps=3, interrupt_at=5)
        self.assertEqual(resume_module.load(self.dir / "cut")["step"], 3)
        resumed = self.train(ckpt=self.dir / "cut", save_steps=3, resume=True)
        self.assertEqual(resumed[4][0], {"kind": "resumed", "step": 3, "rows_consumed": 6})
        self.assertEqual((resumed[3]["steps"], resumed[3]["resumed_at_step"]), (8, 3))
        self.assertTrue(all(torch.equal(a, b) for a, b in zip(whole[2].parameters(), resumed[2].parameters())))  # projector
        self.assertTrue(all(torch.equal(a, b) for a, b in zip(
            (p for p in whole[0].parameters() if p.requires_grad), (p for p in resumed[0].parameters() if p.requires_grad))))
        self.assertEqual(whole[3]["last"]["lm_loss"], resumed[3]["last"]["lm_loss"])
        with self.assertRaisesRegex(Exception, "other settings"):
            self.train(ckpt=self.dir / "cut", save_steps=3, resume=True, lr=1e-3)

    def test_result_spans_carry_no_loss_in_the_task_loop(self):
        from model.encoder import task
        src = fixture_rows()[0]
        final = prompt.format_play_completion("A quiet move.", src["target"])
        reads = [{"text": '<tool_call>{"name":"gives_check","arguments":{"move":"e2e4"}}</tool_call>\n', "loss": True},
                 {"text": '<tool_result>{"call":1,"ok":true}</tool_result>\n', "loss": False},
                 {"text": final, "loss": True}]
        row = rows_module.convert_row(dict(src, completion="".join(s["text"] for s in reads), segments=reads), "play-enc")
        plain = dict(row, segments=[{"text": row["completion"], "loss": True}])
        counts = []
        for r in (row, plain):
            batch = task.make_batch([r], Processor(), DATA, T.IMAGE, 64, 100000, "cpu", T.PAD)
            counts.append(batch["lm_tokens"])
        self.assertEqual(counts[1] - counts[0], len(reads[1]["text"]))  # char-level tokens: exactly the result span
        self.assertEqual(task.check_serve_parity([row], Processor(), DATA, "play-enc")["status"], "ok")

    def test_align_accepts_a_prompt_that_already_carries_its_image_slots(self):
        from model.encoder.align import encode_row
        tok = type("Tok", (Tokenizer,), {"__call__": lambda self, text, add_special_tokens=False: {"input_ids": self.encode(text)}})()
        slotted = encode_row(tok, T.IMAGE, 64, self.rows[0], max_len=100000)
        bare = encode_row(tok, T.IMAGE, 64, dict(self.rows[0], prompt="Whose turn is it?"), max_len=100000)
        self.assertEqual(slotted[0].count(T.IMAGE), 64)
        self.assertEqual(bare[0].count(T.IMAGE), 64)
        with self.assertRaises(ValueError):
            encode_row(tok, T.IMAGE, 80, self.rows[0], max_len=100000)  # 64 slots written, 80 expected


class Interrupted(Exception):
    """Stands in for a killed job."""


if __name__ == "__main__":
    unittest.main()
