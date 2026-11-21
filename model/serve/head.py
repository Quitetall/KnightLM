"""Move-head serving: the head picks the move, the language head writes reasoning conditioned on it.

`HeadPredictor` is a serve `Predictor` (base plus optional LoRA adapter) with a trained move head
(model/policy_head, docs/model/move-head.md). Prompt kind `play-head` (model/prompt.py):

  1. head-read prompt (no move) -> host -> last hidden state -> head -> raw logits over the 1968 moves
  2. masked to the packet's legal set; the pick m* is the masked argmax (no engine, no sampling)
  3. the same prompt with `Move head: m*` -> the language head writes the play call (reasoning, move)
  4. agreement (model.policy_head.agree): a move field or stated move that differs from m* is
     `stated_move_mismatch`. raw: the completion as written. constrained: when the reasoning is
     recoverable the move field is set to m* (path constrained_move), or the reasoning is written after a
     supplied call head (constrained_forced_reasoning); a reasoning that names another move stays a
     mismatch. Nothing is repaired into agreement and there is no fallback move.

`resolve_head` is pure (no torch) and carries the whole decision logic; the tests drive it with scripted text.
"""

from dataclasses import dataclass, field
import time

from model.policy_head import agree, space
from model.serve import play as play_module
from model.serve.inference import MODES, PLAY_MAX_NEW_TOKENS, Predictor, PlayPrediction

KIND = "play-head"


@dataclass
class HeadPick:
    """The head's answer for one position. `legal_logits` are the masked logits over the legal set only."""
    move: str
    legal_logits: dict
    probs: dict
    raw_move: str
    raw_move_legal: bool
    raw_illegal_mass: float
    extra: dict = field(default_factory=dict)


def pick_from_logits(raw, legal_moves):
    """Masked argmax and diagnostics from raw 1968 logits (any float sequence). Refuses an empty or
    out-of-space legal set (space.MoveSpaceError) and a wrong length."""
    import math
    if len(raw) != space.SIZE:
        raise ValueError(f"logits must have {space.SIZE} entries, got {len(raw)}")
    legal_idx = space.indices(legal_moves)
    legal = {space.UCI[i]: float(raw[i]) for i in legal_idx}
    top = max(legal_idx, key=lambda i: (float(raw[i]), -i))
    peak = max(float(x) for x in raw)
    total = sum(math.exp(float(x) - peak) for x in raw)
    legal_mass = sum(math.exp(v - peak) for v in legal.values())
    probs = {m: math.exp(v - peak) / legal_mass for m, v in legal.items()}
    raw_top = max(range(space.SIZE), key=lambda i: (float(raw[i]), -i))
    return HeadPick(space.UCI[top], legal, probs, space.UCI[raw_top], space.UCI[raw_top] in legal,
                    1.0 - legal_mass / total)


def resolve_head(text, legal_moves, head_move, mode, force=None):
    """Decide what a play-head reply yields. Returns (fields, extra_tokens), as play.resolve.

    The agreement check is policy_head.agree.interpret_with_head. In constrained mode a refused completion whose
    reasoning is recoverable (or that `force() -> (reasoning, new_tokens)` can write after the call head) gets
    its move field set to `head_move`: that repairs a missing, illegal or other-move *field*, never the
    reasoning, so reasoning that states a different move is still `stated_move_mismatch`."""
    if mode not in MODES:
        raise ValueError(f"mode must be one of {MODES}, got {mode!r}")
    result = agree.interpret_with_head(text, legal_moves, head_move)
    fields = dict(move=result.move, reasoning=result.reasoning, legal=result.code == "ok", code=result.code,
                  path=play_module.PATH_MODEL if result.code == "ok" else play_module.PATH_NONE,
                  raw_code=result.code, stated_move=result.stated_move)
    if mode != "constrained" or result.code == "ok":
        return fields, 0
    found, path, spent = play_module.move_prefix(text), play_module.PATH_CONSTRAINED, 0
    if found is None and force is not None:
        reasoning, spent = force()
        around = play_module.prefix_for(reasoning)
        found = None if around is None else (reasoning, *around)
        path = play_module.PATH_FORCED
    if found is None:
        return dict(fields, move=None, reasoning=None, legal=False, code=play_module.NO_PREFIX), spent
    reasoning, _, _ = found
    stated = play_module.stated_move(reasoning)
    if stated is not None and stated != head_move:
        return dict(fields, move=head_move, reasoning=reasoning, legal=False, code="stated_move_mismatch",
                    path=play_module.PATH_NONE, stated_move=stated), spent
    return dict(fields, move=head_move, reasoning=reasoning, legal=True, code="ok", path=path,
                stated_move=stated), spent


class HeadPredictor(Predictor):
    """Predictor + move head. `predict_play` is the play-head path; `logits` feeds the move_head eval axis."""

    head = None

    def __init__(self, base_dir, adapter_dir=None, device="cuda", head_dir=None, **kwargs):
        if kwargs.pop("prompt_kind", KIND) != KIND:
            raise ValueError(f"HeadPredictor speaks prompt kind {KIND!r}")
        if kwargs.get("encoder_dir") is not None or kwargs.get("projector_dir") is not None:
            raise ValueError("the move head is not combined with the board encoder in this arm")
        super().__init__(base_dir, adapter_dir, device, prompt_kind=KIND, **kwargs)
        if head_dir is None:
            raise ValueError("HeadPredictor needs head_dir (a model.policy_head.head.save_head directory)")
        self.attach_head(head_dir)

    def attach_head(self, head_dir):
        from pathlib import Path
        from model.corpus.pipeline import sha256_file
        from model.policy_head.head import load_head
        self.head_dir = Path(head_dir)
        self.head_sha256 = sha256_file(self.head_dir / "head.pt")
        head = load_head(self.head_dir)
        hidden = self.model.config.text_config.hidden_size
        if head.norm.normalized_shape[0] != hidden:
            raise ValueError(f"head reads {head.norm.normalized_shape[0]}-wide states; this host has {hidden}")
        self.head = head.to(self.device)

    def manifest(self):
        return dict(super().manifest(), head_sha256=self.head_sha256, head_label="human-only move head")

    def _check_play_kind(self):
        if self.prompt_kind != KIND:
            raise ValueError(f"HeadPredictor speaks prompt kind {KIND!r}, not {self.prompt_kind!r}")

    def _raw_logits(self, packet):
        torch = self._torch
        _, ids = self._play_prompt_ids(packet)
        hidden = self.last_hidden(ids)
        with torch.inference_mode():
            return self.head(hidden.to(next(self.head.parameters()).dtype))[0].tolist(), len(ids)

    def logits(self, packet):
        """Raw, unmasked logits in model.policy_head.space order (the move_head eval axis input)."""
        return self._raw_logits(packet)[0]

    def pick(self, packet):
        """The head's masked pick with masked logits and diagnostics. Needs the packet's legal_moves."""
        if not packet.get("legal_moves"):
            raise ValueError("the move head needs a non-empty packet legal_moves")
        return pick_from_logits(self.logits(packet), packet["legal_moves"])

    def predict_play(self, packet, mode):
        if mode not in MODES:
            raise ValueError(f"mode must be one of {MODES}, got {mode!r}")
        legal_moves = packet.get("legal_moves")
        if not legal_moves:
            raise ValueError("the move head needs a non-empty packet legal_moves")
        with self._play_lock:
            started = time.perf_counter()
            raw, head_tokens = self._raw_logits(packet)
            pick = pick_from_logits(raw, legal_moves)
            text_prompt, prompt_ids = self._play_prompt_ids(packet, head_move=pick.move)
            new = self._generate(prompt_ids, PLAY_MAX_NEW_TOKENS)
            text = self.tokenizer.decode(new, skip_special_tokens=True)
            fields, extra = resolve_head(
                text, legal_moves, pick.move, mode,
                lambda: self._force_reasoning(packet, text_prompt, prompt_ids))
            return PlayPrediction(latency_ms=(time.perf_counter() - started) * 1000, raw_reply=text,
                                  input_tokens=head_tokens + len(prompt_ids), output_tokens=len(new) + extra,
                                  head_move=pick.move, **fields)
