"""CPU tests of the ctok1 embedding-extension path on a tiny random-weight Idefics3 (no GPU, no training).

What is pinned: the new rows are the only embedding parameters that train; their delta is all the adapter
stores for them; a saved adapter reloads on a freshly loaded extended base with identical logits; the
trainable count against the full-copy `modules_to_save` route. Needs torch, transformers and peft
(model/train/requirements.txt); skipped without them.
"""

import tempfile
import unittest
from pathlib import Path

try:
    import peft
    import torch
    from model.tokenizer.test_hf import host_tokenizer, tiny_model, sample_ids
except ImportError:  # pragma: no cover
    peft = None

from model.tokenizer import vocab

REGEX = r"model\.text_model\.layers\.\d+\.self_attn\.[qv]_proj"


@unittest.skipIf(peft is None, "torch/transformers/peft not installed")
class Extension(unittest.TestCase):
    def setUp(self):
        from model.train import ctok

        self.ctok = ctok
        self.tok = host_tokenizer()
        self.host_n = len(self.tok)
        self.model = tiny_model(self.host_n)
        self.host_embed = self.model.get_input_embeddings().weight.detach().clone()
        self.host_head = self.model.get_output_embeddings().weight.detach().clone()
        self.base = ctok.extend_processor(type("P", (), {"tokenizer": self.tok})())
        self.untied = ctok.extend_model(self.model, self.tok, self.base)
        self.extended = tiny_model(self.host_n)  # same seed: identical host weights
        ctok.extend_model(self.extended, self.tok, self.base)

    def wrap(self, model, mode="embed+head"):
        return peft.get_peft_model(model, self.ctok.lora_config(peft, model, REGEX, 4, 8, self.base, mode))

    def test_extension_keeps_host_rows_and_sizes_the_matrices(self):
        self.assertTrue(self.untied)
        embed = self.model.get_input_embeddings().weight
        self.assertEqual(embed.shape[0], self.host_n + vocab.SIZE)
        self.assertTrue(torch.equal(embed[:self.host_n], self.host_embed))
        self.assertTrue(torch.equal(self.model.get_output_embeddings().weight[:self.host_n], self.host_head))

    def test_only_new_rows_and_lora_train_and_counts_beat_modules_to_save(self):
        hidden = self.host_embed.shape[1]
        both = self.ctok.parameter_report(self.wrap(self.model))
        self.assertEqual(both["new_token_rows"], 2 * vocab.SIZE * hidden)
        self.assertEqual(both["other"], 0)
        only = self.ctok.parameter_report(self.wrap(self._fresh(), "embed"))
        self.assertEqual(only["new_token_rows"], vocab.SIZE * hidden)
        full = self._fresh()
        full = peft.get_peft_model(full, peft.LoraConfig(r=4, lora_alpha=8, target_modules=REGEX,
                                                         modules_to_save=["embed_tokens", "lm_head"]))
        copies = sum(p.numel() for p in full.parameters() if p.requires_grad)
        self.assertLess(both["trainable"], copies)
        self.assertEqual(both["modules_to_save_alternative"], 2 * (self.host_n + vocab.SIZE) * hidden)
        self.assertEqual(copies - both["lora"], both["modules_to_save_alternative"])

    def _fresh(self):
        model = tiny_model(self.host_n)
        self.ctok.extend_model(model, self.tok, self.base)
        return model

    def test_gradient_reaches_new_rows_only(self):
        model = self.wrap(self.model)
        ids = sample_ids(self.base)
        loss = model(input_ids=ids, labels=ids).loss
        loss.backward()
        deltas = {n: p for n, p in model.named_parameters() if "trainable_tokens_delta" in n}
        self.assertEqual(len(deltas), 2)
        for name, param in deltas.items():
            self.assertEqual(param.shape[0], vocab.SIZE, name)
            self.assertGreater(float(param.grad.abs().sum()), 0.0, name)
        for name, param in model.named_parameters():
            if "embed_tokens" in name and "trainable_tokens_delta" not in name:
                self.assertFalse(param.requires_grad, name)

    def test_adapter_reload_on_a_fresh_extended_base_gives_identical_logits(self):
        model = self.wrap(self.model)
        gen = torch.Generator().manual_seed(1)
        with torch.no_grad():  # stand in for training: move every trainable parameter
            for p in model.parameters():
                if p.requires_grad:
                    p.add_(0.05 * torch.randn(p.shape, generator=gen))
        ids = sample_ids(self.base)
        model.eval()
        with torch.no_grad():
            want = model(input_ids=ids).logits
        with tempfile.TemporaryDirectory() as tmp:
            base_dir, adapter = Path(tmp) / "base", Path(tmp) / "adapter"
            self.extended.save_pretrained(base_dir)  # the extended base snapshot
            model.save_pretrained(adapter)
            size = sum(f.stat().st_size for f in adapter.iterdir())
            from transformers import AutoModelForImageTextToText

            fresh = AutoModelForImageTextToText.from_pretrained(base_dir, dtype=torch.float32)
            reloaded = peft.PeftModel.from_pretrained(fresh, adapter).eval()
            with torch.no_grad():
                got = reloaded(input_ids=ids).logits
            self.assertTrue(torch.allclose(got, want, atol=1e-5, rtol=0))
            stored = sum(p.numel() for p in model.parameters() if p.requires_grad) * 4
            self.assertLess(size, 1.05 * stored + 65536)  # the adapter holds the LoRA weights and new-row deltas only
            merged = reloaded.merge_and_unload().eval()
            with torch.no_grad():
                self.assertTrue(torch.allclose(merged(input_ids=ids).logits, want, atol=1e-4, rtol=0))
            embed = merged.get_input_embeddings().weight
            self.assertTrue(torch.equal(embed[:self.host_n], self.host_embed))  # host rows never moved
            self.assertFalse(torch.equal(embed[self.base:], self.extended.get_input_embeddings().weight[self.base:]))
            self.assertTrue(torch.equal(merged.get_output_embeddings().weight[:self.host_n], self.host_head))


if __name__ == "__main__":
    unittest.main()
