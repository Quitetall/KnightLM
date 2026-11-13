"""CPU preflight pieces that need the training stack (torch, transformers, peft).

`model/train/run.py` imports this lazily, so the corpus, base and config checks
run on an interpreter without a ML stack. The tokenisation, parity and
optimiser-step checks use the BLUT trainer's own data code
(`hf_image_text_data.py` / `hf_image_text_train.py` from blut-cookbook-standard),
so what is checked is what the GPU run will execute.
"""

import glob
import importlib
import os
from pathlib import Path
import re
import sys

REPO = Path(__file__).resolve().parents[2]


def locate_backend_python():
    """Directory holding hf_image_text_data.py: $BLUT_HF_PYTHON_DIR, else the
    cargo checkout of the rev pinned in apps/blut-knightlm/Cargo.toml."""
    env = os.environ.get("BLUT_HF_PYTHON_DIR")
    if env:
        return Path(env) if (Path(env) / "hf_image_text_data.py").is_file() else None
    rev = re.search(r'blut-cookbooks\.git", rev = "([0-9a-f]{40})"',
                    (REPO / "apps/blut-knightlm/Cargo.toml").read_text())
    pattern = str(Path.home() / ".cargo/git/checkouts/blut-cookbooks-*/*/src/backends/hf_trainer/python")
    found = sorted(glob.glob(pattern))
    preferred = [p for p in found if rev and Path(p).parts[-5].startswith(rev.group(1)[:7])]
    for candidate in preferred or found:
        if (Path(candidate) / "hf_image_text_data.py").is_file():
            return Path(candidate)
    return None


def load_backend(path):
    """Import the trainer's data and train modules (stdlib-only at import time)."""
    sys.path.insert(0, str(path))
    try:
        return importlib.import_module("hf_image_text_data"), importlib.import_module("hf_image_text_train")
    finally:
        sys.path.remove(str(path))


def load_processor(base_dir):
    from transformers import AutoProcessor
    return AutoProcessor.from_pretrained(base_dir, local_files_only=True)


def percentile(values, q):
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, int(q * len(ordered)))]


def cpu_steps(base_dir, regex, examples, data, train, rank, alpha, lr, steps=2, batch=2, seq_len=2048):
    """Two optimiser steps on CPU in float32 with the trainer's encode/collate."""
    import time
    import peft
    import torch
    from transformers import AutoModelForImageTextToText

    started = time.monotonic()
    torch.manual_seed(0)
    torch.set_num_threads(min(8, os.cpu_count() or 1))
    model = AutoModelForImageTextToText.from_pretrained(base_dir, local_files_only=True, dtype=torch.float32)
    resolved = train.resolve_lora_regex({"lora_target_regex": regex}, model.config.model_type,
                                        [n for n, _ in model.named_modules()])
    model = peft.get_peft_model(model, peft.LoraConfig(r=rank, lora_alpha=alpha, target_modules=resolved, bias="none"))
    params = [p for p in model.parameters() if p.requires_grad]
    optimizer = torch.optim.AdamW(params, lr=lr)
    before = [p.detach().clone() for p in params]
    usable = [e for e in examples if len(e[1]) <= seq_len and len(e[1]) > len(e[0])]
    if len(usable) < steps * batch:
        raise ValueError(f"need {steps * batch} usable rows for the CPU steps, have {len(usable)}")
    losses, norms = [], []
    model.train()
    for i in range(steps):
        cols = data.collate(usable[i * batch:(i + 1) * batch], 0)
        inputs = {k: torch.tensor(v) for k, v in cols.items()}
        optimizer.zero_grad(set_to_none=True)
        loss = model(**inputs, use_cache=False).loss
        if not torch.isfinite(loss):
            raise ValueError(f"non-finite loss {float(loss)} on CPU step {i + 1}")
        loss.backward()
        norm = float(torch.nn.utils.clip_grad_norm_(params, 1.0, error_if_nonfinite=True))
        optimizer.step()
        losses.append(float(loss.detach()))
        norms.append(norm)
    changed = any(not torch.equal(a, p.detach()) for a, p in zip(before, params, strict=True))
    if not changed or max(norms) <= 0:
        raise ValueError("LoRA parameters did not update on CPU (zero gradient)")
    return {"steps": steps, "batch": batch, "losses": losses, "grad_norms": norms,
            "trainable_parameters": sum(p.numel() for p in params), "lora_target_regex": resolved,
            "model_type": model.config.model_type, "seconds": round(time.monotonic() - started, 1)}
