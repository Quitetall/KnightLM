"""Joint head/adapter training on a tiny random host (CPU): parity with the serve contract, legality, outputs, reload.

The data path is the BLUT trainer's own `hf_image_text_data` (located like model/train/preflight.py); tests that
need it skip when the crate's python is not on this machine, torch tests skip without torch/peft/transformers.
Nothing here says anything about chess strength.
"""

import json
import tempfile
import unittest
from pathlib import Path

from model import prompt
from model.eval.fakes import fixture_rows
from model.policy_head import joint, manifest, space
from model.serve import test_head as T
from model.tests import fake_backend
from model.train import resume as resume_module, segments as segments_module

try:
    import torch
    import peft  # noqa: F401
except ImportError:
    torch = None


def real_backend():
    from model.train import preflight
    path = preflight.locate_backend_python()
    return None if path is None else preflight.load_backend(path)[0]


def backend():
    return real_backend()


DATA = backend() or fake_backend  # CI has no blut-cookbooks checkout; the fake restates its contract


class Tokenizer(T.FakeTokenizer):
    def convert_tokens_to_ids(self, token):
        return T.EOS if token == "<end_of_utterance>" else -1


class Processor(T.FakeProcessor):
    tokenizer = Tokenizer()
    chat_template = True


def rows(n=6):
    out = []
    for row in fixture_rows()[:n]:
        move = row["target"]
        out.append(dict(row, reasoning=f"A quiet developing move. I choose {move}."))
    return out


@unittest.skipIf(real_backend() is None, "the BLUT trainer's python is not on this machine (the fake is used)")
class FakeBackendParity(unittest.TestCase):
    """The CI stand-in (model/tests/fake_backend.py) is the real data module on what the joint trainers call."""

    def test_encode_and_the_row_stream_equal_the_real_modules(self):
        real = real_backend()
        proc = Processor()
        for row in rows():
            v = joint.views(row)
            self.assertEqual(fake_backend.encode(v["cond_prompt"], v["completion"], proc),
                             real.encode(v["cond_prompt"], v["completion"], proc))
        with tempfile.TemporaryDirectory() as tmp:
            shard = Path(tmp) / "s.jsonl"
            shard.write_text("".join(f"{i}\n" for i in range(40)))
            for buffer in (1, 7):
                a, b = fake_backend.RowStream([shard], 3, buffer, 2), real.RowStream([shard], 3, buffer, 2)
                self.assertEqual([a.next_line() for _ in range(81)], [b.next_line() for _ in range(81)])
            self.assertEqual(fake_backend.count_rows([shard]), real.count_rows([shard]))


class ViewTests(unittest.TestCase):
    def test_the_two_views_differ_only_by_the_move(self):
        v = joint.views(rows(1)[0])
        self.assertTrue(v["read_prompt"].endswith(prompt.HEAD_LABEL))
        self.assertNotIn(prompt.HEAD_LABEL + " ", v["read_prompt"])
        self.assertEqual(v["cond_prompt"], v["read_prompt"] + " " + v["target"])
        self.assertEqual(prompt.parse_play_completion(v["completion"])["move"], v["target"])

    def test_refusals(self):
        row = rows(1)[0]
        with self.assertRaises(space.MoveSpaceError):
            joint.views(dict(row, target="a1a1"))
        with self.assertRaises(joint.Refused):
            joint.views({k: v for k, v in row.items() if k != "reasoning"})
        joint.views({k: v for k, v in row.items() if k != "reasoning"}, need_completion=False)  # head-only is fine


class ParityTests(unittest.TestCase):
    def test_trainer_ids_equal_serve_ids(self):
        report = joint.check_serve_parity(rows(), Processor(), DATA)
        self.assertEqual((report["status"], report["rows"]), ("ok", 6))

    def test_a_completion_that_disagrees_with_its_target_is_refused(self):
        bad = rows(1)[0]
        other = next(m for m in bad["packet"]["legal_moves"] if m != bad["target"])
        bad["reasoning"] = f"I choose {other}."
        with self.assertRaises(joint.Refused):
            joint.check_serve_parity([bad], Processor(), DATA)

    def test_head_only_rows_need_no_reasoning(self):
        plain = [{k: v for k, v in r.items() if k != "reasoning"} for r in rows(2)]
        self.assertEqual(joint.check_serve_parity(plain, Processor(), DATA, need_completion=False)["status"], "ok")


@unittest.skipIf(torch is None, "torch not installed")
class MaskTests(unittest.TestCase):
    def test_masks_match_the_packets_and_the_masked_argmax_is_legal(self):
        report = joint.check_mask_legality(rows())
        self.assertEqual((report["masked_argmax_legal"], report["illegal_mass_after_mask"]), (1.0, 0.0))

    def test_a_target_outside_the_move_space_is_refused(self):
        bad = dict(rows(1)[0], target="zzzz")
        with self.assertRaises(space.MoveSpaceError):
            joint.check_mask_legality([bad])


CFG = dict(mode="joint", epochs=4, batch_size=2, grad_accum=1, seq_len=100000, lr=2e-3, head_lr_mult=10.0,
           head_weight=1.0, warmup_ratio=0.0, lr_scheduler="constant", max_grad_norm=1.0, seed=0,
           shuffle_buffer=1, max_steps=None, save_steps=0, logging_steps=1)


@unittest.skipIf(torch is None, "torch or peft not installed")
class JointTests(T.HostCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.dir = Path(self.tmp.name)
        self.shard = self.dir / "train-00000.jsonl"
        self.shard.write_text("".join(json.dumps(r) + "\n" for r in rows()))

    def peft_host(self, seed=0):
        host = self.host(seed)
        cfg = peft.LoraConfig(r=4, lora_alpha=8, target_modules=r".*text_model.*\.(q_proj|v_proj)", bias="none")
        return peft.get_peft_model(host, cfg)

    def train(self, model, mode="joint", **over):
        from model.policy_head.head import MoveHead
        torch.manual_seed(0)
        head = MoveHead(self.HIDDEN)
        logged = []
        summary = joint.train_loop(model, head, Processor(), DATA, [self.shard], dict(CFG, mode=mode, **over),
                                   logged.append)
        return head, summary, logged

    def test_hidden_state_contract_batched_equals_the_serve_read(self):
        from model.policy_head.head import MoveHead
        model, head = self.peft_host().eval(), MoveHead(self.HIDDEN).eval()
        batch = rows(3)
        stub = joint.serve_stub(Processor())
        seqs = [joint.read_ids(stub, r["packet"]) for r in batch]
        self.assertGreater(len({len(s) for s in seqs}), 1)  # unequal lengths: padding is exercised
        ids, attn = joint.pad(seqs, T.PAD)
        with torch.no_grad():
            got = joint.head_forward(model, head, ids, attn, train_host=False)
        from model.serve.head import HeadPredictor
        predictor = self.predictor(HeadPredictor,
                                   "play-head", model=model, head=head)
        for i, r in enumerate(batch):
            torch.testing.assert_close(got[i], torch.tensor(predictor.logits(r["packet"])), rtol=0, atol=1e-5)

    def test_joint_training_lowers_both_losses_and_trains_only_adapter_and_head(self):
        model = self.peft_host()
        base_before = {n: p.detach().clone() for n, p in model.named_parameters() if not p.requires_grad}
        head, summary, logged = self.train(model)
        self.assertEqual(summary["steps"], 12)
        self.assertLess(summary["last"]["head_loss"], summary["first"]["head_loss"])
        self.assertLess(summary["last"]["lm_loss"], summary["first"]["lm_loss"])
        self.assertGreater(summary["trainable_adapter_parameters"], 0)
        self.assertTrue(all(torch.equal(p, base_before[n]) for n, p in model.named_parameters() if n in base_before))
        self.assertEqual(len(logged), 12)

    def test_head_only_probe_leaves_the_host_untouched(self):
        host = self.host()
        before = {n: p.detach().clone() for n, p in host.named_parameters()}
        head, summary, _ = self.train(host, mode="head-only")
        self.assertIsNone(summary["last"]["lm_loss"])
        self.assertLess(summary["last"]["head_loss"], summary["first"]["head_loss"])
        self.assertTrue(all(torch.equal(p, before[n]) for n, p in host.named_parameters()))

    def test_written_outputs_reload_to_the_same_logits_through_the_serve_loader(self):
        model = self.peft_host()
        head, _, _ = self.train(model)
        out = joint.write_outputs(self.dir / "run", model, head, mode="joint", base_sha256="b" * 64,
                                  corpus_manifest_sha256="c" * 64)
        self.assertTrue((self.dir / "run/adapter/adapter_model.safetensors").is_file())
        self.assertTrue((self.dir / "run/head/head.pt").is_file() and (self.dir / "run/head/head.json").is_file())
        bound = manifest.read(self.dir / "run/head")
        self.assertEqual((bound["base_sha256"], bound["adapter_sha256"], bound["head_sha256"]),
                         ("b" * 64, out["adapter_sha256"], out["head_sha256"]))
        self.assertEqual(bound["ctok"]["spec_version"], "ctok/1")
        self.assertEqual(bound["rules"]["prompt_kind"], "play-head")
        worst = joint.reload_equivalence(self.host(), out["adapter_dir"], out["head_dir"], model, head,
                                         [r["packet"] for r in rows(3)], Processor(), base_sha256="b" * 64)
        self.assertLess(worst, 1e-6)

    def test_the_manifest_refuses_a_head_served_on_another_adapter_or_base(self):
        model = self.peft_host()
        head, _, _ = self.train(model, max_steps=2)
        out = joint.write_outputs(self.dir / "run", model, head, mode="joint", base_sha256="b" * 64)
        from model.serve.head import HeadPredictor
        predictor = self.predictor(HeadPredictor, "play-head", model=self.host(), base_sha256="b" * 64,
                                   adapter_sha256=out["adapter_sha256"])
        predictor.attach_head(out["head_dir"])  # matches
        for attrs in ({"adapter_sha256": "0" * 64}, {"adapter_sha256": None}, {"base_sha256": "f" * 64}):
            other = self.predictor(HeadPredictor, "play-head", model=self.host(),
                                   **{"base_sha256": "b" * 64, "adapter_sha256": out["adapter_sha256"], **attrs})
            with self.assertRaises(ValueError):
                other.attach_head(out["head_dir"])
        plain = self.dir / "plain"  # a head saved without a manifest still loads
        from model.policy_head.head import save_head
        save_head(plain, head)
        self.predictor(HeadPredictor, "play-head", model=self.host(), base_sha256="b" * 64,
                       adapter_sha256="1" * 64).attach_head(plain)

    def test_head_only_outputs_have_no_adapter(self):
        host = self.host()
        head, _, _ = self.train(host, mode="head-only", max_steps=2)
        out = joint.write_outputs(self.dir / "probe", host, head, mode="head-only", base_sha256="b" * 64)
        self.assertIsNone(out["adapter_dir"])
        self.assertEqual(manifest.read(self.dir / "probe/head")["adapter_sha256"], "none")
        worst = joint.reload_equivalence(self.host(), None, out["head_dir"], host, head,
                                         [r["packet"] for r in rows(2)], Processor(), base_sha256="b" * 64)
        self.assertLess(worst, 1e-6)

    def test_too_long_rows_are_dropped_and_counted_never_truncated(self):
        model = self.peft_host()
        from model.policy_head.head import MoveHead
        with self.assertRaisesRegex(RuntimeError, "no optimizer step"):  # nothing fits in 10 tokens
            joint.train_loop(model, MoveHead(self.HIDDEN), Processor(), DATA, [self.shard],
                             dict(CFG, seq_len=10, epochs=1), lambda _e: None)

    # --- resume (model/train/resume.py) ---------------------------------------------------------

    def go(self, ckpt, mode="joint", interrupt_at=None, resume=False, **over):
        """A full run with checkpoints every 4 steps; `interrupt_at` kills it from the step log, like a crashed job."""
        from model.policy_head.head import MoveHead
        model = self.peft_host() if mode == "joint" else self.host()
        torch.manual_seed(0)
        head = MoveHead(self.HIDDEN)
        events = []

        def emit(event):
            events.append(event)
            if interrupt_at and event.get("kind") == "step" and event["step"] == interrupt_at:
                raise Interrupted

        cfg = dict(CFG, mode=mode, save_steps=4, checkpoint_dir=ckpt, resume=resume, **over)
        summary = joint.train_loop(model, head, Processor(), DATA, [self.shard], cfg, emit)
        weights = resume_module.trainable_state(model, head=head)
        return weights, summary, events

    def assert_same(self, a, b, atol=0.0):
        self.assertEqual(a.keys(), b.keys())
        for group in a:
            self.assertEqual(a[group].keys(), b[group].keys())
            for name in a[group]:
                torch.testing.assert_close(a[group][name], b[group][name], rtol=0, atol=atol, msg=f"{group}.{name}")

    def test_an_interrupted_joint_run_resumes_to_the_same_weights_as_an_uninterrupted_one(self):
        whole, whole_summary, _ = self.go(self.dir / "whole")
        with self.assertRaises(Interrupted):
            self.go(self.dir / "cut", interrupt_at=6)  # the checkpoint of step 4 survives; steps 5 and 6 are lost
        self.assertEqual(resume_module.load(self.dir / "cut")["step"], 4)
        resumed, summary, events = self.go(self.dir / "cut", resume=True)
        self.assertEqual(events[0], {"kind": "resumed", "step": 4, "rows_consumed": 8})
        self.assertEqual((summary["steps"], summary["resumed_at_step"], summary["rows_consumed"]),
                         (12, 4, whole_summary["rows_consumed"]))
        self.assertTrue(any(weights.numel() for weights in whole["host"].values()))  # the adapter is in the comparison
        self.assert_same(whole, resumed)  # bit-identical on CPU: atol 0
        self.assertEqual(summary["last"]["head_loss"], whole_summary["last"]["head_loss"])
        drop = lambda record: {k: v for k, v in record.items() if k != "seconds"}  # noqa: E731
        self.assertEqual(drop(summary["first"]), drop(whole_summary["first"]))

    def test_a_head_only_run_resumes_the_same_way(self):
        whole, _, _ = self.go(self.dir / "whole", mode="head-only")
        with self.assertRaises(Interrupted):
            self.go(self.dir / "cut", mode="head-only", interrupt_at=9)
        resumed, summary, _ = self.go(self.dir / "cut", mode="head-only", resume=True)
        self.assertEqual(summary["resumed_at_step"], 8)
        self.assert_same(whole, resumed)

    def test_resume_refuses_nothing_to_resume_and_other_settings(self):
        with self.assertRaisesRegex(joint.Refused, "no checkpoint"):
            self.go(self.dir / "none", resume=True)
        with self.assertRaises(Interrupted):
            self.go(self.dir / "cut", interrupt_at=5)
        with self.assertRaisesRegex(joint.Refused, "other settings.*lr"):
            self.go(self.dir / "cut", resume=True, lr=1e-3)

    def test_a_checkpoint_is_replaced_atomically_and_only_the_latest_is_kept(self):
        with self.assertRaises(Interrupted):
            self.go(self.dir / "cut", interrupt_at=10)
        self.assertEqual(sorted(p.name for p in (self.dir / "cut").iterdir()), ["state.pt"])
        self.assertEqual(resume_module.load(self.dir / "cut")["step"], 8)

    # --- loss segments (model/train/segments.py) --------------------------------------------------

    def segment_rows(self):
        out = []
        for row in rows(4):
            move = row["target"]
            reads = ('<tool_call>{"name":"gives_check","arguments":{"move":"' + move + '"}}</tool_call>\n',
                     '<tool_result>{"call":1,"ok":true,"result":{"gives_check":false}}</tool_result>\n')
            final = prompt.format_play_completion(row["reasoning"], move)
            segments = [{"text": reads[0], "loss": True}, {"text": reads[1], "loss": False},
                        {"text": final, "loss": True}]
            out.append(dict(row, segments=segments, completion="".join(s["text"] for s in segments)))
        return out

    def test_result_spans_carry_no_loss_and_everything_else_does(self):
        row = self.segment_rows()[0]
        v = joint.views(row)
        prompt_ids, ids, labels = segments_module.encode(v["cond_prompt"], row["segments"], Processor(), DATA)
        plain_prompt, plain_ids = DATA.encode(v["cond_prompt"], row["completion"], Processor())
        self.assertEqual((prompt_ids, ids), (plain_prompt, plain_ids))  # the same ids as the trainer's encode
        result = row["segments"][1]["text"]
        want = ids[len(prompt_ids):]
        first = len(row["segments"][0]["text"])
        masked = [i for i, x in enumerate(labels) if x == -100 and i >= len(prompt_ids)]
        self.assertEqual(len(masked), len(result))  # char-level fake tokenizer: one token per character
        self.assertEqual([ids[i] for i in masked], [ord(c) for c in result])
        self.assertEqual(masked[0], len(prompt_ids) + first)
        self.assertTrue(all(labels[i] == ids[i] for i in range(len(prompt_ids), len(ids)) if i not in masked))
        self.assertEqual(labels[-1], ids[-1])  # the end of turn is trained
        self.assertEqual(len(want) - len(result), sum(1 for x in labels if x != -100))

    def test_a_joint_run_on_segment_rows_trains_on_fewer_tokens_than_the_same_rows_whole(self):
        from model.policy_head.head import MoveHead
        cut = self.dir / "seg.jsonl"
        cut.write_text("".join(json.dumps(r) + "\n" for r in self.segment_rows()))
        whole = self.dir / "whole.jsonl"
        whole.write_text("".join(json.dumps(dict(r, segments=[{"text": r["completion"], "loss": True}])) + "\n"
                                 for r in self.segment_rows()))
        counts = []
        for shard in (cut, whole):
            stub = joint.serve_stub(Processor())
            batch = joint.make_batch([json.loads(l) for l in shard.read_text().splitlines()[:2]], Processor(), DATA,
                                     stub, "joint", 100000, "cpu", T.PAD)
            counts.append(batch["lm_tokens"])
        self.assertLess(counts[0], counts[1])
        model = self.peft_host()
        torch.manual_seed(0)
        summary = joint.train_loop(model, MoveHead(self.HIDDEN), Processor(), DATA, [cut], dict(CFG, epochs=3),
                                   lambda _e: None)
        self.assertEqual(summary["steps"], 6)
        self.assertLess(summary["last"]["lm_loss"], summary["first"]["lm_loss"])

    def test_segments_that_do_not_spell_the_completion_are_refused(self):
        row = self.segment_rows()[0]
        bad = dict(row, completion=row["completion"] + "x")
        with self.assertRaises(joint.Refused):
            joint.views(bad)
        with self.assertRaises(segments_module.SegmentError):  # a boundary inside a token cannot carry a mask
            class Merge(Tokenizer):
                def encode(self, text, add_special_tokens=False):
                    ids = super().encode(text)
                    return ids[:-1] + [999] if text.endswith("</tool_call>\n") else ids  # a token that straddles it
            proc = Processor()
            proc.tokenizer = Merge()
            segments_module.encode(joint.views(row)["cond_prompt"], row["segments"], proc, DATA)

    def test_the_recipe_path_flags_rows_with_result_spans(self):
        self.assertEqual(segments_module.check_recipe_rows(self.segment_rows()), ["?"] * 4)
        self.assertEqual(segments_module.check_recipe_rows(rows()), [])


class Interrupted(Exception):
    """Stands in for a killed job."""


if __name__ == "__main__":
    unittest.main()
