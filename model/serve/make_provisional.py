"""Write the provisional checkpoint: a freshly initialised LoRA adapter (B=0) on the base.

The adapter is a no-op by construction. This script proves it on real holdout
prompts (max logit delta base vs base+adapter) and writes manifest.json, so the
untrained base and trained checkpoints load through one path (base + adapter).

  python -m model.serve.make_provisional --corpus CORPUS --model-dir BASE --output NEW_DIR
"""

import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import sys

from model.corpus.pipeline import sha256_file
from model.serve.holdout import read_holdout
from model.prompt import user_chat
from model.train.harness import PROFILE, WEIGHTS_SHA256, lora_config

ROOT = Path(__file__).resolve().parents[1]


def main():
    import peft
    import torch
    import transformers

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus", type=Path, required=True)
    parser.add_argument("--model-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True, help="new directory")
    parser.add_argument("--probes", type=int, default=5)
    parser.add_argument("--seed", type=int, default=42)
    args = parser.parse_args()
    from model.serve.inference import Predictor

    profile = json.loads(PROFILE.read_text())
    args.output.mkdir()
    torch.manual_seed(args.seed)
    base = Predictor(args.model_dir)  # verifies the base sha256
    config = lora_config()
    config.base_model_name_or_path = profile["repository"]
    config.revision = profile["revision"]
    peft_model = peft.get_peft_model(base.model, config)
    adapter = args.output / "adapter"
    peft_model.peft_config["default"].base_model_name_or_path = profile["repository"]
    peft_model.peft_config["default"].revision = profile["revision"]
    peft_model.save_pretrained(adapter)
    b_norm = max(float(p.detach().abs().max()) for n, p in peft_model.named_parameters() if "lora_B" in n)
    peft_model = peft_model.eval()

    holdout = read_holdout(args.corpus, args.probes)
    deltas = []
    for row in holdout[:args.probes]:
        text = base.processor.apply_chat_template(
            user_chat(row["packet"]), tokenize=False, add_generation_prompt=True)
        ids = torch.tensor([base.tokenizer.encode(text, add_special_tokens=False)], device="cuda")
        with torch.inference_mode():
            with peft_model.disable_adapter():
                ref = peft_model(input_ids=ids, use_cache=False).logits.float().cpu()
            got = peft_model(input_ids=ids, use_cache=False).logits.float().cpu()
        deltas.append(float((got - ref).abs().max()))
    # Independent check through the real load path: fresh base + saved adapter.
    del peft_model, base
    torch.cuda.empty_cache()
    plain = Predictor(args.model_dir)
    wrapped = Predictor(args.model_dir, adapter)
    path_deltas = []
    for row in holdout[:args.probes]:
        text = plain.processor.apply_chat_template(
            user_chat(row["packet"]), tokenize=False, add_generation_prompt=True)
        ids = torch.tensor([plain.tokenizer.encode(text, add_special_tokens=False)], device="cuda")
        with torch.inference_mode():
            a = plain.model(input_ids=ids, use_cache=False).logits.float().cpu()
            b = wrapped.model(input_ids=ids, use_cache=False).logits.float().cpu()
        path_deltas.append(float((a - b).abs().max()))

    manifest = {
        "evidence": "provisional_untrained_base_checkpoint",
        "created_utc": datetime.now(timezone.utc).isoformat(),
        "note": "Freshly initialised LoRA adapter (lora_B == 0): functionally the untrained base.",
        "base": {"repository": profile["repository"], "revision": profile["revision"],
                 "sha256": WEIGHTS_SHA256},
        "adapter_sha256": sha256_file(adapter / "adapter_model.safetensors"),
        "adapter_config_sha256": sha256_file(adapter / "adapter_config.json"),
        "prompt_sha256": sha256_file(ROOT / "prompt.py"),
        "harness_sha256": sha256_file(ROOT / "train/harness.py"),
        "tokenizer_sha256": sha256_file(args.model_dir / "tokenizer.json"),
        "chat_template_sha256": sha256_file(args.model_dir / "chat_template.json"),
        "lora_B_max_abs": b_norm,
        "equivalence": {"probes": args.probes, "dtype": "bfloat16",
                        "max_logit_delta_adapter_disabled_vs_enabled": max(deltas),
                        "max_logit_delta_base_vs_saved_adapter_reload": max(path_deltas)},
        "environment": {"python": sys.version.split()[0], "torch": torch.__version__,
                        "transformers": transformers.__version__, "peft": peft.__version__,
                        "gpu": torch.cuda.get_device_name()},
    }
    (args.output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps(manifest, indent=2))


if __name__ == "__main__":
    main()
