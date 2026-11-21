"""Local SmolVLM move prediction: base weights, optionally plus a LoRA adapter.

Engine-free by construction (KLM-WAR-0005): nothing here imports or reaches a
chess engine. The legal-move list comes from the request packet only.

Modes
- "raw": free greedy decode with the settings the training harness uses for
  holdout generation (do_sample=False, max_new_tokens=10, strip of the decoded
  reply). The reply is reported untouched; `legal` says whether it is in the
  packet's legal_moves. Nothing is repaired.
- "constrained": the same greedy decode, but each step may only emit a token
  that continues the tokenisation of some legal move (or the end token once a
  move is complete). One `generate` call with `prefix_allowed_tokens_fn`: the
  same prompt, weights and decode path as raw, a legal move guaranteed, and no
  per-candidate forward passes. It is greedy over the legal-move token trie, not
  an exact argmax of whole-sequence probability; that is the price of the
  simplicity. The app's GBNF grammar (crates/ai/src/play.rs) restricts the same way.
"""

from dataclasses import dataclass
from pathlib import Path
import json
import threading
import time

from model.corpus.pipeline import sha256_file
from model import prompt as prompt_module
from model.prompt import mode_chat, packet_json, play_prompt, user_chat
from model.serve import play as play_module

PROFILE = Path(__file__).resolve().parents[1] / "configs/models/smolvlm-500m.json"
WEIGHTS_SHA256 = "d05b567eeaf534e83d375551f068ed57b5f52d37c657197f644af5ef9db091a2"
MAX_NEW_TOKENS = 10  # as model/train/harness.py holdout generation
MODES = ("raw", "constrained")
PLAY_FORCED_TOKENS = 160  # budget for reasoning written after a supplied call head
COACH_MAX_NEW_TOKENS = 512  # provisional lines, a short reply and a few tool calls
PLAY_MAX_NEW_TOKENS = 256  # a 400-character reasoning, the call wrapper and a UCI move fit with room


@dataclass
class Prediction:
    move: str
    raw_reply: str
    legal: bool
    input_tokens: int
    output_tokens: int
    latency_ms: float


@dataclass
class CoachGeneration:
    """One free coach completion: the raw text (`Provisional:` lines, reply, tool calls)."""
    text: str
    input_tokens: int
    output_tokens: int


@dataclass
class PlayPrediction:
    """One play-mode answer. `legal` is true only when `code` is "ok"; `path` says who chose the move."""
    move: str | None
    reasoning: str | None
    raw_reply: str
    legal: bool
    code: str
    path: str
    raw_code: str
    stated_move: str | None
    input_tokens: int
    output_tokens: int
    latency_ms: float
    head_move: str | None = None  # the move head's pick, when a head chose the move (model.serve.head)


def build_trie(sequences):
    """Prefix tree over token-id sequences: {token: subtree}; {} marks a leaf."""
    root = {}
    for seq in sequences:
        node = root
        for token in seq:
            node = node.setdefault(token, {})
    return root


def allowed_next(trie, generated):
    """Token ids that may follow `generated` inside `trie` ([] if off the trie)."""
    node = trie
    for token in generated:
        node = node.get(token)
        if node is None:
            return []
    return list(node)


class Predictor:
    n_board_tokens = prompt_module.BOARD_TOKENS
    _board = None  # (encoder, projector) when serving the board-encoder arms (play-enc, play-enc-cgt)
    _soft = None   # the current request's soft tokens, set only inside predict_play
    _play_lock = threading.RLock()

    def __init__(self, base_dir, adapter_dir=None, device="cuda", prompt_kind="play",
                 encoder_dir=None, projector_dir=None):
        import torch
        from transformers import AutoModelForImageTextToText, AutoProcessor

        prompt_module.check_play_kind(prompt_kind)
        self.prompt_kind = prompt_kind  # which play prompt (FEN packet or CGT) predict_play speaks
        self.base_dir = Path(base_dir)
        self.adapter_dir = Path(adapter_dir) if adapter_dir is not None else None
        self.device = device
        self.base_sha256 = sha256_file(self.base_dir / "model.safetensors")
        if self.base_sha256 != WEIGHTS_SHA256:
            profile = json.loads(PROFILE.read_text())
            raise ValueError(f"base weights do not match {profile['repository']}@{profile['revision']}")
        self.processor = AutoProcessor.from_pretrained(self.base_dir, local_files_only=True)
        self.tokenizer = self.processor.tokenizer
        model = AutoModelForImageTextToText.from_pretrained(
            self.base_dir, local_files_only=True, dtype=torch.bfloat16,
        ).to(device)
        self.adapter_sha256 = None
        if self.adapter_dir is not None:
            import peft
            self.adapter_sha256 = sha256_file(self.adapter_dir / "adapter_model.safetensors")
            model = peft.PeftModel.from_pretrained(model, self.adapter_dir)
        self.model = model.eval()
        self._torch = torch
        self.encoder_dir = Path(encoder_dir) if encoder_dir is not None else None
        self.projector_dir = Path(projector_dir) if projector_dir is not None else None
        if (self.encoder_dir is None) != (self.projector_dir is None):
            raise ValueError("a board encoder and its projector are loaded together: pass both or neither")
        if self.encoder_dir is not None:
            self._load_board()

    def _load_board(self):
        """Load the frozen encoder and the host's projector (model/encoder), checked against this host."""
        from model.encoder.encoder import load_encoder, load_projector
        encoder = load_encoder(self.encoder_dir).to(self.device).eval()
        projector = load_projector(self.projector_dir).to(self.device).eval()
        for module in (encoder, projector):
            for p in module.parameters():
                p.requires_grad_(False)
        hidden = self.model.config.text_config.hidden_size
        if projector.host_hidden != hidden or projector.d_encoder != encoder.cfg.d_model:
            raise ValueError(f"projector maps {projector.d_encoder}->{projector.host_hidden}; the encoder emits "
                             f"{encoder.cfg.d_model} and this host reads {hidden}")
        self._board = (encoder, projector)
        self.n_board_tokens = encoder.cfg.n_tokens
        self.encoder_sha256 = sha256_file(self.encoder_dir / "encoder.pt")
        self.projector_sha256 = sha256_file(self.projector_dir / "projector.pt")
        from model.encoder import manifest as manifest_module
        check_pair = manifest_module.check_pair  # manifests are optional; a present one must match
        problems = check_pair(self.encoder_dir, self.projector_dir, getattr(self, "base_sha256", None))
        if problems:
            raise ValueError("board encoder and projector do not match: " + "; ".join(problems))
        adapter_dir = getattr(self, "adapter_dir", None)
        task = adapter_dir is not None and manifest_module.read_task(Path(adapter_dir).parent)
        if task:  # an adapter written by model.encoder.task names the projector it was trained with
            problems = manifest_module.check_task(
                task, base_sha256=getattr(self, "base_sha256", None), adapter_sha256=getattr(self, "adapter_sha256", None),
                encoder_sha256=self.encoder_sha256, projector_sha256=self.projector_sha256,
                prompt_kind=getattr(self, "prompt_kind", None))
            if problems:
                raise ValueError("adapter and board encoder do not match: " + "; ".join(problems))

    def _soft_tokens(self, packet):
        """(1, n_board_tokens, host_hidden) soft tokens for one packet's position."""
        from model.encoder.inject import soft_tokens
        from model.encoder.planes import packet_planes
        torch = self._torch
        planes = torch.tensor(packet_planes(packet), dtype=torch.float32, device=self.device).reshape(1, -1, 8, 8)
        with torch.inference_mode():
            return soft_tokens(self._board[0], self._board[1], planes)

    def _check_play_kind(self):
        kind = self.prompt_kind
        if kind in prompt_module.READS_KINDS:
            raise ValueError(f"prompt kind {kind!r} lets the model call read tools between turns; the Python "
                             "predictor serves one call, so it is a training and scoring kind only")
        if kind in prompt_module.HEAD_KINDS:
            raise ValueError(f"prompt kind {kind!r} needs model.serve.head.HeadPredictor (a move head)")
        if kind in prompt_module.ENC_KINDS and self._board is None:
            raise ValueError(f"prompt kind {kind!r} needs a board encoder and projector (encoder_dir, projector_dir)")
        if self._board is not None and kind not in prompt_module.ENC_KINDS:
            raise ValueError(f"an encoder is loaded but the prompt kind is {kind!r}; use one of {prompt_module.ENC_KINDS}")

    def manifest(self):
        """Hashes identifying exactly what answers: base, adapter, prompt, tokenizer, chat template."""
        manifest = {
            "base_sha256": self.base_sha256,
            "adapter_sha256": self.adapter_sha256 or "none",
            "prompt_sha256": sha256_file(Path(__file__).resolve().parents[1] / "prompt.py"),
            "tokenizer_sha256": sha256_file(self.base_dir / "tokenizer.json"),
            "chat_template_sha256": sha256_file(self.base_dir / "chat_template.json"),
        }
        if self.prompt_kind != "play":  # the FEN-packet manifest (and its hash) is unchanged
            manifest["play_prompt_kind"] = self.prompt_kind
            if self.prompt_kind not in ("play-head", "play-enc", "play-reads"):  # these never call the compiler
                manifest["compiler_sha256"] = sha256_file(prompt_module.knightlm_binary())
        if self._board is not None:
            manifest.update(encoder_sha256=self.encoder_sha256, projector_sha256=self.projector_sha256,
                            board_tokens=self.n_board_tokens)
        return manifest

    def _prompt_ids(self, packet):
        text = self.processor.apply_chat_template(
            user_chat(packet), tokenize=False, add_generation_prompt=True)
        return text, self.tokenizer.encode(text, add_special_tokens=False)

    def _candidate_ids(self, packet, prompt_text, prompt_ids, move):
        """Token ids the trained target `move` + EOS has after the prompt (as harness.encode)."""
        chat = user_chat(packet) + [{"role": "assistant", "content": [{"type": "text", "text": move}]}]
        answer = self.processor.apply_chat_template(chat, tokenize=False)
        ids = self.tokenizer.encode(answer, add_special_tokens=False)
        if ids[:len(prompt_ids)] != prompt_ids:
            raise ValueError("prompt and completion tokenize to different prefixes")
        end = ids.index(self.tokenizer.eos_token_id, len(prompt_ids)) + 1
        return ids[len(prompt_ids):end]

    def predict(self, packet, mode):
        if mode not in MODES:
            raise ValueError(f"mode must be one of {MODES}, got {mode!r}")
        legal_moves = packet.get("legal_moves")
        if mode == "constrained" and not legal_moves:
            raise ValueError("constrained mode needs a non-empty packet legal_moves")
        torch = self._torch
        started = time.perf_counter()
        prompt_text, prompt_ids = self._prompt_ids(packet)
        kwargs = dict(do_sample=False, max_new_tokens=MAX_NEW_TOKENS,
                      eos_token_id=self.tokenizer.eos_token_id,
                      pad_token_id=self.tokenizer.pad_token_id)
        if mode == "constrained":
            trie = build_trie(self._candidate_ids(packet, prompt_text, prompt_ids, m)
                              for m in legal_moves)
            width = len(prompt_ids)
            kwargs["prefix_allowed_tokens_fn"] = (
                lambda _batch, ids: allowed_next(trie, ids[width:].tolist()))
        input_ids = torch.tensor([prompt_ids], device=self.device)
        with torch.inference_mode():
            out = self.model.generate(
                input_ids=input_ids, attention_mask=torch.ones_like(input_ids), **kwargs)
        new = out[0, len(prompt_ids):].tolist()
        # Count through the first EOS (inclusive); anything after is padding.
        eos = self.tokenizer.eos_token_id
        output_tokens = new.index(eos) + 1 if eos in new else len(new)
        reply = self.tokenizer.decode(new, skip_special_tokens=True)
        move = reply.strip()
        latency_ms = (time.perf_counter() - started) * 1000
        return Prediction(
            move=move, raw_reply=reply,
            legal=bool(legal_moves) and move in legal_moves,
            input_tokens=len(prompt_ids), output_tokens=output_tokens, latency_ms=latency_ms)

    # --- Play mode (docs/model/modes-and-tools.md): play prompt, one submit_move call ---

    def _play_prompt_ids(self, packet, **prompt_kwargs):
        text = self.processor.apply_chat_template(
            mode_chat(play_prompt(self.prompt_kind, packet, n_board_tokens=self.n_board_tokens, **prompt_kwargs)),
            tokenize=False, add_generation_prompt=True)
        return text, self.tokenizer.encode(text, add_special_tokens=False)

    def _generate(self, prompt_ids, max_new_tokens, **extra):
        torch = self._torch
        input_ids = torch.tensor([prompt_ids], device=self.device)
        if self._soft is not None:
            # Board arms: the soft tokens are merged into the `<image>` slots (the documented idefics3 path),
            # then the host decodes from embeddings. `generate` returns only the new tokens, and a
            # prefix_allowed_tokens_fn is shown the prompt ids again so it sees what it sees without soft tokens.
            from model.encoder.inject import check_slots, injected_embeds
            check_slots(input_ids, self._image_token_id(), self._soft)
            fn = extra.pop("prefix_allowed_tokens_fn", None)
            if fn is not None:
                extra["prefix_allowed_tokens_fn"] = lambda batch, ids: fn(batch, torch.cat([input_ids[0], ids]))
            with torch.inference_mode():
                embeds = injected_embeds(self.model, input_ids, self._soft)
                out = self.model.generate(
                    inputs_embeds=embeds, attention_mask=torch.ones_like(input_ids), do_sample=False,
                    max_new_tokens=max_new_tokens, eos_token_id=self.tokenizer.eos_token_id,
                    pad_token_id=self.tokenizer.pad_token_id, **extra)
            new = out[0].tolist()
            eos = self.tokenizer.eos_token_id
            return new[:new.index(eos) + 1] if eos in new else new
        with torch.inference_mode():
            out = self.model.generate(
                input_ids=input_ids, attention_mask=torch.ones_like(input_ids), do_sample=False,
                max_new_tokens=max_new_tokens, eos_token_id=self.tokenizer.eos_token_id,
                pad_token_id=self.tokenizer.pad_token_id, **extra)
        new = out[0, len(prompt_ids):].tolist()
        eos = self.tokenizer.eos_token_id
        return new[:new.index(eos) + 1] if eos in new else new

    def _image_token_id(self):
        return self.model.config.image_token_id

    def last_hidden(self, prompt_ids, soft=None):
        """The host's last hidden state at the final prompt token, (1, H) float32: the move head's input.

        The contract (docs/model/move-head.md): `hidden_states[-1]` of the language model, after its final
        norm, at the last position of the chat-templated prompt. Soft tokens, when given, go through the
        documented `image_hidden_states` path (model/encoder/inject.py)."""
        from model.policy_head.head import last_hidden
        torch = self._torch
        if soft is None and self._image_token_id() in prompt_ids:
            raise ValueError("the prompt has <image> slots but no soft tokens were given")
        input_ids = torch.tensor([prompt_ids], device=self.device)
        mask = torch.ones_like(input_ids)
        with torch.inference_mode():
            if soft is None:
                out = self.model(input_ids=input_ids, attention_mask=mask, output_hidden_states=True)
            else:
                from model.encoder.inject import host_forward
                out = host_forward(self.model, input_ids, soft, attention_mask=mask, output_hidden_states=True)
        return last_hidden(out.hidden_states[-1], mask).float()

    def complete(self, prompt_text):
        """Raw greedy completion of an arbitrary prompt text in the play chat shape (S0 rules QA).

        Returns (reply, input_tokens, output_tokens, latency_ms). Never repaired or constrained."""
        started = time.perf_counter()
        text = self.processor.apply_chat_template(mode_chat(prompt_text), tokenize=False, add_generation_prompt=True)
        prompt_ids = self.tokenizer.encode(text, add_special_tokens=False)
        new = self._generate(prompt_ids, PLAY_MAX_NEW_TOKENS)
        reply = self.tokenizer.decode(new, skip_special_tokens=True)
        return reply, len(prompt_ids), len(new), (time.perf_counter() - started) * 1000

    def _free_play(self, packet):
        """Greedy free decode of the play completion, shared by raw and constrained (one-entry cache)."""
        key = packet_json(packet)
        if getattr(self, "_play_cache", (None, None))[0] == key:
            return self._play_cache[1]
        _, prompt_ids = self._play_prompt_ids(packet)
        new = self._generate(prompt_ids, PLAY_MAX_NEW_TOKENS)
        value = (prompt_ids, new, self.tokenizer.decode(new, skip_special_tokens=True))
        self._play_cache = (key, value)
        return value

    def _play_answer_ids(self, prompt_ids, packet, answer):
        chat = mode_chat(play_prompt(self.prompt_kind, packet)) + [{"role": "assistant", "content": [{"type": "text", "text": answer}]}]
        ids = self.tokenizer.encode(self.processor.apply_chat_template(chat, tokenize=False),
                                    add_special_tokens=False)
        if ids[:len(prompt_ids)] != prompt_ids:
            raise ValueError("prompt and completion tokenize to different prefixes")
        end = ids.index(self.tokenizer.eos_token_id, len(prompt_ids)) + 1
        return ids[len(prompt_ids):end]

    def _force_reasoning(self, packet, prompt_text, prompt_ids):
        """Supply the call head and let the model write the reasoning (greedy, stops at its closing quote)."""
        ids = self.tokenizer.encode(prompt_text + play_module.call_head(), add_special_tokens=False)
        if ids[:len(prompt_ids)] != prompt_ids:
            raise ValueError("prompt and call head tokenize to different prefixes")
        new = self._generate(ids, PLAY_FORCED_TOKENS, stop_strings=['"'], tokenizer=self.tokenizer)
        text = self.tokenizer.decode(new, skip_special_tokens=True)
        return play_module.reasoning_from_forced(text), len(new)

    def _constrain_move(self, packet, prompt_ids, prefix, suffix):
        """Choose the move field from the legal list after the model's own reasoning.

        Every legal move's full completion (prefix + move + suffix) is tokenised as the trainer
        would; the tokens they share are fed as given, and the first token where they differ
        starts the legal-move trie, decoded greedily as in `predict`. Returns (move, new_tokens).
        """
        legal = packet["legal_moves"]
        answers = {m: self._play_answer_ids(prompt_ids, packet, prefix + m + suffix) for m in legal}
        if len(answers) == 1:
            return legal[0], 0
        first = next(iter(answers.values()))
        shared = 0
        while all(len(a) > shared and a[shared] == first[shared] for a in answers.values()):
            shared += 1
        table = {tuple(a[shared:]): m for m, a in answers.items()}
        trie = build_trie(table)
        width = len(prompt_ids) + shared
        new = self._generate(
            prompt_ids + first[:shared], max(len(k) for k in table),
            prefix_allowed_tokens_fn=lambda _batch, ids: allowed_next(trie, ids[width:].tolist()))
        return table[tuple(new)], len(new)

    def predict_play(self, packet, mode):
        """Play-mode prediction. raw: the model's own completion, checked, never repaired.

        constrained: the same completion when it already passes every check (path "model");
        otherwise, if the reasoning is recoverable, the move field is chosen from the packet's
        legal list after that reasoning (path "constrained_move"); else the call head is supplied,
        the model writes the reasoning and the move is chosen the same way (path
        "constrained_forced_reasoning"); else no move is returned (code constrained_no_reasoning).
        An illegal move is never returned in constrained mode.
        """
        if mode not in MODES:
            raise ValueError(f"mode must be one of {MODES}, got {mode!r}")
        legal_moves = packet.get("legal_moves")
        if mode == "constrained" and not legal_moves:
            raise ValueError("constrained mode needs a non-empty packet legal_moves")
        self._check_play_kind()
        with self._play_lock:
            started = time.perf_counter()
            self._soft = self._soft_tokens(packet) if self._board is not None else None
            try:
                prompt_ids, new, text = self._free_play(packet)
                fields, extra = play_module.resolve(
                    text, legal_moves or [], mode,
                    lambda prefix, suffix: self._constrain_move(packet, prompt_ids, prefix, suffix),
                    lambda: self._force_reasoning(packet, self._play_prompt_ids(packet)[0], prompt_ids))
            finally:
                self._soft = None
            out = dict(fields, raw_reply=text, input_tokens=len(prompt_ids), output_tokens=len(new) + extra)
            return PlayPrediction(latency_ms=(time.perf_counter() - started) * 1000, **out)

    def predict_coach(self, prompt_text):
        """Free greedy decode of a coach turn (model/serve/coach.py reads the commit rule off it)."""
        text = self.processor.apply_chat_template(
            mode_chat(prompt_text), tokenize=False, add_generation_prompt=True)
        ids = self.tokenizer.encode(text, add_special_tokens=False)
        new = self._generate(ids, COACH_MAX_NEW_TOKENS)
        return CoachGeneration(self.tokenizer.decode(new, skip_special_tokens=True), len(ids), len(new))
