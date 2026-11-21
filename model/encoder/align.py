"""Projector alignment: train the per-host projector with the host LLM and encoder frozen.

One step = soft tokens in the host's image slot, text prompt and completion as in S0 rules QA,
language-model loss on the completion only. Stage A trains the projector alone; stage B (not
here) adds the host LoRA through the BLUT recipe. Prepared, not run: docs/model/board-encoder.md.
"""

import argparse
import json
from pathlib import Path

import torch

from model.encoder.encoder import Projector, load_encoder, save_projector
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
    """`<image>` x n + prompt text + completion; labels mask everything but the completion."""
    prompt = [image_token_id] * n_tokens + tokenizer(row["prompt"], add_special_tokens=False)["input_ids"]
    completion = tokenizer(row["completion"], add_special_tokens=False)["input_ids"] + [tokenizer.eos_token_id]
    ids = (prompt + completion)[:max_len]
    labels = ([-100] * len(prompt) + completion)[:max_len]
    return ids, labels


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
    n_tokens, image_id = encoder.cfg.n_tokens, host.config.image_token_id
    rows = [json.loads(line) for line in open(args.rows, encoding="utf-8") if line.strip()]
    opt = torch.optim.AdamW(projector.parameters(), lr=args.lr, weight_decay=0.0)
    loss = None
    for step in range(args.steps):
        chunk = [rows[(step * args.batch + k) % len(rows)] for k in range(args.batch)]
        enc = [encode_row(tok, image_id, n_tokens, r) for r in chunk]
        width = max(len(i) for i, _ in enc)
        ids = torch.tensor([i + [tok.pad_token_id] * (width - len(i)) for i, _ in enc], device=args.device)
        lab = torch.tensor([x + [-100] * (width - len(x)) for _, x in enc], device=args.device)
        mask = torch.tensor([[1] * len(i) + [0] * (width - len(i)) for i, _ in enc], device=args.device)
        planes = torch.tensor([build_planes(r["fen"], r.get("repetition")) for r in chunk],
                              dtype=torch.float32, device=args.device).reshape(len(chunk), -1, 8, 8)
        loss = alignment_step(host, encoder, projector, planes, ids, lab, mask)
        opt.zero_grad()
        loss.backward()
        opt.step()
        if step % 50 == 0:
            print(json.dumps({"step": step, "loss": round(loss.item(), 4)}), flush=True)
    save_projector(args.out, projector)
    print(json.dumps({"status": "ok", "steps": args.steps, "final_loss": round(loss.item(), 5) if loss is not None else None}))


if __name__ == "__main__":
    main()
