"""Resume support for the joint trainers (`model.policy_head.joint`, `model.encoder.task`).

Every `save_steps` optimiser steps a trainer writes one checkpoint, `<run>/resume/state.pt` (atomic: written beside it,
then renamed over the last one, so a kill mid-write leaves the previous checkpoint). It holds everything the next
step depends on:

  trainable weights   the host's trainable (adapter) parameters by name, and each extra module's state dict (head,
                      projector); the frozen base is never saved
  optimizer, scheduler  their state dicts (AdamW moments, learning-rate position)
  stream position     rows consumed from the deterministic row stream; resume replays the stream to that row
  counters            step, dropped rows, the first step's record
  rng                 torch (and CUDA) generator state
  config              the settings that fix the run; a resume with different settings is refused

Resuming replays the row stream (same seed, shuffle buffer and epochs give the same order), restores the state and
continues at step + 1, so the run is the one that was interrupted. On CPU, with the same torch build and thread count,
the resumed run's weights are bit-identical to the uninterrupted run's (tested on a tiny host,
`model.train.test_resume`). On a GPU, kernels that are not deterministic can differ in the last bits; the tolerance
is then the ordinary run-to-run noise of the same recipe, not something resume adds. A checkpoint is taken after a
step, so the interrupted step's window is recomputed, never skipped or doubled.
"""

import os
from pathlib import Path

FILE = "state.pt"
SCHEMA = "knightlm.train-resume/v1"
CONFIG_KEYS = ("mode", "epochs", "batch_size", "grad_accum", "seq_len", "lr", "head_lr_mult", "head_weight",
               "warmup_ratio", "lr_scheduler", "max_grad_norm", "seed", "shuffle_buffer", "max_steps", "rank", "alpha")


class ResumeRefused(Exception):
    """A resume that would not continue the same run."""


def fingerprint(cfg, **extra):
    """The settings that fix the run (JSON-comparable)."""
    out = {k: cfg.get(k) for k in CONFIG_KEYS if k in cfg}
    out.update(extra)
    return out


def trainable_state(model, **modules):
    """Host trainable parameters by name, and each extra module's state dict, as CPU copies."""
    state = {"host": {n: p.detach().cpu().clone() for n, p in model.named_parameters() if p.requires_grad}}
    for name, module in modules.items():
        state[name] = {k: v.detach().cpu().clone() for k, v in module.state_dict().items()}
    return state


def restore_state(state, model, **modules):
    named = dict(model.named_parameters())
    missing = [n for n in state["host"] if n not in named]
    if missing:
        raise ResumeRefused(f"the checkpoint has host parameters this model lacks: {missing[:3]}")
    for name, tensor in state["host"].items():
        named[name].data.copy_(tensor.to(named[name].device))
    for name, module in modules.items():
        module.load_state_dict(state[name])


def rng_state():
    import torch
    return {"cpu": torch.get_rng_state(),
            "cuda": torch.cuda.get_rng_state_all() if torch.cuda.is_available() else None}


def set_rng_state(state):
    import torch
    torch.set_rng_state(state["cpu"])
    if state.get("cuda") is not None and torch.cuda.is_available():
        torch.cuda.set_rng_state_all(state["cuda"])


def save(directory, *, step, weights, optimizer, scheduler, stream_consumed, counters, cfg):
    import torch
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    payload = {"schema": SCHEMA, "step": step, "weights": weights, "optimizer": optimizer.state_dict(),
               "scheduler": scheduler.state_dict(), "stream_consumed": stream_consumed, "counters": counters,
               "config": cfg, "rng": rng_state()}
    tmp = directory / (FILE + ".tmp")
    torch.save(payload, tmp)
    os.replace(tmp, directory / FILE)
    return directory / FILE


def load(directory):
    """The checkpoint dict, or None when there is none."""
    import torch
    path = Path(directory) / FILE
    if not path.is_file():
        return None
    payload = torch.load(path, map_location="cpu", weights_only=True)
    if payload.get("schema") != SCHEMA:
        raise ResumeRefused(f"{path}: schema {payload.get('schema')!r}, expected {SCHEMA!r}")
    return payload


def check_config(payload, cfg):
    """Refuse a resume whose settings differ from the checkpoint's (a different run, not a continuation)."""
    saved = payload["config"]
    diff = {k: (saved.get(k), cfg.get(k)) for k in sorted(set(saved) | set(cfg)) if saved.get(k) != cfg.get(k)}
    if diff:
        raise ResumeRefused("the checkpoint was written with other settings (saved, now): "
                            + ", ".join(f"{k}={v}" for k, v in diff.items()))


def fast_forward(stream, rows):
    """Advance a freshly built `RowStream` to the position the checkpoint recorded."""
    for _ in range(rows):
        if stream.next_line() is None:
            raise ResumeRefused(f"the row stream ended before the checkpoint's position ({rows} rows)")
