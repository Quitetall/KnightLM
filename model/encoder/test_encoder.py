"""Board-encoder tests. The plane and label tests need no torch; the model tests skip without it."""

import tempfile
import unittest
from pathlib import Path

from model.encoder import labels as L
from model.encoder import planes as P

try:
    import torch
except ImportError:  # CI's plain python has no torch
    torch = None

START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
EP = "rnbqkbnr/ppp1pppp/8/8/3pP3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 3"


def plane(flat, k):
    return flat[k * 64:(k + 1) * 64]


class PlaneTests(unittest.TestCase):
    def test_start_position(self):
        flat = P.build_planes(START)
        self.assertEqual(len(flat), P.PLANES * 64)
        self.assertEqual(sum(sum(plane(flat, k)) for k in range(12)), 32)
        self.assertEqual(plane(flat, 0)[8:16], [1.0] * 8)           # white pawns on rank 2
        self.assertEqual(plane(flat, 5)[4], 1.0)                    # white king on e1
        self.assertEqual(plane(flat, 11)[60], 1.0)                  # black king on e8
        self.assertEqual(plane(flat, P.SIDE), [1.0] * 64)
        for k in range(4):
            self.assertEqual(plane(flat, P.CASTLE0 + k), [1.0] * 64)
        self.assertEqual(sum(plane(flat, P.EP)), 0)
        self.assertEqual(plane(flat, P.REP_KNOWN), [0.0] * 64)      # unknown stays unknown

    def test_state_planes(self):
        flat = P.build_planes(EP, repetition=2)
        self.assertEqual(plane(flat, P.SIDE), [0.0] * 64)
        self.assertEqual(plane(flat, P.EP)[P.square_index("e3")], 1.0)
        self.assertEqual(sum(plane(flat, P.EP)), 1.0)
        self.assertEqual(plane(flat, P.REP_KNOWN)[0], 1.0)
        self.assertAlmostEqual(plane(flat, P.REP)[0], 2 / 3)
        self.assertAlmostEqual(plane(flat, P.FULL)[0], 3 / 200)

    def test_refuses_what_it_would_have_to_invent(self):
        with self.assertRaises(ValueError):
            P.build_planes("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -")
        with self.assertRaises(ValueError):
            P.build_planes("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w HAha - 0 1")

    def test_packet_path(self):
        packet = {"fen": START, "history_complete": True, "facts": {"repetition_count": 1}}
        self.assertEqual(P.packet_planes(packet), P.build_planes(START, 1))
        packet["history_complete"] = False
        self.assertEqual(P.packet_planes(packet), P.build_planes(START, None))


class LabelTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.rows = L.read_labels(L.FIXTURE)

    def test_fixture_agrees_with_planes(self):
        """The compiler's piece-on-square labels equal the planes read from the same FEN."""
        self.assertEqual(len(self.rows), 200)
        for row in self.rows:
            flat = P.build_planes(row["fen"], row["repetition"])
            for sq, ch in enumerate(row["squares"]):
                got = [P.PIECES[k] for k in range(12) if plane(flat, k)[sq]]
                self.assertEqual(got, [] if ch == "." else [ch], (row["fen"], sq))

    def test_targets_shape(self):
        t = L.targets(self.rows[0])
        self.assertEqual((len(t["piece"]), len(t["attacked"]), len(t["legal"]), len(t["counts"])), (64, 2, 64, 10))
        self.assertEqual(sum(map(sum, t["legal"])), len(L.legal_pairs(self.rows[0])))

    def test_fixture_matches_the_compiler(self):
        exe = L.find_binary()
        if exe is None:
            self.skipTest("knightlm binary not built")
        fresh = L.compile_labels([r["fen"] for r in self.rows[:25]], exe)
        for old, new in zip(self.rows, fresh):
            self.assertEqual({k: old[k] for k in new}, new)


    def test_walk_rows_come_from_the_compiler(self):
        exe = L.find_binary()
        if exe is None:
            self.skipTest("knightlm binary not built")
        item = {"fen": START, "walk": 10, "seed": 3}
        a, b = L.compile_labels([item], exe)[0], L.compile_labels([item], exe)[0]
        self.assertEqual(a, b)
        self.assertEqual(a["source_fen"], START)
        self.assertNotEqual(a["fen"], START)
        self.assertEqual(L.compile_labels([a["fen"]], exe)[0]["squares"], a["squares"])


@unittest.skipIf(torch is None, "torch not installed")
class ModelTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from model.encoder import pretrain
        cls.pretrain = pretrain
        cls.rows = L.read_labels(L.FIXTURE)

    def cfg(self, **kw):
        from model.encoder.encoder import EncoderConfig
        cfg = EncoderConfig.tiny()
        cfg.__dict__.update(kw)
        return cfg

    def test_token_counts(self):
        from model.encoder.encoder import BoardEncoder
        for n_global, want in ((0, 64), (16, 80)):
            enc = BoardEncoder(self.cfg(n_global=n_global)).eval()
            planes, _ = self.pretrain.make_batch(self.rows[:2])
            self.assertEqual(tuple(enc(planes).shape), (2, want, 32))

    def test_heads_overfit_and_loss_decreases(self):
        torch.set_num_threads(4)
        train_rows = self.rows[:100]
        cfg = self.cfg(d_model=64, ff_mult=2)
        enc, heads, history = self.pretrain.train(train_rows, cfg, epochs=150, batch=20, lr=3e-3)
        first, last = history[0], history[-1]
        for head in ("piece", "attacked", "legal", "check", "counts"):
            self.assertLess(last[head], first[head], head)
        axis = self.pretrain.evaluate_heads(enc, heads, train_rows)
        h = axis["heads"]
        self.assertGreater(h["piece_on_square"]["square_accuracy"], 0.99)
        self.assertGreater(h["attacked_squares"]["square_accuracy"], 0.95)
        self.assertGreater(h["check"]["accuracy"], 0.95)
        self.assertGreater(h["material"]["counts_exact"], 0.9)
        self.assertGreater(h["legal_moves"]["recall"], 0.7)

    def test_eval_axis_has_no_composite(self):
        from model.encoder.encoder import BoardEncoder, RulesHeads
        enc, heads = BoardEncoder(self.cfg()), RulesHeads(32)
        axis = self.pretrain.evaluate_heads(enc, heads, self.rows[160:])
        self.assertEqual(axis["axis"], "board-encoder-rules-qa")
        self.assertEqual(set(axis["heads"]), set(self.pretrain.HEADS))
        self.assertEqual(axis["n_positions"], 40)
        flat = repr(axis)
        for banned in ("composite", "overall", "mean_accuracy", "score"):
            self.assertNotIn(f"'{banned}'", flat)

    def host(self, hidden=24, image_id=99, seed=0):
        from transformers import SmolVLMConfig, SmolVLMForConditionalGeneration
        torch.manual_seed(seed)
        cfg = SmolVLMConfig(
            vision_config=dict(hidden_size=16, intermediate_size=32, num_hidden_layers=1, num_attention_heads=2,
                               image_size=32, patch_size=8),
            text_config=dict(hidden_size=hidden, intermediate_size=48, num_hidden_layers=2, num_attention_heads=4,
                             num_key_value_heads=2, vocab_size=100, max_position_embeddings=256),
            image_token_id=image_id, scale_factor=2, vocab_size=100)
        return SmolVLMForConditionalGeneration(cfg).eval()

    def inputs(self, n_tokens):
        ids = torch.tensor([[1, 2] + [99] * n_tokens + [3, 4, 5]] * 2)
        planes, _ = self.pretrain.make_batch(self.rows[:2])
        return ids, planes

    def test_soft_tokens_reach_host_input_embeddings(self):
        from model.encoder.encoder import BoardEncoder, Projector
        from model.encoder.inject import host_forward, soft_tokens
        host = self.host()
        enc = BoardEncoder(self.cfg()).eval()
        proj = Projector(32, 24)
        ids, planes = self.inputs(64)
        seen = {}
        def capture(mod, args, kwargs):
            seen["embeds"] = kwargs["inputs_embeds"].detach().clone()

        hook = host.model.text_model.register_forward_pre_hook(capture, with_kwargs=True)
        with torch.no_grad():
            soft = soft_tokens(enc, proj, planes)
            out = host_forward(host, ids, soft)
        hook.remove()
        embeds, mask = seen["embeds"], ids == 99
        for b in range(2):
            torch.testing.assert_close(embeds[b][mask[b]], soft[b])             # soft tokens, in order
        text = host.model.get_input_embeddings()(ids)
        torch.testing.assert_close(embeds[~mask], text[~mask])                  # text positions untouched
        # And they matter: another board gives other logits.
        other, _ = self.pretrain.make_batch(self.rows[2:4])
        with torch.no_grad():
            out2 = host_forward(host, ids, soft_tokens(enc, proj, other))
        self.assertFalse(torch.allclose(out.logits, out2.logits))

    def test_slot_count_mismatch_is_refused(self):
        from model.encoder.inject import host_forward
        host = self.host()
        ids, _ = self.inputs(60)
        with self.assertRaises(ValueError):
            host_forward(host, ids, torch.zeros(2, 64, 24))

    def test_eighty_tokens_also_fit(self):
        from model.encoder.encoder import BoardEncoder, Projector
        from model.encoder.inject import host_forward, soft_tokens
        host, enc = self.host(), BoardEncoder(self.cfg(n_global=16)).eval()
        ids, planes = self.inputs(80)
        with torch.no_grad():
            out = host_forward(host, ids, soft_tokens(enc, Projector(32, 24), planes))
        self.assertEqual(out.logits.shape[:2], ids.shape)

    def test_reload_is_equivalent(self):
        from model.encoder.encoder import (BoardEncoder, Projector, load_encoder, load_projector, save_encoder,
                                           save_projector)
        from model.encoder.inject import host_forward, soft_tokens
        host = self.host()
        enc, proj = BoardEncoder(self.cfg(n_global=4)).eval(), Projector(32, 24).eval()
        ids, planes = self.inputs(68)
        with tempfile.TemporaryDirectory() as td:
            save_encoder(Path(td) / "enc", enc)
            save_projector(Path(td) / "proj", proj)
            enc2, proj2 = load_encoder(Path(td) / "enc").eval(), load_projector(Path(td) / "proj").eval()
        with torch.no_grad():
            a = host_forward(host, ids, soft_tokens(enc, proj, planes)).logits
            b = host_forward(host, ids, soft_tokens(enc2, proj2, planes)).logits
        torch.testing.assert_close(a, b, rtol=0, atol=0)

    def test_alignment_gradients_reach_only_the_projector(self):
        from model.encoder.align import alignment_step, freeze
        from model.encoder.encoder import BoardEncoder, Projector
        host, enc, proj = freeze(self.host()), freeze(BoardEncoder(self.cfg())), Projector(32, 24)
        ids, planes = self.inputs(64)
        labels = ids.clone()
        labels[ids == 99] = -100
        alignment_step(host, enc, proj, planes, ids, labels).backward()
        self.assertTrue(all(p.grad is None for p in host.parameters()))
        self.assertTrue(all(p.grad is None for p in enc.parameters()))
        self.assertTrue(all(p.grad is not None and p.grad.abs().sum() > 0 for p in proj.parameters()))


if __name__ == "__main__":
    unittest.main()
