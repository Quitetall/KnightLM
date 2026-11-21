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
import time

from model.corpus.pipeline import sha256_file
from model.prompt import mode_chat, packet_json, play_text, user_chat
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
    def __init__(self, base_dir, adapter_dir=None, device="cuda"):
        import torch
        from transformers import AutoModelForImageTextToText, AutoProcessor

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

    def manifest(self):
        """Hashes identifying exactly what answers: base, adapter, prompt, tokenizer, chat template."""
        return {
            "base_sha256": self.base_sha256,
            "adapter_sha256": self.adapter_sha256 or "none",
            "prompt_sha256": sha256_file(Path(__file__).resolve().parents[1] / "prompt.py"),
            "tokenizer_sha256": sha256_file(self.base_dir / "tokenizer.json"),
            "chat_template_sha256": sha256_file(self.base_dir / "chat_template.json"),
        }

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

    def _play_prompt_ids(self, packet):
        text = self.processor.apply_chat_template(
            mode_chat(play_text(packet)), tokenize=False, add_generation_prompt=True)
        return text, self.tokenizer.encode(text, add_special_tokens=False)

    def _generate(self, prompt_ids, max_new_tokens, **extra):
        torch = self._torch
        input_ids = torch.tensor([prompt_ids], device=self.device)
        with torch.inference_mode():
            out = self.model.generate(
                input_ids=input_ids, attention_mask=torch.ones_like(input_ids), do_sample=False,
                max_new_tokens=max_new_tokens, eos_token_id=self.tokenizer.eos_token_id,
                pad_token_id=self.tokenizer.pad_token_id, **extra)
        new = out[0, len(prompt_ids):].tolist()
        eos = self.tokenizer.eos_token_id
        return new[:new.index(eos) + 1] if eos in new else new

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
        chat = mode_chat(play_text(packet)) + [{"role": "assistant", "content": [{"type": "text", "text": answer}]}]
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
        started = time.perf_counter()
        prompt_ids, new, text = self._free_play(packet)
        fields, extra = play_module.resolve(
            text, legal_moves or [], mode,
            lambda prefix, suffix: self._constrain_move(packet, prompt_ids, prefix, suffix),
            lambda: self._force_reasoning(packet, self._play_prompt_ids(packet)[0], prompt_ids))
        out = dict(fields, raw_reply=text, input_tokens=len(prompt_ids), output_tokens=len(new) + extra)
        return PlayPrediction(latency_ms=(time.perf_counter() - started) * 1000, **out)

    def predict_coach(self, prompt_text):
        """Free greedy decode of a coach turn (model/serve/coach.py reads the commit rule off it)."""
        text = self.processor.apply_chat_template(
            mode_chat(prompt_text), tokenize=False, add_generation_prompt=True)
        ids = self.tokenizer.encode(text, add_special_tokens=False)
        new = self._generate(ids, COACH_MAX_NEW_TOKENS)
        return CoachGeneration(self.tokenizer.decode(new, skip_special_tokens=True), len(ids), len(new))
