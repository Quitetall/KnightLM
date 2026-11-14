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
from model.prompt import user_chat

PROFILE = Path(__file__).resolve().parents[1] / "configs/models/smolvlm-500m.json"
WEIGHTS_SHA256 = "d05b567eeaf534e83d375551f068ed57b5f52d37c657197f644af5ef9db091a2"
MAX_NEW_TOKENS = 10  # as model/train/harness.py holdout generation
MODES = ("raw", "constrained")


@dataclass
class Prediction:
    move: str
    raw_reply: str
    legal: bool
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
