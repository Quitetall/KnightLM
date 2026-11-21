"""Embedding extension for arms T1 and T3: new ctok rows trained without full embedding copies.

The ctok/1 tokens are appended after the host vocabulary (`model/tokenizer/hf.py`). A LoRA run must train the
new input rows (and, with an untied head, the new output rows) or the adapter cannot read its own input.
`modules_to_save=["embed_tokens", "lm_head"]` does that by training and saving full copies of both
matrices: (49280 + 2496) x 960 x 2 = 99.4M parameters on SmolVLM-500M. PEFT's `trainable_token_indices`
trains and saves only the listed rows (a delta over the frozen matrix): 2496 x 960 = 2.40M parameters per
matrix, 4.79M for the pair, and the host rows cannot move. That is the path here, chosen after the CPU
reload test in `test_ctok.py` (identical logits after save and reload, host rows byte-identical).

Both matrices by default: the T1/T3 completions never contain a ctok token, so output rows get gradient only
from the softmax. Left at their host-text initialisation they would sit close to the host rows they were
built from (`<sq:e4>` near "e4") and compete with them at decode time; training them lets the model push them
down. A serving layer should still mask the ctok ids when it samples.

    python -m model.train.ctok build-base --base-dir DIR --out DIR [--seed N]    # extended base snapshot, CPU

The base weights stay the pinned stock file: the extension is a deterministic function of (pinned base,
ctok vocabulary hash, seed), recorded in `ctok-extension.json` next to the extended snapshot.
Needs torch, transformers and peft (model/train/requirements.txt); nothing else imports it.
"""

import argparse
import hashlib
import json
from pathlib import Path

from model.tokenizer import hf, vocab

EXTENSION = "ctok-extension.json"
MODES = ("embed", "embed+head")


def extend_processor(processor):
    """Append the ctok vocabulary to the processor's tokenizer in place; returns `base`."""
    return hf.extend_tokenizer(processor.tokenizer)


def new_ids(base):
    return list(range(base, base + vocab.SIZE))


def extend_model(model, tokenizer, base, seed=0):
    """Resize both matrices to the extended tokenizer and initialise the new rows (host rows untouched).

    Tied embeddings share one matrix, so it is initialised once. Returns whether the head is untied.
    """
    model.resize_token_embeddings(len(tokenizer), mean_resizing=False)
    embed = model.get_input_embeddings().weight
    head = model.get_output_embeddings().weight
    untied = head.data_ptr() != embed.data_ptr()
    host_rows = embed.detach()[:base].clone()
    hf.init_new_rows(embed.data, tokenizer, base, seed)
    if untied:
        hf.init_new_rows(head.data, tokenizer, base, seed)
    if not bool((embed.detach()[:base] == host_rows).all()):
        raise ValueError("extending the embedding changed a host row")
    return untied


def trainable_token_indices(model, base, mode="embed+head"):
    """{module name: new row ids} for `LoraConfig(trainable_token_indices=...)`."""
    if mode not in MODES:
        raise ValueError(f"mode must be one of {MODES}")
    ids = new_ids(base)
    found = {}
    for name, module in model.named_modules():
        if name.endswith("embed_tokens") and hasattr(module, "weight") and module.weight.shape[0] >= base + vocab.SIZE:
            found.setdefault("embed_tokens", ids)
    if "embed_tokens" not in found:
        raise ValueError("no embed_tokens module with room for the ctok rows (extend the model first)")
    untied = model.get_output_embeddings().weight.data_ptr() != model.get_input_embeddings().weight.data_ptr()
    if mode == "embed+head" and untied:
        found["lm_head"] = ids
    return found


def lora_config(peft, model, regex, rank, alpha, base, mode="embed+head"):
    return peft.LoraConfig(r=rank, lora_alpha=alpha, target_modules=regex, bias="none",
                           trainable_token_indices=trainable_token_indices(model, base, mode))


def parameter_report(model):
    """Counts of trainable parameters by kind; the full-copy alternative is reported beside them."""
    lora = rows = other = 0
    for name, param in model.named_parameters():
        if not param.requires_grad:
            continue
        if "trainable_tokens_delta" in name:
            rows += param.numel()
        elif "lora_" in name:
            lora += param.numel()
        else:
            other += param.numel()
    hidden = model.get_input_embeddings().weight.shape[1]
    total_rows = model.get_input_embeddings().weight.shape[0]
    config = model.config
    tied = bool(getattr(getattr(config, "text_config", config), "tie_word_embeddings", False))
    untied = not tied
    return {"lora": lora, "new_token_rows": rows, "other": other, "trainable": lora + rows + other,
            "new_rows_per_matrix": vocab.SIZE * hidden, "head_untied": untied,
            "modules_to_save_alternative": total_rows * hidden * (2 if untied else 1)}


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def build_base(base_dir, out_dir, seed=0):
    """Write the extended base snapshot (tokenizer + resized, initialised weights) for a ctok1 run."""
    import torch
    from transformers import AutoModelForImageTextToText, AutoProcessor

    base_dir, out_dir = Path(base_dir), Path(out_dir)
    if out_dir.exists():
        raise ValueError(f"{out_dir} exists; the extended base is built into a new directory")
    processor = AutoProcessor.from_pretrained(base_dir, local_files_only=True)
    model = AutoModelForImageTextToText.from_pretrained(base_dir, local_files_only=True, dtype=torch.bfloat16)
    base = extend_processor(processor)
    untied = extend_model(model, processor.tokenizer, base, seed)
    out_dir.mkdir(parents=True)
    model.save_pretrained(out_dir)
    processor.save_pretrained(out_dir)
    weights = out_dir / "model.safetensors"
    record = {"format": "knightlm-ctok-extension/1", "spec_version": vocab.SPEC_VERSION,
              "vocab_sha256": vocab.vocab_sha256(), "vocab_size": vocab.SIZE, "base": base, "seed": seed,
              "head_untied": untied, "stock_base_dir": str(base_dir),
              "stock_weights_sha256": sha256_file(base_dir / "model.safetensors") if (base_dir / "model.safetensors").is_file() else None,
              "extended_weights_sha256": sha256_file(weights) if weights.is_file() else None,
              "init": "model/tokenizer/hf.py init_new_rows"}
    (out_dir / EXTENSION).write_text(json.dumps(record, indent=2) + "\n")
    return record


def check_extension(out_dir):
    """The recorded extension must match this checkout's vocabulary; refuse a snapshot from another one."""
    record = json.loads((Path(out_dir) / EXTENSION).read_text())
    if record["vocab_sha256"] != vocab.vocab_sha256() or record["vocab_size"] != vocab.SIZE:
        raise ValueError("the extended base was built for a different ctok vocabulary")
    return record


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    commands = parser.add_subparsers(dest="command", required=True)
    make = commands.add_parser("build-base")
    make.add_argument("--base-dir", type=Path, required=True)
    make.add_argument("--out", type=Path, required=True)
    make.add_argument("--seed", type=int, default=0)
    args = parser.parse_args(argv)
    print(json.dumps(build_base(args.base_dir, args.out, args.seed), indent=2))


if __name__ == "__main__":
    main()
