"""Fine-tune the pinned SmolVLM with decoder-only LoRA on the Lichess pilot."""

import argparse
from datetime import datetime, timezone
import json
import math
from pathlib import Path
import random
import time

from model.corpus.pipeline import read_corpus, sha256_file

PROFILE = Path(__file__).resolve().parents[1] / "configs/models/smolvlm-500m.json"
WEIGHTS_SHA256 = "d05b567eeaf534e83d375551f068ed57b5f52d37c657197f644af5ef9db091a2"
LORA_TARGETS = r"model\.text_model\.layers\.\d+\.self_attn\.[qv]_proj"


def encode(row, processor):
    tokenizer = processor.tokenizer
    packet = json.dumps(row["packet"], sort_keys=True, separators=(",", ":"))
    chat = [{"role": "user", "content": [{"type": "text", "text": (
        "Choose a legal chess move. Reply with just the UCI move.\n" + packet
    )}]}]
    prompt = processor.apply_chat_template(chat, tokenize=False, add_generation_prompt=True)
    chat.append({"role": "assistant", "content": [{"type": "text", "text": row["target"]}]})
    answer = processor.apply_chat_template(chat, tokenize=False)
    prompt_ids = tokenizer.encode(prompt, add_special_tokens=False)
    ids = tokenizer.encode(answer, add_special_tokens=False)
    if ids[:len(prompt_ids)] != prompt_ids:
        raise ValueError("SmolVLM prompt and completion tokenize to different prefixes")
    end = ids.index(tokenizer.eos_token_id, len(prompt_ids)) + 1
    return prompt_ids, ids[:end]


def train(args, train_rows, holdout_rows):
    import peft
    import torch
    import transformers
    from transformers import AutoModelForImageTextToText, AutoProcessor

    started = datetime.now(timezone.utc).isoformat()
    clock = time.monotonic()
    if not torch.cuda.is_available() or not torch.cuda.is_bf16_supported():
        raise RuntimeError("this pilot requires CUDA with bfloat16 support")
    torch.manual_seed(args.seed)
    torch.cuda.reset_peak_memory_stats()
    profile = json.loads(PROFILE.read_text())
    if sha256_file(args.model_dir / "model.safetensors") != WEIGHTS_SHA256:
        raise ValueError(f"base weights do not match {profile['repository']}@{profile['revision']}")
    processor = AutoProcessor.from_pretrained(args.model_dir, local_files_only=True)
    tokenizer = processor.tokenizer
    train_examples = [encode(row, processor) for row in train_rows]
    holdout_examples = [encode(row, processor) for row in holdout_rows]
    random.Random(args.seed).shuffle(train_examples)

    def load_base():
        return AutoModelForImageTextToText.from_pretrained(
            args.model_dir, local_files_only=True, dtype=torch.bfloat16,
        ).to("cuda")

    def batches(examples):
        for start in range(0, len(examples), args.batch_size):
            yield examples[start:start + args.batch_size]

    def tensors(batch, *, generate=False):
        sequences = [prefix if generate else ids for prefix, ids in batch]
        width = max(map(len, sequences))
        input_ids, attention_mask, labels = [], [], []
        for (prefix, _), ids in zip(batch, sequences, strict=True):
            pad = [tokenizer.pad_token_id] * (width - len(ids))
            if generate:
                input_ids.append(pad + ids)
                attention_mask.append([0] * len(pad) + [1] * len(ids))
            else:
                input_ids.append(ids + pad)
                attention_mask.append([1] * len(ids) + [0] * len(pad))
                labels.append([-100] * len(prefix) + ids[len(prefix):] + [-100] * len(pad))
        values = {"input_ids": input_ids, "attention_mask": attention_mask}
        if not generate:
            values["labels"] = labels
        return {key: torch.tensor(value, device="cuda") for key, value in values.items()}

    @torch.inference_mode()
    def holdout_loss(model):
        loss_sum, tokens = 0.0, 0
        for batch in batches(holdout_examples):
            inputs = tensors(batch)
            count = int(inputs["labels"].ne(-100).sum())
            loss_sum += float(model(**inputs, use_cache=False).loss) * count
            tokens += count
        loss = loss_sum / tokens
        if not math.isfinite(loss):
            raise RuntimeError("non-finite holdout loss")
        return loss

    args.output.mkdir()
    config = peft.LoraConfig(r=8, lora_alpha=16, target_modules=LORA_TARGETS, bias="none")
    model = peft.get_peft_model(load_base(), config)
    parameters = [param for param in model.parameters() if param.requires_grad]
    initial = [param.detach().cpu().clone() for param in parameters]
    trainable = sum(param.numel() for param in parameters)
    optimizer = torch.optim.AdamW(parameters, lr=args.learning_rate)
    before_loss = holdout_loss(model.eval())
    print(f"Before training: holdout loss={before_loss:.4f}, LoRA parameters={trainable}", flush=True)

    model.train()
    max_gradient = 0.0
    losses = []
    for step, batch in enumerate(batches(train_examples), 1):
        optimizer.zero_grad(set_to_none=True)
        loss = model(**tensors(batch), use_cache=False).loss
        loss_value = float(loss.detach())
        if not math.isfinite(loss_value):
            raise RuntimeError(f"non-finite training loss at step {step}")
        loss.backward()
        norm = torch.nn.utils.clip_grad_norm_(parameters, max_norm=1.0, error_if_nonfinite=True)
        max_gradient = max(max_gradient, float(norm))
        optimizer.step()
        losses.append(loss_value)
        if step == 1 or step % 25 == 0 or step * args.batch_size >= len(train_examples):
            print(f"step {step}, games {min(step * args.batch_size, len(train_examples))}: loss={loss_value:.4f}", flush=True)
    changed = any(not torch.equal(old, new.detach().cpu()) for old, new in zip(initial, parameters, strict=True))
    if max_gradient <= 0 or not changed:
        raise RuntimeError("LoRA training did not update adapter weights")

    model.eval()
    probe = tensors(holdout_examples[:1], generate=True)
    with torch.inference_mode():
        expected = model(**probe, use_cache=False).logits[:, -1, :].float().cpu()
    adapter = args.output / "adapter"
    config.base_model_name_or_path = profile["repository"]
    config.revision = profile["revision"]
    model.save_pretrained(adapter)
    del model, optimizer, parameters
    reloaded = peft.PeftModel.from_pretrained(load_base(), adapter).eval()
    with torch.inference_mode():
        actual = reloaded(**probe, use_cache=False).logits[:, -1, :].float().cpu()
    torch.testing.assert_close(actual, expected, rtol=0, atol=0)
    after_loss = holdout_loss(reloaded)

    predictions = []
    with torch.inference_mode():
        for batch in batches(holdout_examples):
            inputs = tensors(batch, generate=True)
            generated = reloaded.generate(
                **inputs, do_sample=False, max_new_tokens=10,
                eos_token_id=tokenizer.eos_token_id, pad_token_id=tokenizer.pad_token_id,
            )
            predictions.extend(tokenizer.batch_decode(
                generated[:, inputs["input_ids"].shape[1]:], skip_special_tokens=True,
            ))
    legal, exact = 0, 0
    with (args.output / "holdout-predictions.jsonl").open("x") as stream:
        for row, reply in zip(holdout_rows, predictions, strict=True):
            move = reply.strip()
            is_legal = move in row["packet"]["legal_moves"]
            legal += is_legal
            exact += move == row["target"]
            stream.write(json.dumps({"game_id": row["game_id"], "reply": reply,
                                     "legal": is_legal, "target": row["target"]}) + "\n")

    receipt = {
        "evidence": "local_text_only_lora_pilot",
        "started_utc": started,
        "elapsed_seconds": round(time.monotonic() - clock, 2),
        "model": {"repository": profile["repository"], "revision": profile["revision"],
                  "base_sha256": WEIGHTS_SHA256,
                  "chat_template_sha256": sha256_file(args.model_dir / "chat_template.json"),
                  "tokenizer_sha256": sha256_file(args.model_dir / "tokenizer.json")},
        "corpus_manifest_sha256": sha256_file(args.corpus / "manifest.json"),
        "trainer_sha256": sha256_file(Path(__file__)),
        "adapter_sha256": sha256_file(adapter / "adapter_model.safetensors"),
        "adapter_config_sha256": sha256_file(adapter / "adapter_config.json"),
        "holdout_predictions_sha256": sha256_file(args.output / "holdout-predictions.jsonl"),
        "environment": {"gpu": torch.cuda.get_device_name(), "cuda": torch.version.cuda,
                        "torch": torch.__version__, "transformers": transformers.__version__,
                        "peft": peft.__version__},
        "config": {"batch_size": args.batch_size, "learning_rate": args.learning_rate,
                   "seed": args.seed, "epochs": 1, "rank": config.r, "alpha": config.lora_alpha,
                   "lora_targets": LORA_TARGETS},
        "train_games": len(train_rows), "holdout_games": len(holdout_rows),
        "optimizer_steps": len(losses), "trainable_parameters": trainable,
        "max_gradient_norm": max_gradient, "adapter_updated": changed,
        "mean_step_loss": sum(losses) / len(losses),
        "adapter_reload_max_logit_error": float((actual - expected).abs().max()),
        "holdout_loss_before": before_loss, "holdout_loss_after": after_loss,
        "unmasked_legal_with_legal_list": legal, "played_move_matches": exact,
        "peak_gpu_allocated_bytes": torch.cuda.max_memory_allocated(),
    }
    (args.output / "run-manifest.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(receipt, indent=2), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus", type=Path, required=True)
    parser.add_argument("--model-dir", type=Path, required=True, help="local pinned SmolVLM snapshot")
    parser.add_argument("--output", type=Path, required=True, help="new run directory")
    parser.add_argument("--batch-size", type=int, default=8)
    parser.add_argument("--learning-rate", type=float, default=2e-4)
    parser.add_argument("--seed", type=int, default=42)
    args = parser.parse_args()
    if args.batch_size < 1 or not math.isfinite(args.learning_rate) or args.learning_rate <= 0:
        parser.error("batch size and learning rate must be positive")
    if args.output.exists():
        parser.error("output must be a new directory")
    train_rows, holdout_rows = read_corpus(args.corpus)
    train(args, train_rows, holdout_rows)


if __name__ == "__main__":
    main()
