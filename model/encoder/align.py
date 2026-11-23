"""Projector alignment: train the per-host projector with the host LLM and encoder frozen.

One step = soft tokens in the host's image slot, text prompt and completion as in S0 rules QA,
language-model loss on the completion only. Stage A trains the projector alone; stage B (not
here) adds the host LoRA through the BLUT recipe. Prepared, not run: docs/model/board-encoder.md.
"""

import argparse
import json
from pathlib import Path

import torch

from model.corpus.pipeline import sha256_file
from model.encoder.encoder import Projector, load_encoder, save_projector
from model.encoder.manifest import write_projector_manifest
from model.encoder.inject import host_forward, soft_tokens
from model.encoder.planes import build_planes


def freeze(module):
    module.eval()
    for p in module.parameters():
        p.requires_grad_(False)
    return module


def alignment_step(host, encoder, projector, planes, input_ids, labels, attention_mask=None):
    """Loss for one batch. Gradients reach only the projector (the host and encoder are frozen)."""
    soft = soft_tokens(encoder, projector, planes)
    return host_forward(host, input_ids, soft, attention_mask=attention_mask, labels=labels).loss


def encode_row(tokenizer, image_token_id, n_tokens, row, max_len=256):
    """`<image>` x n + prompt text + completion; labels mask everything but the completion. A prompt that already
    carries its `<image>` slots (the rows of `model.encoder.rows`, the serve prompt of play-enc) is used as it is."""
    body = tokenizer(row["prompt"], add_special_tokens=False)["input_ids"]
    slots = body.count(image_token_id)
    if slots not in (0, n_tokens):
        raise ValueError(f"row {row.get('id')}: {slots} <image> slots in the prompt, the encoder emits {n_tokens}")
    prompt = body if slots else [image_token_id] * n_tokens + body
    completion = tokenizer(row["completion"], add_special_tokens=False)["input_ids"] + [tokenizer.eos_token_id]
    ids = (prompt + completion)[:max_len]
    labels = ([-100] * len(prompt) + completion)[:max_len]
    return ids, labels


def align_projector(host, encoder, projector, tokenizer, rows, steps, batch, lr, device="cpu", log=None):
    """Train `projector` on `rows` with the host and encoder frozen; returns the final loss (or None for 0 steps).
    The loop `main` runs on the GPU, so the CPU tests exercise the same code."""
    n_tokens, image_id = encoder.cfg.n_tokens, host.config.image_token_id
    opt = torch.optim.AdamW(projector.parameters(), lr=lr, weight_decay=0.0)
    loss = None
    for step in range(steps):
        chunk = [rows[(step * batch + k) % len(rows)] for k in range(batch)]
        enc = [encode_row(tokenizer, image_id, n_tokens, r) for r in chunk]
        width = max(len(i) for i, _ in enc)
        pad = tokenizer.pad_token_id if tokenizer.pad_token_id is not None else tokenizer.eos_token_id
        ids = torch.tensor([i + [pad] * (width - len(i)) for i, _ in enc], device=device)
        lab = torch.tensor([x + [-100] * (width - len(x)) for _, x in enc], device=device)
        mask = torch.tensor([[1] * len(i) + [0] * (width - len(i)) for i, _ in enc], device=device)
        planes = torch.tensor([build_planes(r["fen"], r.get("repetition")) for r in chunk],
                              dtype=torch.float32, device=device).reshape(len(chunk), -1, 8, 8)
        loss = alignment_step(host, encoder, projector, planes, ids, lab, mask)
        opt.zero_grad()
        loss.backward()
        opt.step()
        if log is not None and step % 50 == 0:
            log({"step": step, "loss": round(loss.item(), 4)})
    return None if loss is None else loss.item()


def write_projector_dir(out, projector, encoder_dir, *, host_base_sha256, final_loss, steps, rows_sha256=None, train=None):
    """The projector layout the server loads (`projector.pt`, `projector.json`) plus its manifest."""
    save_projector(out, projector)
    write_projector_manifest(out, encoder_dir, host_base_sha256=host_base_sha256, final_loss=final_loss, steps=steps,
                             rows_sha256=rows_sha256, train=train)


def main(argv=None):
    from transformers import AutoModelForImageTextToText, AutoTokenizer
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--host", required=True, help="host model directory (SmolVLM-500M-Instruct snapshot)")
    ap.add_argument("--encoder", type=Path, required=True)
    ap.add_argument("--rows", type=Path, required=True, help='JSONL {"fen","repetition"?,"prompt","completion"}')
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--steps", type=int, default=2000)
    ap.add_argument("--batch", type=int, default=16)
    ap.add_argument("--lr", type=float, default=1e-3)
    ap.add_argument("--device", default="cuda")
    args = ap.parse_args(argv)
    tok = AutoTokenizer.from_pretrained(args.host)
    host = freeze(AutoModelForImageTextToText.from_pretrained(args.host, torch_dtype=torch.bfloat16).to(args.device))
    encoder = freeze(load_encoder(args.encoder)).to(args.device)
    projector = Projector(encoder.cfg.d_model, host.config.text_config.hidden_size).to(args.device)
    rows = [json.loads(line) for line in open(args.rows, encoding="utf-8") if line.strip()]
    final = align_projector(host, encoder, projector, tok, rows, args.steps, args.batch, args.lr, args.device,
                            log=lambda record: print(json.dumps(record), flush=True))
    weights = Path(args.host) / "model.safetensors"
    write_projector_dir(args.out, projector.cpu(), args.encoder,
                        host_base_sha256=sha256_file(weights) if weights.is_file() else None,
                        final_loss=final, steps=args.steps, rows_sha256=sha256_file(args.rows),
                        train={"batch": args.batch, "lr": args.lr, "device": args.device})
    print(json.dumps({"status": "ok", "steps": args.steps, "final_loss": round(final, 5) if final is not None else None}))


if __name__ == "__main__":
    main()
