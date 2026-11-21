import json
import unittest

from model.eval.fakes import fixture_rows
from model.policy_head import agree, metrics, space
from model.prompt import format_play_completion

try:
    import torch
    from model.policy_head import head as head_module
    from model.policy_head.toy import ToyHost
    HAVE = True
except ImportError:
    HAVE = False


class Space(unittest.TestCase):
    def test_size_and_roundtrip(self):
        self.assertEqual(space.SIZE, 1968)
        self.assertEqual(len(set(space.UCI)), 1968)
        self.assertIn("e7e8q", space.INDEX)
        self.assertIn("e1g1", space.INDEX)

    def test_every_fixture_legal_move_is_in_space(self):
        for row in fixture_rows():
            idx = space.indices(row["packet"]["legal_moves"])
            self.assertEqual(len(idx), len(row["packet"]["legal_moves"]))
            self.assertIn(row["target"], row["packet"]["legal_moves"])

    def test_refusals(self):
        with self.assertRaises(space.MoveSpaceError):
            space.indices([])
        with self.assertRaises(space.MoveSpaceError):
            space.indices(["e2e5x"])
        with self.assertRaises(space.MoveSpaceError):
            space.target_index("a1a1", ["e2e4"])


class Metrics(unittest.TestCase):
    def test_row_stats_planted(self):
        logits = [0.0] * space.SIZE
        legal = [space.INDEX[m] for m in ("e2e4", "d2d4", "g1f3")]
        logits[space.INDEX["a1a2"]] = 20.0  # illegal, dominates the raw softmax
        logits[space.INDEX["d2d4"]] = 2.0
        logits[space.INDEX["e2e4"]] = 1.0
        s = metrics.row_stats(logits, legal, space.INDEX["e2e4"])
        self.assertFalse(s["raw_top1_legal"])
        self.assertGreater(s["raw_illegal_mass"], 0.99)
        self.assertFalse(s["masked_top1_hit"])
        self.assertTrue(s["masked_top3_hit"])
        self.assertAlmostEqual(sum(metrics.masked_probs(logits, legal).values()), 1.0)

    def test_calibration_perfect_and_overconfident(self):
        self.assertEqual(metrics.calibration([1.0, 1.0], [True, True])["ece"], 0.0)
        self.assertAlmostEqual(metrics.calibration([0.95, 0.95], [True, False])["ece"], 0.45, places=3)


class Agreement(unittest.TestCase):
    legal = ["e2e4", "d2d4"]

    def test_agree_and_mismatch(self):
        ok = format_play_completion("I choose e2e4 for space.", "e2e4")
        self.assertEqual(agree.interpret_with_head(ok, self.legal, "e2e4").code, "ok")
        other = format_play_completion("I choose d2d4.", "d2d4")
        self.assertEqual(agree.interpret_with_head(other, self.legal, "e2e4").code, "stated_move_mismatch")
        split = format_play_completion("I choose d2d4.", "e2e4")  # stated differs from move field
        self.assertEqual(agree.interpret_with_head(split, self.legal, "e2e4").code, "stated_move_mismatch")

    def test_head_move_must_be_legal(self):
        with self.assertRaises(ValueError):
            agree.interpret_with_head("{}", self.legal, "a1a1")


class Preflight(unittest.TestCase):
    def test_counts_and_refuses_illegal_target(self):
        import tempfile
        from pathlib import Path
        from model.policy_head import train
        rows = [dict(r) for r in fixture_rows()[:3]]
        rows[2] = dict(rows[2], target="a1a1")
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / "train-00000.jsonl").write_text("".join(json.dumps(r) + "\n" for r in rows))
            plan = train.preflight(tmp, 10, 2)
        self.assertEqual((plan["rows_usable"], plan["rows_refused"], plan["epochs"]), (2, 1, 2))
        self.assertEqual(train.main(["--gpu"]), 3)


@unittest.skipUnless(HAVE, "torch not installed")
class HeadOnToyHost(unittest.TestCase):
    def batch(self, n=48, seed=1):
        g = torch.Generator().manual_seed(seed)
        ids = torch.randint(1, 64, (n, 12), generator=g)
        lengths = torch.randint(6, 13, (n,), generator=g)
        attn = (torch.arange(12)[None] < lengths[:, None])
        ids = ids * attn
        legal_lists, targets = [], []
        for row in range(n):
            k = int(torch.randint(8, 30, (1,), generator=g))
            legal = sorted(torch.randperm(space.SIZE, generator=g)[:k].tolist())
            pick = legal[int(ids[row, 0]) % k]  # a learnable function of the input
            legal_lists.append([space.UCI[i] for i in legal])
            targets.append(space.UCI[pick])
        return ids, attn, legal_lists, targets

    def test_loss_decreases_and_masked_argmax_legal(self):
        ids, attn, legal_lists, targets = self.batch()
        host = ToyHost(seed=0)
        head = head_module.MoveHead(host.hidden_size)
        mask = head_module.legal_mask(legal_lists)
        target = head_module.target_tensor(targets, legal_lists)
        opt = torch.optim.Adam(list(head.parameters()) + list(host.parameters()), lr=3e-3)
        losses = []
        for _ in range(60):
            opt.zero_grad()
            loss = head.loss(host(ids, attn), mask, target)
            loss.backward()
            opt.step()
            losses.append(loss.item())
        self.assertLess(losses[-1], losses[0] * 0.7)
        out = head.predict(host(ids, attn), mask)
        self.assertTrue(mask.gather(1, out["masked_argmax"][:, None]).all())
        self.assertTrue(mask.gather(1, out["masked_topk"]).all())
        self.assertTrue((out["masked_probs"][~mask] == 0).all())

    def test_masked_argmax_legal_for_untrained_head_and_raw_may_not_be(self):
        ids, attn, legal_lists, _ = self.batch(64, seed=2)
        host, head = ToyHost(seed=3), head_module.MoveHead(32)
        mask = head_module.legal_mask(legal_lists)
        out = head.predict(host(ids, attn), mask)
        self.assertTrue(mask.gather(1, out["masked_argmax"][:, None]).all())
        raw_legal = mask.gather(1, out["raw_argmax"][:, None]).float().mean().item()
        self.assertLess(raw_legal, 1.0)  # unmasked random head picks illegal moves: why the mask is the product

    def test_empty_legal_set_refused_and_pad_position(self):
        head = head_module.MoveHead(32)
        with self.assertRaises(ValueError):
            head_module.mask_logits(head(torch.zeros(1, 32)), torch.zeros(1, space.SIZE, dtype=torch.bool))
        states = torch.arange(2 * 4 * 3, dtype=torch.float).reshape(2, 4, 3)
        picked = head_module.last_hidden(states, torch.tensor([[1, 1, 0, 0], [1, 1, 1, 1]]))
        self.assertTrue(torch.equal(picked, torch.stack([states[0, 1], states[1, 3]])))

    def test_feeds_eval_axis(self):
        from model.eval import move_head
        rows = fixture_rows()
        host, head = ToyHost(seed=4), head_module.MoveHead(32)

        class P:
            def logits(self, packet):
                ids = torch.tensor([[1 + (ord(c) % 60) for c in packet["fen"][:12]]])
                with torch.no_grad():
                    return head(host(ids, torch.ones_like(ids)))[0].tolist()
        _, card = move_head.evaluate(rows, P())
        self.assertEqual(card["axes"]["move_head"]["masked"]["legality"]["rate"], 1.0)
        json.dumps(card)


if __name__ == "__main__":
    unittest.main()
