"""Joint training of the LoRA adapter and the move head (arm S1, human-only move head).

Prepared, not run on a GPU: `python -m model.policy_head.train --gpu` is the entry point; the CPU tests
drive `train_loop` on a tiny random host. Spec: docs/model/move-head.md; commands: docs/model/training-gate.md.

One corpus row gives two views (docs/model/move-head.md, "Inference order"):

  head-read prompt  = play_prompt("play-head", packet)                  -> host -> hidden_states[-1] at the last
                                                                           token -> head -> legal-masked CE
  conditioned       = play_prompt("play-head", packet, head_move=target) -> prompt of the language loss
  completion        = the unchanged play call (reasoning, then move == target) -> language loss

Both prompts are tokenised through `Predictor._play_prompt_ids`, the very function the server uses, so the
trainer cannot drift from the serve contract; the language example is built by the BLUT trainer's own
`hf_image_text_data.encode` and checked equal to the serve ids (`check_serve_parity`).

Why this is a loop here and not a recipe stage: the pinned BLUT image-text trainer (`hf_sft_train`) builds one
LoRA model and rejects `spec.extra`, so it cannot carry a second trainable module. The loop reuses everything
else the trainer does (row stream, encode, collate, LoRA config and regex, scheduler, token-weighted
accumulation, clipping, the adapter's `save_pretrained` layout) so an adapter written here loads like any
other. Output (`out_dir`):

  adapter/   adapter_model.safetensors + adapter_config.json     (serve --adapter-dir)
  head/      head.pt, head.json, manifest.json                   (serve --head-dir)
  train_summary.json
"""

from contextlib import nullcontext
import json
import math
import time
from pathlib import Path

from model import prompt as prompt_module
from model.corpus.pipeline import sha256_file
from model.corpus.render import render_completion
from model.policy_head import agree, manifest as manifest_module, space
from model.serve.inference import Predictor

KIND = "play-head"
MODES = ("joint", "head-only")


class Refused(Exception):
    """A check found a reason not to train; nothing was launched."""


# --- the two views of a row ------------------------------------------------------------------

def views(row, need_completion=True):
    """The prompts, completion, mask row and target of one corpus row. Refuses what the head cannot train on."""
    packet, target = row["packet"], row["target"]
    legal = packet["legal_moves"]
    space.target_index(target, legal)  # MoveSpaceError: empty set, move outside ctok/1, illegal target
    out = {"read_prompt": prompt_module.play_prompt(KIND, packet),
           "cond_prompt": prompt_module.play_prompt(KIND, packet, head_move=target),
           "packet": packet, "target": target, "legal": legal, "completion": None}
    if need_completion:
        if "reasoning" not in row:
            raise Refused(f"game {row.get('game_id')}: a joint row needs a reasoning field (use --mode head-only)")
        out["completion"] = render_completion(row["reasoning"], target)
    return out


def serve_stub(processor, n_board_tokens=prompt_module.BOARD_TOKENS):
    """A Predictor with only what `_play_prompt_ids` reads: the serve tokenisation, with no weights loaded."""
    stub = object.__new__(Predictor)
    stub.processor, stub.tokenizer = processor, processor.tokenizer
    stub.prompt_kind, stub.n_board_tokens = KIND, n_board_tokens
    return stub


def read_ids(stub, packet):
    """Token ids of the head-read prompt: exactly what `HeadPredictor.logits` feeds the host."""
    return stub._play_prompt_ids(packet)[1]


def check_serve_parity(rows, processor, data, need_completion=True):
    """For each row: head-read ids and conditioned ids equal the serve ids, the head-read text does not contain the
    move, the conditioned text is the head-read text plus ' ' + move, and the language example the trainer encodes
    starts with the serve conditioned ids and ends on the end-of-turn token."""
    stub = serve_stub(processor)
    for row in rows:
        v = views(row, need_completion)
        packet, target = v["packet"], v["target"]
        if not v["read_prompt"].endswith(prompt_module.HEAD_LABEL):
            raise Refused(f"head-read prompt of {row.get('game_id')} does not end in {prompt_module.HEAD_LABEL!r}")
        if v["cond_prompt"] != v["read_prompt"] + " " + target:
            raise Refused(f"conditioned prompt of {row.get('game_id')} is not head-read prompt + ' ' + move")
        serve_cond = stub._play_prompt_ids(packet, head_move=target)[1]
        serve_read = read_ids(stub, packet)
        if serve_read == serve_cond:
            raise Refused("head-read and conditioned prompts tokenise identically: the move is missing")
        if not need_completion:
            continue
        verdict = agree.interpret_with_head(v["completion"], v["legal"], target)
        if verdict.code != "ok":
            raise Refused(f"the completion of {row.get('game_id')} does not agree with its own target ({verdict.code})")
        prompt_ids, ids = data.encode(v["cond_prompt"], v["completion"], processor)
        if prompt_ids != serve_cond:
            raise Refused(f"trainer prompt ids differ from the serve conditioned ids for {row.get('game_id')}")
        if len(ids) <= len(prompt_ids):
            raise Refused(f"no completion tokens for {row.get('game_id')}")
    return {"status": "ok", "rows": len(rows),
            "contract": "head-read ids == Predictor._play_prompt_ids; conditioned prompt ids == trainer prompt ids; "
                        "completion agrees with the target"}


def check_mask_legality(rows, hidden_size=16, seed=0):
    """Mask row of every sample: non-empty, in ctok/1, contains the target. Then a fresh random head's masked
    argmax is legal for every row while the unmasked one need not be (the mask is the product)."""
    import torch
    from model.policy_head import head as head_module
    legal_lists = [r["packet"]["legal_moves"] for r in rows]
    targets = [r["target"] for r in rows]
    mask = head_module.legal_mask(legal_lists)
    target = head_module.target_tensor(targets, legal_lists)
    if not bool(mask.gather(1, target[:, None]).all()):
        raise Refused("a target is outside its own mask")
    for row, legal in zip(mask, legal_lists):
        if int(row.sum()) != len(set(legal)):
            raise Refused("a mask row does not match its packet's legal moves")
    torch.manual_seed(seed)
    head = head_module.MoveHead(hidden_size)
    out = head.predict(torch.randn(len(rows), hidden_size), mask)
    legal_pick = mask.gather(1, out["masked_argmax"][:, None]).all().item()
    illegal_mass = float(out["masked_probs"][~mask].sum())
    if not legal_pick or illegal_mass != 0.0:
        raise Refused("the masked argmax of a random head is illegal or puts mass on illegal moves")
    raw_legal = float(mask.gather(1, out["raw_argmax"][:, None]).float().mean())
    return {"status": "ok", "rows": len(rows), "masked_argmax_legal": 1.0, "illegal_mass_after_mask": illegal_mass,
            "raw_argmax_legal_for_random_head": round(raw_legal, 3)}


# --- training --------------------------------------------------------------------------------

def pad(seqs, pad_id):
    """Right-pad id lists: (ids, attention_mask) tensors, the last real token at attention_mask.sum() - 1."""
    import torch
    width = max(len(s) for s in seqs)
    ids = torch.tensor([s + [pad_id] * (width - len(s)) for s in seqs])
    attn = torch.tensor([[1] * len(s) + [0] * (width - len(s)) for s in seqs])
    return ids, attn


def head_forward(model, head, ids, attn, train_host):
    """Hidden state at the last prompt token (the serve contract: hidden_states[-1], final norm applied) -> head."""
    import torch
    from model.policy_head.head import last_hidden
    with (nullcontext() if train_host else torch.no_grad()):
        out = model(input_ids=ids, attention_mask=attn, output_hidden_states=True, use_cache=False)
    hidden = last_hidden(out.hidden_states[-1], attn)
    return head(hidden.float())


def make_batch(rows, processor, data, stub, mode, seq_len, device, pad_id):
    """One micro-batch: the head-read ids and mask/target tensors, and (joint) the language example tensors.
    Returns None-able pieces; rows longer than seq_len are dropped by the caller via `fits`."""
    import torch
    from model.policy_head import head as head_module
    vs = [views(r, need_completion=(mode == "joint")) for r in rows]
    read = [read_ids(stub, v["packet"]) for v in vs]
    legal = [v["legal"] for v in vs]
    batch = {"read": [torch.as_tensor(t, device=device) for t in pad(read, pad_id)],
             "mask": head_module.legal_mask(legal, device=device),
             "target": head_module.target_tensor([v["target"] for v in vs], legal, device=device), "rows": len(vs)}
    if mode == "joint":
        examples = [data.encode(v["cond_prompt"], v["completion"], processor) for v in vs]
        cols = data.collate(examples, pad_id)
        batch["lm"] = {k: torch.tensor(x, device=device) for k, x in cols.items()}
        batch["lm_tokens"] = int(batch["lm"]["labels"].ne(-100).sum())
        batch["fits"] = max(len(i) for _, i in examples) <= seq_len and max(len(t) for t in read) <= seq_len
    else:
        batch["lm_tokens"] = 0
        batch["fits"] = max(len(t) for t in read) <= seq_len
    return batch


def plan_steps(n_rows, epochs, batch_size, grad_accum, max_steps=None):
    per_step = batch_size * grad_accum
    return int(max_steps or epochs * (n_rows // per_step))


def train_loop(model, head, processor, data, shards, cfg, emit, device="cpu"):
    """The joint (or head-only) optimisation loop. `model` is the (peft-wrapped, for joint) host.

    cfg keys: mode, epochs, batch_size, grad_accum, seq_len, lr, head_lr_mult, head_weight, warmup_ratio,
    lr_scheduler, max_grad_norm, seed, shuffle_buffer, max_steps, save_steps, snapshot (callable(step) or None).
    Returns a summary dict. The caller saves the final adapter and head."""
    import torch
    import torch.nn.functional as F
    from transformers import get_scheduler
    from model.policy_head.head import mask_logits
    mode = cfg["mode"]
    if mode not in MODES:
        raise ValueError(f"mode must be one of {MODES}")
    joint = mode == "joint"
    tokenizer = processor.tokenizer
    pad_id = tokenizer.pad_token_id if tokenizer.pad_token_id is not None else tokenizer.eos_token_id
    stub = serve_stub(processor)
    head.to(device).train()
    groups = [{"params": list(head.parameters()), "lr": cfg["lr"] * cfg["head_lr_mult"]}]
    adapter = [p for p in model.parameters() if p.requires_grad] if joint else []
    if joint:
        if not adapter:
            raise Refused("no trainable adapter parameters: joint mode needs a peft-wrapped host")
        groups.append({"params": adapter, "lr": cfg["lr"]})
        model.train()
    else:
        model.eval()
        for p in model.parameters():
            p.requires_grad_(False)
    optimizer = torch.optim.AdamW(groups)
    n_rows = data.count_rows(shards)
    per_step = cfg["batch_size"] * cfg["grad_accum"]
    total = plan_steps(n_rows, cfg["epochs"], cfg["batch_size"], cfg["grad_accum"], cfg.get("max_steps"))
    if total < 1:
        raise Refused(f"{n_rows} rows x {cfg['epochs']} epochs is less than one step of {per_step} rows")
    scheduler = get_scheduler(cfg["lr_scheduler"], optimizer, num_warmup_steps=math.ceil(cfg["warmup_ratio"] * total),
                              num_training_steps=total)
    stream = data.RowStream(shards, cfg["seed"], cfg["shuffle_buffer"], cfg["epochs"])
    params = list(head.parameters()) + adapter
    first = [p.detach().cpu().clone() for p in params]
    dropped, step, last, start = 0, 0, {}, time.monotonic()
    history = []

    def next_micro():
        nonlocal dropped
        while True:
            rows = []
            while len(rows) < cfg["batch_size"]:
                line = stream.next_line()
                if line is None:
                    return None
                rows.append(json.loads(line))
            micro = make_batch(rows, processor, data, stub, mode, cfg["seq_len"], device, pad_id)
            if micro["fits"]:
                return micro
            dropped += len(rows)

    while step < total:
        window = []
        for _ in range(cfg["grad_accum"]):
            micro = next_micro()
            if micro is None:
                break
            window.append(micro)
        if len(window) < cfg["grad_accum"]:
            break
        n_tok = sum(m["lm_tokens"] for m in window)
        n_row = sum(m["rows"] for m in window)
        optimizer.zero_grad(set_to_none=True)
        lm_sum = head_sum = hit = 0.0
        for m in window:
            ids, attn = m["read"]
            raw = head_forward(model, head, ids, attn, train_host=joint)
            masked = mask_logits(raw, m["mask"])
            h_loss = F.cross_entropy(masked, m["target"])
            loss = cfg["head_weight"] * h_loss * (m["rows"] / n_row)
            head_sum += float(h_loss.detach()) * m["rows"]
            hit += float((masked.argmax(-1) == m["target"]).sum())
            if joint:
                lm_loss = model(**m["lm"], use_cache=False).loss
                loss = loss + lm_loss * (m["lm_tokens"] / n_tok)
                lm_sum += float(lm_loss.detach()) * m["lm_tokens"]
            if not torch.isfinite(loss):
                raise RuntimeError(f"non-finite loss at step {step + 1}")
            loss.backward()
        norm = float(torch.nn.utils.clip_grad_norm_(params, cfg["max_grad_norm"], error_if_nonfinite=True))
        optimizer.step()
        scheduler.step()
        step += 1
        last = {"step": step, "total": total, "head_loss": head_sum / n_row, "head_top1": hit / n_row,
                "lm_loss": (lm_sum / n_tok) if joint else None, "grad_norm": norm,
                "lr": scheduler.get_last_lr()[0], "seconds": round(time.monotonic() - start, 1)}
        history.append(last)
        if step == 1 or step % cfg.get("logging_steps", 10) == 0 or step == total:
            emit({"kind": "step", **last})
        if cfg.get("save_steps") and step % cfg["save_steps"] == 0 and step < total and cfg.get("snapshot"):
            cfg["snapshot"](step)
    if step == 0:
        raise RuntimeError("no optimizer step ran")
    if not any(not torch.equal(a, p.detach().cpu()) for a, p in zip(first, params)):
        raise RuntimeError("no trainable parameter changed")
    head.eval()
    model.eval()
    return {"steps": step, "total_steps": total, "rows_consumed": stream.consumed, "dropped_long": dropped,
            "first": history[0], "last": last, "trainable_adapter_parameters": sum(p.numel() for p in adapter),
            "head_parameters": sum(p.numel() for p in head.parameters())}


# --- writing the artifacts the server loads --------------------------------------------------

def write_outputs(out_dir, model, head, *, mode, base_sha256, corpus_manifest_sha256=None, train=None,
                  compiler_sha256=None):
    """`adapter/` (peft save_pretrained, joint only) and `head/` (save_head + manifest). Returns the paths and hashes."""
    from model.policy_head.head import save_head
    out_dir = Path(out_dir)
    adapter_dir, head_dir = out_dir / "adapter", out_dir / "head"
    adapter_sha = None
    if mode == "joint":
        model.save_pretrained(adapter_dir)
        adapter_sha = sha256_file(adapter_dir / "adapter_model.safetensors")
    save_head(head_dir, head)
    head_sha = sha256_file(head_dir / "head.pt")
    manifest = manifest_module.build(
        head_sha256=head_sha, hidden_size=head.norm.normalized_shape[0], base_sha256=base_sha256,
        adapter_sha256=adapter_sha, corpus_manifest_sha256=corpus_manifest_sha256, compiler_sha256=compiler_sha256,
        train=dict(train or {}, mode=mode))
    manifest_module.write(head_dir, manifest)
    return {"adapter_dir": str(adapter_dir) if adapter_sha else None, "head_dir": str(head_dir),
            "adapter_sha256": adapter_sha, "head_sha256": head_sha}


def reload_equivalence(fresh_host, adapter_dir, head_dir, trained_model, trained_head, packets, processor,
                       base_sha256=None, device="cpu"):
    """Serve what was written and compare with what was trained: the head's raw logits on each packet from the
    in-memory (adapter + head) against a fresh host + `PeftModel.from_pretrained(adapter_dir)` + `attach_head`
    (the HeadPredictor loader, manifest check included). Returns the largest absolute difference."""
    import torch
    from model.serve.head import HeadPredictor

    def predictor(model, attach=None, head=None):
        p = object.__new__(HeadPredictor)
        p.model, p.tokenizer, p.processor = model.eval(), processor.tokenizer, processor
        p.device, p._torch, p.prompt_kind = device, torch, KIND
        p.n_board_tokens = prompt_module.BOARD_TOKENS
        p.base_sha256 = base_sha256
        p.adapter_sha256 = sha256_file(Path(adapter_dir) / "adapter_model.safetensors") if adapter_dir else None
        if head is not None:
            p.head = head.eval()
        if attach is not None:
            p.attach_head(attach)
        return p

    trained = predictor(trained_model, head=trained_head)
    host = fresh_host
    if adapter_dir:
        import peft
        host = peft.PeftModel.from_pretrained(host, adapter_dir)
    served = predictor(host, attach=head_dir)
    worst = 0.0
    for packet in packets:
        a, b = trained.logits(packet), served.logits(packet)
        worst = max(worst, max(abs(x - y) for x, y in zip(a, b)))
    return worst
