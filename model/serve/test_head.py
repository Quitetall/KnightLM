"""Move-head serving and board-encoder serving, CPU only.

Torch-free: the agreement and constrained logic (`resolve_head`), the masked pick and the server contract.
With torch and transformers: a tiny random SmolVLM host with a character-level fake tokenizer stands in for the
real model, so the plumbing is exercised (last hidden state, head pick, conditioned prompt, soft-token injection
through the idefics3 slot, a peft-wrapped host); nothing here says anything about chess strength.
"""

import tempfile
import unittest
from pathlib import Path

from model import prompt
from model.policy_head import space
from model.serve import head as head_module
from model.serve.head import HeadPick, pick_from_logits, resolve_head

try:
    import torch
except ImportError:  # CI's plain python has no torch
    torch = None

LEGAL = ["e2e4", "d2d4", "g1f3"]


def call(reasoning, move):
    return prompt.format_play_completion(reasoning, move)


def logits_with(favoured, value=9.0, illegal=None):
    out = [0.0] * space.SIZE
    out[space.INDEX[favoured]] = value
    if illegal:
        out[space.INDEX[illegal]] = 50.0
    return out


class PickTests(unittest.TestCase):
    def test_masked_pick_is_legal_and_raw_diagnostics_are_kept(self):
        pick = pick_from_logits(logits_with("d2d4", illegal="a1a2"), LEGAL)
        self.assertEqual(pick.move, "d2d4")
        self.assertEqual(set(pick.legal_logits), set(LEGAL))
        self.assertEqual(pick.raw_move, "a1a2")
        self.assertFalse(pick.raw_move_legal)
        self.assertGreater(pick.raw_illegal_mass, 0.99)
        self.assertAlmostEqual(sum(pick.probs.values()), 1.0)
        self.assertEqual(max(pick.probs, key=pick.probs.get), "d2d4")

    def test_refusals(self):
        with self.assertRaises(space.MoveSpaceError):
            pick_from_logits(logits_with("e2e4"), [])
        with self.assertRaises(space.MoveSpaceError):
            pick_from_logits(logits_with("e2e4"), ["zzzz"])
        with self.assertRaises(ValueError):
            pick_from_logits([0.0] * 10, LEGAL)


class ResolveTests(unittest.TestCase):
    def test_agreement_is_ok_on_the_model_path(self):
        fields, extra = resolve_head(call("White to move. I choose e2e4.", "e2e4"), LEGAL, "e2e4", "raw")
        self.assertEqual((fields["code"], fields["path"], fields["legal"], extra), ("ok", "model", True, 0))

    def test_a_different_move_field_is_a_mismatch_in_raw_and_not_repaired(self):
        fields, _ = resolve_head(call("A quiet developing move.", "d2d4"), LEGAL, "e2e4", "raw")
        self.assertEqual((fields["code"], fields["legal"], fields["path"]), ("stated_move_mismatch", False, "none"))
        self.assertEqual(fields["move"], "d2d4")  # what the model wrote, never replaced
        self.assertEqual(fields["stated_move"], None)

    def test_reasoning_that_names_another_move_is_a_mismatch(self):
        fields, _ = resolve_head(call("I choose d2d4.", "e2e4"), LEGAL, "e2e4", "raw")
        self.assertEqual(fields["code"], "stated_move_mismatch")
        self.assertEqual(fields["stated_move"], "d2d4")

    def test_constrained_sets_the_move_field_to_the_head_move(self):
        fields, extra = resolve_head(call("A quiet developing move.", "d2d4"), LEGAL, "e2e4", "constrained")
        self.assertEqual((fields["code"], fields["move"], fields["path"], fields["legal"], extra),
                         ("ok", "e2e4", "constrained_move", True, 0))
        self.assertEqual(fields["raw_code"], "stated_move_mismatch")

    def test_constrained_repairs_an_illegal_field_but_never_reasoning_that_disagrees(self):
        fixed, _ = resolve_head(call("A quiet developing move.", "a1a1"), LEGAL, "e2e4", "constrained")
        self.assertEqual((fixed["code"], fixed["move"], fixed["raw_code"]), ("ok", "e2e4", "illegal_move"))
        stuck, _ = resolve_head(call("I choose d2d4.", "d2d4"), LEGAL, "e2e4", "constrained")
        self.assertEqual((stuck["code"], stuck["legal"], stuck["path"]), ("stated_move_mismatch", False, "none"))
        self.assertEqual(stuck["stated_move"], "d2d4")

    def test_constrained_forces_reasoning_when_none_is_recoverable(self):
        forced, spent = resolve_head("no call here", LEGAL, "e2e4", "constrained",
                                     force=lambda: ("Develops the pawn.", 7))
        self.assertEqual((forced["code"], forced["path"], forced["move"], spent),
                         ("ok", "constrained_forced_reasoning", "e2e4", 7))
        none, _ = resolve_head("no call here", LEGAL, "e2e4", "constrained", force=lambda: (None, 3))
        self.assertEqual((none["code"], none["move"], none["legal"]), ("constrained_no_reasoning", None, False))
        none, _ = resolve_head("no call here", LEGAL, "e2e4", "constrained")
        self.assertEqual(none["code"], "constrained_no_reasoning")
        said, _ = resolve_head("no call here", LEGAL, "e2e4", "constrained", force=lambda: ("I play d2d4.", 4))
        self.assertEqual(said["code"], "stated_move_mismatch")

    def test_raw_never_repairs_and_bad_arguments_are_refused(self):
        fields, extra = resolve_head("no call here", LEGAL, "e2e4", "raw", force=lambda: ("x", 9))
        self.assertEqual((fields["code"], extra), ("not_one_call", 0))
        with self.assertRaises(ValueError):
            resolve_head(call("r", "e2e4"), LEGAL, "a1a1", "raw")  # the head's move must be legal
        with self.assertRaises(ValueError):
            resolve_head("", LEGAL, "e2e4", "greedy")


# --- tiny host ------------------------------------------------------------------------------

IMAGE, EOS, PAD, VOCAB = 319, 318, 317, 320
PACKET = {"schema_version": 1, "ruleset": "standard-untimed-v1", "fen": "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
          "legal_moves": ["e2e4", "d2d4", "g1f3"], "history_complete": True,
          "facts": {"repetition_count": 1, "in_check": False}}
OTHER = dict(PACKET, fen="rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1")


class FakeTokenizer:
    eos_token_id, pad_token_id = EOS, PAD

    def encode(self, text, add_special_tokens=False):
        ids, i = [], 0
        while i < len(text):
            for marker, token in (("<image>", IMAGE), ("<eos>", EOS)):
                if text.startswith(marker, i):
                    ids.append(token)
                    i += len(marker)
                    break
            else:
                ids.append(ord(text[i]) if ord(text[i]) < 256 else 63)
                i += 1
        return ids

    def decode(self, ids, skip_special_tokens=True):
        return "".join(chr(t) for t in ids if t < 256)


class FakeProcessor:
    def apply_chat_template(self, chat, tokenize=False, add_generation_prompt=False):
        out = ""
        for message in chat:
            text = message["content"][0]["text"]
            out += f"<u>{text}</u>" if message["role"] == "user" else f"<a>{text}<eos>"
        return out + ("<a>" if add_generation_prompt else "")


@unittest.skipIf(torch is None, "torch not installed")
class HostCase(unittest.TestCase):
    HIDDEN = 24

    def host(self, seed=0):
        from transformers import SmolVLMConfig, SmolVLMForConditionalGeneration
        torch.manual_seed(seed)
        cfg = SmolVLMConfig(
            vision_config=dict(hidden_size=16, intermediate_size=32, num_hidden_layers=1, num_attention_heads=2,
                               image_size=32, patch_size=8),
            text_config=dict(hidden_size=self.HIDDEN, intermediate_size=48, num_hidden_layers=2, num_attention_heads=4,
                             num_key_value_heads=2, vocab_size=VOCAB, max_position_embeddings=1024),
            image_token_id=IMAGE, scale_factor=2, vocab_size=VOCAB)
        return SmolVLMForConditionalGeneration(cfg).eval()

    def predictor(self, cls, kind, model=None, **attrs):
        predictor = object.__new__(cls)
        predictor.model = model or self.host()
        predictor.tokenizer, predictor.processor = FakeTokenizer(), FakeProcessor()
        predictor.device, predictor._torch, predictor.prompt_kind = "cpu", torch, kind
        for name, value in attrs.items():
            setattr(predictor, name, value)
        return predictor

    def head_predictor(self, seed=0, **kw):
        from model.policy_head.head import MoveHead
        torch.manual_seed(seed)
        head = MoveHead(self.HIDDEN).eval()
        return self.predictor(head_module.HeadPredictor, "play-head", head=head, **kw)


class HeadPredictorTests(HostCase):
    def scripted(self, predictor, text, seen):
        """Replace free generation by a scripted completion; record the prompts the host was given."""
        real_hidden = predictor.last_hidden

        def last_hidden(ids, soft=None):
            seen["head_read"] = predictor.tokenizer.decode(ids)
            return real_hidden(ids, soft)

        def generate(ids, max_new, **extra):
            seen["conditioned"] = predictor.tokenizer.decode(ids)
            return predictor.tokenizer.encode(text) + [EOS]

        predictor.last_hidden, predictor._generate = last_hidden, generate

    def test_logits_are_raw_over_the_move_space_and_the_pick_is_the_masked_argmax(self):
        predictor = self.head_predictor()
        raw = predictor.logits(PACKET)
        self.assertEqual(len(raw), space.SIZE)
        self.assertEqual(raw, predictor.logits(PACKET))  # deterministic
        pick = predictor.pick(PACKET)
        self.assertIn(pick.move, PACKET["legal_moves"])
        self.assertEqual(pick.move, max(PACKET["legal_moves"], key=lambda m: (raw[space.INDEX[m]], m)))
        self.assertEqual(set(pick.legal_logits), set(PACKET["legal_moves"]))
        self.assertNotEqual(raw, predictor.logits(OTHER))  # the hidden state follows the position

    def test_last_hidden_is_the_final_language_hidden_state_at_the_last_token(self):
        predictor = self.head_predictor()
        _, ids = predictor._play_prompt_ids(PACKET)
        out = predictor.model(input_ids=torch.tensor([ids]), output_hidden_states=True)
        torch.testing.assert_close(predictor.last_hidden(ids), out.hidden_states[-1][:, -1].float())
        self.assertEqual(tuple(predictor.last_hidden(ids).shape), (1, self.HIDDEN))

    def test_the_head_reads_a_prompt_without_its_move_and_the_language_stage_sees_it(self):
        predictor, seen = self.head_predictor(), {}
        pick = predictor.pick(PACKET)
        text = call(f"I choose {pick.move}.", pick.move)
        self.scripted(predictor, text, seen)
        out = predictor.predict_play(PACKET, "raw")
        self.assertTrue(seen["head_read"].endswith("Move head:</u><a>"))
        self.assertNotIn("Move head: ", seen["head_read"])
        self.assertTrue(seen["conditioned"].endswith(f"Move head: {pick.move}</u><a>"))
        self.assertEqual((out.code, out.move, out.head_move, out.path, out.legal), ("ok", pick.move, pick.move, "model", True))
        self.assertEqual(out.raw_reply, text)

    def test_a_disagreeing_completion_is_a_mismatch_never_replaced(self):
        predictor, seen = self.head_predictor(), {}
        pick = predictor.pick(PACKET)
        other = next(m for m in PACKET["legal_moves"] if m != pick.move)
        self.scripted(predictor, call("A quiet developing move.", other), seen)
        raw = predictor.predict_play(PACKET, "raw")
        self.assertEqual((raw.code, raw.legal, raw.move, raw.head_move), ("stated_move_mismatch", False, other, pick.move))
        con = predictor.predict_play(PACKET, "constrained")
        self.assertEqual((con.code, con.move, con.path, con.raw_code), ("ok", pick.move, "constrained_move", "stated_move_mismatch"))

    def test_a_real_greedy_decode_on_the_tiny_host_returns_a_checked_record(self):
        predictor = self.head_predictor()
        out = predictor.predict_play(PACKET, "raw")  # random weights: the text is noise, the record is honest
        self.assertIn(out.head_move, PACKET["legal_moves"])
        self.assertNotEqual(out.code, "ok")
        self.assertEqual(out.path, "none")
        self.assertGreater(out.input_tokens, 100)

    def test_refusals(self):
        predictor = self.head_predictor()
        with self.assertRaises(ValueError):
            predictor.predict_play(dict(PACKET, legal_moves=[]), "raw")
        with self.assertRaises(ValueError):
            predictor.predict_play(PACKET, "greedy")
        predictor.prompt_kind = "play"
        with self.assertRaises(ValueError):
            predictor.predict_play(PACKET, "raw")

    def test_head_roundtrip_and_width_check(self):
        from model.policy_head.head import MoveHead, load_head, save_head
        predictor = self.head_predictor()
        with tempfile.TemporaryDirectory() as tmp:
            save_head(tmp, predictor.head)
            loaded = load_head(tmp)
            hidden = torch.randn(2, self.HIDDEN)
            torch.testing.assert_close(loaded(hidden), predictor.head(hidden), rtol=0, atol=0)
            predictor.attach_head(tmp)
            self.assertEqual(predictor.logits(PACKET), predictor.logits(PACKET))
            self.assertEqual(len(predictor.head_sha256), 64)
            wide = Path(tmp) / "wide"
            save_head(wide, MoveHead(self.HIDDEN + 1))
            with self.assertRaises(ValueError):
                predictor.attach_head(wide)


@unittest.skipIf(torch is None, "torch not installed")
class BoardServingTests(HostCase):
    def board(self, seed=0, n_global=0, hidden=None):
        from model.encoder.encoder import BoardEncoder, EncoderConfig, Projector
        torch.manual_seed(seed)
        cfg = EncoderConfig.tiny()
        cfg.n_global = n_global
        encoder = BoardEncoder(cfg).eval()
        return encoder, Projector(cfg.d_model, hidden or self.HIDDEN).eval(), cfg.n_tokens

    def enc_predictor(self, kind="play-enc", model=None, n_global=0):
        from model.serve.inference import Predictor
        encoder, projector, n = self.board(n_global=n_global)
        return self.predictor(Predictor, kind, model=model, _board=(encoder, projector), n_board_tokens=n)

    def test_the_play_prompt_has_one_slot_per_soft_token(self):
        for n_global, tokens in ((0, 64), (16, 80)):
            predictor = self.enc_predictor(n_global=n_global)
            _, ids = predictor._play_prompt_ids(PACKET)
            self.assertEqual(ids.count(IMAGE), tokens)
            self.assertEqual(tuple(predictor._soft_tokens(PACKET).shape), (1, tokens, self.HIDDEN))

    def test_soft_tokens_reach_the_host_and_the_board_matters(self):
        predictor = self.enc_predictor()
        _, ids = predictor._play_prompt_ids(PACKET)
        a = predictor.last_hidden(ids, predictor._soft_tokens(PACKET))
        b = predictor.last_hidden(ids, predictor._soft_tokens(OTHER))
        self.assertFalse(torch.allclose(a, b))
        # the same ids with no soft tokens fail loudly: the host cannot merge slots it was not given
        with self.assertRaises(Exception):
            predictor.last_hidden(ids)

    def test_decoding_from_embeddings_matches_the_forward_pass(self):
        from model.encoder.inject import host_forward
        predictor = self.enc_predictor()
        _, ids = predictor._play_prompt_ids(PACKET)
        soft = predictor._soft_tokens(PACKET)
        with torch.no_grad():
            logits = host_forward(predictor.model, torch.tensor([ids]), soft).logits[0, -1]
        predictor._soft = soft
        new = predictor._generate(ids, 3)
        self.assertEqual(new[0], int(logits.argmax()))
        self.assertEqual(new, predictor._generate(ids, 3))  # greedy and repeatable
        # prefix_allowed_tokens_fn is shown prompt + generated, as on the plain path
        widths = []

        def allow(_batch, seen):
            widths.append(len(seen))
            return [65]

        out = predictor._generate(ids, 2, prefix_allowed_tokens_fn=allow)
        self.assertEqual(out[:2], [65, 65])
        self.assertEqual(widths[0], len(ids))

    def test_predict_play_runs_the_board_arm_end_to_end_and_clears_its_state(self):
        predictor = self.enc_predictor()
        out = predictor.predict_play(PACKET, "raw")
        self.assertIsNone(predictor._soft)
        self.assertEqual(out.path, "none")
        self.assertIsNone(out.head_move)
        self.assertGreater(out.input_tokens, 64)

    def test_a_peft_wrapped_host_takes_the_same_path(self):
        try:
            import peft
        except ImportError:
            self.skipTest("peft not installed")
        base = self.host()
        wrapped = peft.get_peft_model(base, peft.LoraConfig(r=2, target_modules=["q_proj", "v_proj"]))
        predictor = self.enc_predictor(model=wrapped.eval())
        predictor._soft = predictor._soft_tokens(PACKET)
        _, ids = predictor._play_prompt_ids(PACKET)
        self.assertEqual(len(predictor._generate(ids, 2)), 2)
        self.assertEqual(tuple(predictor.last_hidden(ids, predictor._soft).shape), (1, self.HIDDEN))

    def test_kind_and_encoder_must_agree(self):
        from model.serve.inference import Predictor
        plain = self.predictor(Predictor, "play-enc")
        with self.assertRaises(ValueError):
            plain.predict_play(PACKET, "raw")  # an enc kind with no board
        with self.assertRaises(ValueError):
            self.enc_predictor(kind="play").predict_play(PACKET, "raw")  # a board with a text kind
        with self.assertRaises(ValueError):
            self.predictor(Predictor, "play-head").predict_play(PACKET, "raw")  # a head kind with no head

    def test_loading_checks_the_projector_against_the_host(self):
        from model.encoder.encoder import save_encoder, save_projector
        from model.serve.inference import Predictor
        encoder, projector, _ = self.board()
        with tempfile.TemporaryDirectory() as tmp:
            save_encoder(Path(tmp) / "e", encoder)
            save_projector(Path(tmp) / "p", projector)
            predictor = self.predictor(Predictor, "play-enc", encoder_dir=Path(tmp) / "e", projector_dir=Path(tmp) / "p")
            predictor._load_board()
            self.assertEqual(predictor.n_board_tokens, 64)
            self.assertEqual(len(predictor.encoder_sha256), 64)
            self.assertTrue(all(not p.requires_grad for p in predictor._board[0].parameters()))
            bad = self.board(hidden=self.HIDDEN + 8)[1]
            save_projector(Path(tmp) / "bad", bad)
            predictor.projector_dir = Path(tmp) / "bad"
            with self.assertRaises(ValueError):
                predictor._load_board()


class ServerTests(unittest.TestCase):
    def client(self, predictor):
        try:
            from fastapi.testclient import TestClient
        except ImportError:
            self.skipTest("fastapi/httpx not installed")
        import model.serve.server as server
        return TestClient(server.create_app(predictor, "raw", "play"))

    class Fake:
        base_dir = None

        def __init__(self, with_head):
            if with_head:
                self.pick = self._pick

        def manifest(self):
            return {"base_sha256": "b", "adapter_sha256": "none", "prompt_sha256": "p", "tokenizer_sha256": "t",
                    "chat_template_sha256": "c", "head_sha256": "h"}

        def _pick(self, packet):
            return pick_from_logits(logits_with("d2d4"), packet["legal_moves"])

        def predict_play(self, packet, mode):
            from model.serve.inference import PlayPrediction
            text = call("I choose d2d4.", "d2d4")
            return PlayPrediction(move="d2d4", reasoning="I choose d2d4.", raw_reply=text, legal=True, code="ok",
                                  path="model", raw_code="ok", stated_move="d2d4", input_tokens=5, output_tokens=6,
                                  latency_ms=1.0, head_move="d2d4" if self.pick_on else None)

        pick_on = True

    def body(self):
        return {"packet": PACKET, "packet_format_version": 1}

    def test_head_endpoint_and_head_move_in_the_play_response(self):
        client = self.client(self.Fake(True))
        head = client.post("/v1/head", json=self.body())
        self.assertEqual(head.status_code, 200)
        data = head.json()
        self.assertEqual(data["move"], "d2d4")
        self.assertEqual(set(data["legal_logits"]), set(PACKET["legal_moves"]))
        self.assertEqual(data["raw_move"], "d2d4")
        play = client.post("/v1/play", json=self.body()).json()
        self.assertEqual((play["move"], play["head_move"], play["code"]), ("d2d4", "d2d4", "ok"))

    def test_without_a_head_the_play_response_is_unchanged_and_head_is_501(self):
        fake = self.Fake(False)
        fake.pick_on = False
        client = self.client(fake)
        self.assertEqual(client.post("/v1/head", json=self.body()).status_code, 501)
        play = client.post("/v1/play", json=self.body()).json()
        self.assertNotIn("head_move", play)
        self.assertEqual(play["stated_move"], "d2d4")

    def test_head_endpoint_surfaces_errors(self):
        fake = self.Fake(True)
        fake.pick = lambda packet: (_ for _ in ()).throw(RuntimeError("boom"))
        self.assertEqual(self.client(fake).post("/v1/head", json=self.body()).status_code, 500)


if __name__ == "__main__":
    unittest.main()
