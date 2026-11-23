"""SmolVLM (Idefics3) integration on CPU: extend tokenizer and embeddings by the ctok vocabulary,
save, reload, and require identical weights and logits. A tiny random-weight config stands in for
the 500M model (same classes, untied lm_head); no GPU, no training. The LoRA reload test needs peft
(model/train/requirements.txt pins it) and is skipped without it.
"""

import tempfile
import unittest
from pathlib import Path

from model.tokenizer import hf, vocab

try:
    import torch
    from tokenizers import Tokenizer, models, pre_tokenizers, trainers, decoders
    from transformers import Idefics3Config, Idefics3ForConditionalGeneration, PreTrainedTokenizerFast
except ImportError:  # pragma: no cover
    torch = None

try:
    import peft
except ImportError:  # pragma: no cover
    peft = None

CORPUS = ["e4 knight white king queen rook bishop pawn black empty position board moves 0123456789 - unknown",
          "white to move black to move castles kingside queenside en passant check checkmate promotes to"] * 20
# Scratch on the 4 TB drive when it exists (the owner's machine); the system default elsewhere (CI).
SCRATCH = "/mnt/4tb/tmp" if Path("/mnt/4tb/tmp").is_dir() else None
REAL_TOKENIZER = Path("/mnt/4tb/artifacts/KnightLM/expire/models/smolvlm-500m-a7da5b98")


def host_tokenizer():
    tok = Tokenizer(models.BPE(unk_token=None))
    tok.pre_tokenizer = pre_tokenizers.ByteLevel(add_prefix_space=False)
    tok.decoder = decoders.ByteLevel()
    tok.train_from_iterator(CORPUS, trainers.BpeTrainer(vocab_size=300, special_tokens=["<|im_end|>"],
                                                        initial_alphabet=pre_tokenizers.ByteLevel.alphabet()))
    return PreTrainedTokenizerFast(tokenizer_object=tok, eos_token="<|im_end|>", pad_token="<|im_end|>")


def tiny_model(vocab_size, seed=0):
    torch.manual_seed(seed)
    cfg = Idefics3Config(
        vocab_size=vocab_size, tie_word_embeddings=False, scale_factor=1,
        text_config=dict(model_type="llama", vocab_size=vocab_size, hidden_size=32, intermediate_size=64,
                         num_hidden_layers=2, num_attention_heads=4, num_key_value_heads=2,
                         tie_word_embeddings=False, max_position_embeddings=128),
        vision_config=dict(model_type="idefics3_vision", hidden_size=32, intermediate_size=64, num_hidden_layers=1,
                           num_attention_heads=2, image_size=32, patch_size=16))
    return Idefics3ForConditionalGeneration(cfg).eval()


def sample_ids(base):
    ids = [3, 4, 5, base + vocab.ID["<pos>"], base + vocab.ID["<sq:e4>"], base + vocab.ID["<m:e2e4>"],
           base + vocab.ID["<reserved:000>"], base + vocab.SIZE - 1]
    return torch.tensor([ids])


@unittest.skipIf(torch is None, "torch/transformers not installed")
class Extension(unittest.TestCase):
    def setUp(self):
        self.tok = host_tokenizer()
        self.host_n = len(self.tok)
        self.model = tiny_model(self.host_n)
        self.old_embed = self.model.get_input_embeddings().weight.detach().clone()
        self.old_head = self.model.get_output_embeddings().weight.detach().clone()

    def extend(self):
        base = hf.extend_tokenizer(self.tok)
        self.model.resize_token_embeddings(len(self.tok))
        for matrix in (self.model.get_input_embeddings().weight, self.model.get_output_embeddings().weight):
            hf.init_new_rows(matrix.data, self.tok, base)
        return base

    def test_tokenizer_appends_after_the_host_vocabulary(self):
        base = hf.extend_tokenizer(self.tok)
        self.assertEqual(base, self.host_n)
        self.assertEqual(len(self.tok), self.host_n + vocab.SIZE)
        text = "<pos><side:w><sq:e4><m:e2e4><nl>"
        ids = self.tok.encode(text, add_special_tokens=False)
        self.assertEqual(ids, [base + vocab.ID[t] for t in ("<pos>", "<side:w>", "<sq:e4>", "<m:e2e4>", "<nl>")])
        self.assertEqual(self.tok.decode(ids), text)
        self.assertEqual(self.tok.encode("e4", add_special_tokens=False), self.tok.encode("e4", add_special_tokens=False))
        with self.assertRaises(ValueError):
            hf.extend_tokenizer(self.tok)

    def test_rows_are_new_old_rows_untouched_and_init_is_the_stated_mean(self):
        base = self.extend()
        embed = self.model.get_input_embeddings().weight
        head = self.model.get_output_embeddings().weight
        self.assertEqual(tuple(embed.shape), (base + vocab.SIZE, 32))
        self.assertEqual(tuple(head.shape), (base + vocab.SIZE, 32))
        self.assertTrue(torch.equal(embed[:base], self.old_embed))
        self.assertTrue(torch.equal(head[:base], self.old_head))
        sq = base + vocab.ID["<sq:e4>"]
        ids = self.tok.encode("e4", add_special_tokens=False)
        self.assertTrue(torch.allclose(embed[sq], self.old_embed[ids].mean(0), atol=1e-6))
        self.assertTrue(torch.allclose(head[sq], self.old_head[ids].mean(0), atol=1e-6))
        move = base + vocab.ID["<m:e2e4>"]
        want = 0.6 * self.old_embed[self.tok.encode("e2", add_special_tokens=False)].mean(0) \
            + 0.4 * self.old_embed[self.tok.encode("e4", add_special_tokens=False)].mean(0)
        self.assertTrue(torch.allclose(embed[move], want, atol=1e-6))
        reverse = base + vocab.ID["<m:e4e2>"]
        self.assertFalse(torch.allclose(embed[move], embed[reverse]))
        rows = embed[base:].detach()
        self.assertTrue(torch.isfinite(rows).all())
        self.assertGreater(rows.std(0).min().item(), 0.0)

    def test_init_is_deterministic(self):
        a = self.tok
        base = hf.extend_tokenizer(a)
        m1, m2 = tiny_model(self.host_n), tiny_model(self.host_n)
        for m in (m1, m2):
            m.resize_token_embeddings(base + vocab.SIZE)
            hf.init_new_rows(m.get_input_embeddings().weight.data, a, base, seed=5)
        self.assertTrue(torch.equal(m1.get_input_embeddings().weight, m2.get_input_embeddings().weight))

    def test_save_and_reload_is_equivalent(self):
        base = self.extend()
        ids = sample_ids(base)
        with torch.no_grad():
            before = self.model(input_ids=ids).logits
        with tempfile.TemporaryDirectory(prefix="ctok-hf-", dir=SCRATCH) as tmp:
            self.model.save_pretrained(tmp)
            self.tok.save_pretrained(tmp)
            model2 = Idefics3ForConditionalGeneration.from_pretrained(tmp).eval()
            tok2 = PreTrainedTokenizerFast.from_pretrained(tmp)
        self.assertEqual(len(tok2), len(self.tok))
        text = "<pos><side:b><sq:e4><m:e7e8q>"
        self.assertEqual(tok2.encode(text, add_special_tokens=False), self.tok.encode(text, add_special_tokens=False))
        for (n1, p1), (n2, p2) in zip(self.model.state_dict().items(), model2.state_dict().items()):
            self.assertEqual(n1, n2)
            self.assertTrue(torch.equal(p1, p2), n1)
        with torch.no_grad():
            after = model2(input_ids=ids).logits
        self.assertTrue(torch.equal(before, after))

    @unittest.skipIf(peft is None, "peft not installed")
    def test_lora_with_modules_to_save_reloads_on_a_resized_base(self):
        base = self.extend()
        ids = sample_ids(base)
        cfg = peft.LoraConfig(r=4, lora_alpha=8, bias="none",
                              target_modules=r".*text_model.*\.(q_proj|k_proj|v_proj|o_proj)",
                              modules_to_save=["embed_tokens", "lm_head"])
        wrapped = peft.get_peft_model(self.model, cfg)
        trainable = {n for n, p in wrapped.named_parameters() if p.requires_grad}
        self.assertTrue(any("embed_tokens" in n for n in trainable))
        self.assertTrue(any("lm_head" in n for n in trainable))
        # Stand-in for training: move the LoRA B matrices and the new rows.
        with torch.no_grad():
            for n, p in wrapped.named_parameters():
                if p.requires_grad and "lora_B" in n:
                    p.add_(0.05 * torch.randn_like(p))
                if p.requires_grad and ("embed_tokens" in n or "lm_head" in n):
                    p.add_(0.01 * torch.randn_like(p))
        wrapped.eval()
        with torch.no_grad():
            trained = wrapped(input_ids=ids).logits
        with tempfile.TemporaryDirectory(prefix="ctok-peft-", dir=SCRATCH) as tmp:
            wrapped.save_pretrained(tmp)
            fresh = tiny_model(self.host_n)  # the same seeded, un-extended base the trained model started from
            fresh.resize_token_embeddings(base + vocab.SIZE)  # resize on load, as the spec says
            reloaded = peft.PeftModel.from_pretrained(fresh, tmp).eval()
        with torch.no_grad():
            again = reloaded(input_ids=ids).logits
        self.assertTrue(torch.allclose(trained, again, atol=1e-5), (trained - again).abs().max().item())


@unittest.skipIf(torch is None or not REAL_TOKENIZER.exists(), "SmolVLM tokenizer snapshot not on disk")
class RealSmolVlmTokenizer(unittest.TestCase):
    def test_extension_lands_after_the_padded_vocabulary(self):
        from transformers import AutoTokenizer

        tok = AutoTokenizer.from_pretrained(REAL_TOKENIZER)
        host_n = len(tok)
        self.assertEqual(host_n, 49280)  # config vocab_size: no free padding rows to reuse
        base = hf.extend_tokenizer(tok)
        self.assertEqual((base, len(tok)), (49280, 49280 + vocab.SIZE))
        ids = tok.encode("<m:e2e4><sq:e4>", add_special_tokens=False)
        self.assertEqual(ids, [base + vocab.ID["<m:e2e4>"], base + vocab.ID["<sq:e4>"]])
        self.assertEqual(len(tok.encode("e2e4", add_special_tokens=False)), 4)


if __name__ == "__main__":
    unittest.main()
