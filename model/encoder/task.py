"""Task training of the board-encoder arms (T4 `play-enc`, T5 `play-enc-cgt`): the LoRA adapter and the projector
trained together, the encoder frozen. Prepared, not run on a GPU: docs/model/training-gate.md has the commands.

    python3 -m model.encoder.task --corpus CORPUS --kind play-enc --encoder ENC --run NAME            # CPU preflight
    python3 -m model.encoder.task --corpus CORPUS --kind play-enc --encoder ENC --run NAME --gpu      # then trains

One step: the rows' planes go through the frozen encoder and the trainable projector to 64 soft tokens, which the host
reads in its `<image>` slots (`model.encoder.inject.host_forward`, the idefics3 `image_hidden_states` path the server
decodes through); the language-model loss falls on the completion only, and on the spans a row's `segments` mark with
`loss: true` (`model/train/segments.py`). The loop is the one of `model.policy_head.joint` (row stream, scheduler,
token-weighted accumulation, clipping, resume checkpoints of `model/train/resume.py`) with the head replaced by the
projector. `--mode projector-only` keeps the host frozen (the alignment stage, with the chat-templated serve prompt).

Rows come from `model.encoder.rows` (S0 rules QA or a reasoning corpus converted to `{fen, repetition, prompt,
completion, packet}`); this entry converts the first `--rows` train rows of an audited corpus itself and records the
hash of what it converted. Output under RUN_ROOT/NAME:

  adapter/     adapter_model.safetensors + adapter_config.json          (serve --adapter-dir)
  projector/   projector.pt, projector.json, manifest.json              (serve --projector-dir)
  task.json    base, adapter, encoder and projector hashes, prompt kind (the server checks it against the adapter)
  rows/        the converted rows the run trained on
  resume/      state.pt, the last checkpoint (`--resume` continues from it)
  train_summary.json, entry.json, train.log

Preflight (each check ok, skipped with the reason, or a refusal): rows, corpus_audit, base_weights, prompt_parity
(trainer prompt ids == the server's, 64 slots per row, planes == the server's planes), cpu_steps (two optimiser steps
on the real base on CPU in float32, outputs written and reloaded through `Predictor._load_board`, same logits).
`--gpu` refuses on any skipped check, on a missing real `--encoder`, and on too little disk or GPU memory.
Exit status: 0 plan ready (or run succeeded), 2 refused, 3 run failed.
"""

import argparse
import json
import math
from pathlib import Path
import shutil
import sys
import tempfile
import time

from model import prompt as prompt_module
from model.corpus.pipeline import MANIFEST, sha256_file
from model.encoder import manifest as manifest_module, rows as rows_module
from model.encoder.planes import build_planes, packet_planes
from model.policy_head import joint
from model.train import resume as resume_module, segments as segments_module
from model.train.bases import BASES, DEFAULT_RUN_ROOT, RECOMMENDED

MODES = ("joint", "projector-only")
Refused = joint.Refused
SAMPLE_ROWS = 64
CPU_STEP_ROWS = 8
# Tokens per converted S0 row (trainer tokenisation, first 64 rows of s0-rules-qa-pilot-2026-11b-t0, measured): play-enc
# 940.6 mean (p95 1001, max 1017), play-enc-cgt 1177.6 (p95 1269, max 1293). The soft tokens replace the `<image>`
# embeddings, so they add no length. Time per row is tokens over the throughput range of the pinned profile (26k to 6.5k
# tokens/s, the planning figure being 14k): UNMEASURED for these arms.
MEAN_TOKENS = {"play-enc": 940.6, "play-enc-cgt": 1177.6}
MS_PER_ROW = {kind: (round(t / 26000 * 1000), round(t / 6500 * 1000)) for kind, t in MEAN_TOKENS.items()}
VRAM_GB_ESTIMATE = 8
MIN_FREE_DISK_GB = 5
MIN_FREE_VRAM_GB = 10


# --- rows and batches --------------------------------------------------------------------------------------

def loss_segments(row):
    return row.get("segments") or [{"text": row["completion"], "loss": True}]


def make_batch(rows, processor, data, image_id, n_tokens, seq_len, device, pad_id):
    """One micro-batch: ids, mask, labels (loss spans only), the planes, and the token count the loss is weighted by."""
    import torch
    examples = [segments_module.encode(r["prompt"], loss_segments(r), processor, data) for r in rows]
    cols = {k: torch.tensor(v, device=device) for k, v in segments_module.collate(examples, pad_id).items()}
    slots = (cols["input_ids"] == image_id).sum(dim=1).tolist()
    if any(n != n_tokens for n in slots):  # n_tokens 0: a text-only row must have no slot at all
        raise Refused(f"<image> slots per row {slots}, the encoder emits {n_tokens}")
    planes = torch.tensor([build_planes(r["fen"], r.get("repetition")) for r in rows], dtype=torch.float32,
                          device=device).reshape(len(rows), -1, 8, 8) if n_tokens else None
    return {"lm": cols, "planes": planes, "rows": len(rows), "lm_tokens": int(cols["labels"].ne(-100).sum()),
            "fits": max(len(i) for _, i, _ in examples) <= seq_len}


def check_serve_parity(rows, processor, data, kind, n_board_tokens=prompt_module.BOARD_TOKENS, binary=None):
    """For each row: the stored prompt is the model.prompt prompt of the arm (plus the question), the trainer's prompt
    ids equal the ids the server feeds the host (chat template and tokenizer of the serve path), the prompt carries
    exactly `n_board_tokens` `<image>` slots and no FEN, the planes equal the server's `packet_planes`, and the final
    call parses."""
    tokenizer = processor.tokenizer
    image_id = tokenizer.convert_tokens_to_ids(prompt_module.IMAGE_TOKEN)
    for row in rows:
        packet = row["packet"]
        want = prompt_module.play_prompt(kind, packet, binary=binary, n_board_tokens=n_board_tokens)
        if "qa" in row:
            want += prompt_module.QA_LABEL + row["qa"]["question"]
        if row["prompt"] != want:
            raise Refused(f"row {row.get('id')}: the stored prompt differs from model.prompt ({kind})")
        if packet["fen"] in row["prompt"]:
            raise Refused(f"row {row.get('id')}: the prompt spells the FEN; the encoder arms must not")
        text = processor.apply_chat_template(prompt_module.mode_chat(row["prompt"]), tokenize=False,
                                             add_generation_prompt=True)
        served = tokenizer.encode(text, add_special_tokens=False)
        prompt_ids, ids, _ = segments_module.encode(row["prompt"], loss_segments(row), processor, data)
        if prompt_ids != served:
            raise Refused(f"row {row.get('id')}: trainer prompt ids differ from the server's")
        if served.count(image_id) != n_board_tokens:
            raise Refused(f"row {row.get('id')}: {served.count(image_id)} <image> slots, expected {n_board_tokens}")
        if build_planes(row["fen"], row.get("repetition")) != packet_planes(packet):
            raise Refused(f"row {row.get('id')}: the row's planes differ from the server's for its packet")
        final = loss_segments(row)[-1]["text"]
        try:
            prompt_module.parse_play_completion(final)
        except prompt_module.PlayCompletionError as error:
            raise Refused(f"row {row.get('id')}: the completion's final call does not parse ({error.code})") from error
        if len(ids) <= len(prompt_ids):
            raise Refused(f"row {row.get('id')}: no completion tokens")
    return {"status": "ok", "rows": len(rows), "kind": kind, "board_tokens": n_board_tokens,
            "contract": "trainer prompt ids == chat-templated model.prompt prompt; image slots == encoder tokens; "
                        "planes == packet_planes; no FEN in the text"}


# --- training ----------------------------------------------------------------------------------------------

def train_loop(model, encoder, projector, processor, data, shards, cfg, emit, device="cpu"):
    """Adapter (joint) and projector optimisation; the encoder (and the base host) frozen. With `encoder` and
    `projector` None it is the text-only masked loop of `model.train.masked` (the adapter alone, no soft tokens).

    cfg keys: mode, epochs, batch_size, grad_accum, seq_len, lr, projector_lr_mult, warmup_ratio, lr_scheduler,
    max_grad_norm, seed, shuffle_buffer, max_steps, save_steps, logging_steps, snapshot (callable(step) or None),
    checkpoint_dir, resume. Returns a summary dict. The caller saves the final adapter and projector."""
    import torch
    from transformers import get_scheduler
    from model.encoder.inject import host_forward
    mode = cfg["mode"]
    if mode not in MODES:
        raise ValueError(f"mode must be one of {MODES}")
    train_adapter = mode == "joint"
    tokenizer = processor.tokenizer
    pad_id = tokenizer.pad_token_id if tokenizer.pad_token_id is not None else tokenizer.eos_token_id
    image_id = model.config.image_token_id
    text_only = encoder is None
    groups = []
    if not text_only:
        encoder.to(device).eval()
        for p in encoder.parameters():
            p.requires_grad_(False)
        projector.to(device).train()
        groups.append({"params": list(projector.parameters()), "lr": cfg["lr"] * cfg["projector_lr_mult"]})
    adapter = [p for p in model.parameters() if p.requires_grad] if train_adapter else []
    if train_adapter:
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
    total = joint.plan_steps(n_rows, cfg["epochs"], cfg["batch_size"], cfg["grad_accum"], cfg.get("max_steps"))
    if total < 1:
        raise Refused(f"{n_rows} rows x {cfg['epochs']} epochs is less than one step of {per_step} rows")
    scheduler = get_scheduler(cfg["lr_scheduler"], optimizer, num_warmup_steps=math.ceil(cfg["warmup_ratio"] * total),
                              num_training_steps=total)
    stream = data.RowStream(shards, cfg["seed"], cfg["shuffle_buffer"], cfg["epochs"])
    params = (list(projector.parameters()) if not text_only else []) + adapter
    n_tokens = 0 if text_only else encoder.cfg.n_tokens
    extra = {} if text_only else {"projector": projector}
    dropped, step, last, start = 0, 0, {}, time.monotonic()
    history = []
    ckpt_dir, fingerprint = cfg.get("checkpoint_dir"), resume_module.fingerprint(cfg, rows=n_rows, total=total)
    first_record = resumed_at = None
    if cfg.get("resume"):
        if not ckpt_dir:
            raise Refused("resume needs a checkpoint_dir")
        saved = resume_module.load(ckpt_dir)
        if saved is None:
            raise Refused(f"nothing to resume: no checkpoint in {ckpt_dir}")
        try:
            resume_module.check_config(saved, fingerprint)
            resume_module.restore_state(saved["weights"], model, **extra)
        except resume_module.ResumeRefused as error:
            raise Refused(str(error)) from error
        optimizer.load_state_dict(saved["optimizer"])
        scheduler.load_state_dict(saved["scheduler"])
        resume_module.fast_forward(stream, saved["stream_consumed"])
        step, dropped = saved["step"], saved["counters"]["dropped"]
        first_record, last = saved["counters"]["first"], saved["counters"]["last"]
        resume_module.set_rng_state(saved["rng"])
        resumed_at = step
        emit({"kind": "resumed", "step": step, "rows_consumed": stream.consumed})
    first = [p.detach().cpu().clone() for p in params]

    def next_micro():
        nonlocal dropped
        while True:
            rows = []
            while len(rows) < cfg["batch_size"]:
                line = stream.next_line()
                if line is None:
                    return None
                rows.append(json.loads(line))
            micro = make_batch(rows, processor, data, image_id, n_tokens, cfg["seq_len"], device, pad_id)
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
        lm_sum = 0.0
        for m in window:
            if text_only:
                lm_loss = model(**m["lm"], use_cache=False).loss
            else:
                with torch.no_grad():
                    tokens = encoder(m["planes"])
                lm_loss = host_forward(model, m["lm"]["input_ids"], projector(tokens),
                                       attention_mask=m["lm"]["attention_mask"], labels=m["lm"]["labels"],
                                       use_cache=False).loss
            loss = lm_loss * (m["lm_tokens"] / n_tok)
            if not torch.isfinite(loss):
                raise RuntimeError(f"non-finite loss at step {step + 1}")
            loss.backward()
            lm_sum += float(lm_loss.detach()) * m["lm_tokens"]
        norm = float(torch.nn.utils.clip_grad_norm_(params, cfg["max_grad_norm"], error_if_nonfinite=True))
        optimizer.step()
        scheduler.step()
        step += 1
        last = {"step": step, "total": total, "lm_loss": lm_sum / n_tok, "grad_norm": norm,
                "lr": scheduler.get_last_lr()[0], "rows": n_row, "seconds": round(time.monotonic() - start, 1)}
        history.append(last)
        first_record = first_record or last
        if step == 1 or step % cfg.get("logging_steps", 10) == 0 or step == total:
            emit({"kind": "step", **last})
        if cfg.get("save_steps") and step % cfg["save_steps"] == 0 and step < total:
            if cfg.get("snapshot"):
                cfg["snapshot"](step)
            if ckpt_dir:
                resume_module.save(ckpt_dir, step=step, weights=resume_module.trainable_state(model, **extra),
                                   optimizer=optimizer, scheduler=scheduler, stream_consumed=stream.consumed,
                                   counters={"dropped": dropped, "first": first_record, "last": last}, cfg=fingerprint)
    if step == 0:
        raise RuntimeError("no optimizer step ran")
    if step > (resumed_at or 0) and not any(not torch.equal(a, p.detach().cpu()) for a, p in zip(first, params)):
        raise RuntimeError("no trainable parameter changed")
    model.eval()
    if not text_only:
        projector.eval()
    return {"steps": step, "total_steps": total, "rows_consumed": stream.consumed, "dropped_long": dropped,
            "resumed_at_step": resumed_at, "first": first_record, "last": last,
            "trainable_adapter_parameters": sum(p.numel() for p in adapter),
            "projector_parameters": 0 if text_only else sum(p.numel() for p in projector.parameters())}


# --- the artifacts the server loads -----------------------------------------------------------------------------

def write_outputs(out_dir, model, projector, encoder_dir, *, mode, kind, base_sha256, n_board_tokens,
                  rows_sha256=None, corpus_manifest_sha256=None, compiler_sha256=None, train=None, final_loss=None,
                  steps=None):
    """`adapter/` (peft save_pretrained, joint only), `projector/` (the layout `_load_board` reads + its manifest) and
    `task.json` binding them. Returns the paths and hashes."""
    from model.encoder.align import write_projector_dir
    out_dir = Path(out_dir)
    adapter_dir, projector_dir = out_dir / "adapter", out_dir / "projector"
    adapter_sha = None
    if mode == "joint":
        model.save_pretrained(adapter_dir)
        adapter_sha = sha256_file(adapter_dir / "adapter_model.safetensors")
    write_projector_dir(projector_dir, projector.cpu(), encoder_dir, host_base_sha256=base_sha256,
                        final_loss=final_loss, steps=steps, rows_sha256=rows_sha256, train=dict(train or {}, mode=mode))
    projector_sha = sha256_file(projector_dir / "projector.pt")
    manifest_module.write_task_manifest(
        out_dir, base_sha256=base_sha256, adapter_sha256=adapter_sha,
        encoder_sha256=sha256_file(Path(encoder_dir) / "encoder.pt"), projector_sha256=projector_sha,
        prompt_kind=kind, n_board_tokens=n_board_tokens, rows_sha256=rows_sha256,
        corpus_manifest_sha256=corpus_manifest_sha256, compiler_sha256=compiler_sha256, train=dict(train or {}, mode=mode))
    return {"adapter_dir": str(adapter_dir) if adapter_sha else None, "projector_dir": str(projector_dir),
            "task_manifest": str(out_dir / manifest_module.TASK_FILE), "adapter_sha256": adapter_sha,
            "projector_sha256": projector_sha}


def reload_equivalence(fresh_host, run_dir, encoder_dir, trained_model, trained_encoder, trained_projector, rows,
                       processor, base_sha256, kind, device="cpu"):
    """Serve what was written and compare with what was trained: the host's next-token logits at the end of each row's
    prompt, from the in-memory (adapter + encoder + projector) against a fresh host + `PeftModel.from_pretrained` +
    `Predictor._load_board` (the loader the server runs, task and pair manifests checked). Largest absolute
    difference."""
    import torch
    from model.encoder.inject import host_forward
    from model.serve.inference import Predictor
    run_dir = Path(run_dir)
    adapter_dir = run_dir / "adapter" if (run_dir / "adapter").is_dir() else None
    host = fresh_host
    if adapter_dir:
        import peft
        host = peft.PeftModel.from_pretrained(host, adapter_dir)
    served = object.__new__(Predictor)
    served.model, served.processor, served.tokenizer = host.eval(), processor, processor.tokenizer
    served.device, served._torch, served.prompt_kind = device, torch, kind
    served.base_sha256 = base_sha256
    served.adapter_dir = adapter_dir
    served.adapter_sha256 = sha256_file(adapter_dir / "adapter_model.safetensors") if adapter_dir else None
    served.encoder_dir, served.projector_dir = Path(encoder_dir), run_dir / "projector"
    served._load_board()
    trained_model.eval()
    trained_encoder.eval()
    trained_projector.eval()
    worst = 0.0
    for row in rows:
        text = processor.apply_chat_template(prompt_module.mode_chat(row["prompt"]), tokenize=False,
                                             add_generation_prompt=True)
        ids = torch.tensor([processor.tokenizer.encode(text, add_special_tokens=False)], device=device)
        with torch.no_grad():
            soft_trained = trained_projector(trained_encoder(torch.tensor(
                [build_planes(row["fen"], row.get("repetition"))], dtype=torch.float32, device=device).reshape(1, -1, 8, 8)))
            want = host_forward(trained_model, ids, soft_trained).logits[0, -1].float()
            got = host_forward(served.model, ids, served._soft_tokens(row["packet"])).logits[0, -1].float()
        worst = max(worst, float((want - got).abs().max()))
    return worst


# --- entry point -------------------------------------------------------------------------------------------------

def check(status, **detail):
    return {"status": status, **detail}


def config_of(args):
    return {"mode": args.mode, "epochs": args.epochs, "batch_size": args.batch_size, "grad_accum": args.grad_accum,
            "seq_len": args.seq_len, "lr": args.lr, "projector_lr_mult": args.projector_lr_mult, "rank": args.rank,
            "alpha": args.alpha, "seed": args.seed, "warmup_ratio": args.warmup_ratio,
            "lr_scheduler": args.lr_scheduler, "max_grad_norm": args.max_grad_norm, "save_steps": args.save_steps,
            "shuffle_buffer": args.shuffle_buffer, "max_steps": args.max_steps, "logging_steps": args.logging_steps}


def load_encoder_dir(args, scratch):
    """(directory, label): the P0 encoder the run uses, or (preflight only) a random production-size one."""
    from model.encoder.encoder import BoardEncoder, EncoderConfig, save_encoder
    if args.encoder:
        if not (Path(args.encoder) / "encoder.pt").is_file():
            raise Refused(f"no encoder.pt in {args.encoder}")
        return Path(args.encoder), "pretrained"
    import torch
    torch.manual_seed(args.seed)
    save_encoder(Path(scratch) / "encoder", BoardEncoder(EncoderConfig()).eval())
    return Path(scratch) / "encoder", "random-init (P0 not run; preflight only)"


def execute(args, rows_file, run_dir, encoder_dir, device, dtype_name, max_steps=None, reload_atol=1e-3, emit=print,
            corpus_manifest_sha256=None, compiler_sha256=None):
    """Load the host, build adapter and projector, run `train_loop`, write the outputs, reload them through the serve
    loader and compare logits. `--gpu` is this function with cuda and bfloat16; the preflight is cpu, float32, 2 steps."""
    import peft
    import torch
    from transformers import AutoModelForImageTextToText
    from model.encoder.encoder import Projector, load_encoder, load_projector
    from model.policy_head.train import load_stack
    data, train_mod, processor, base_dir, base, _ = load_stack(args)
    dtype = getattr(torch, dtype_name)

    def load_host():
        return AutoModelForImageTextToText.from_pretrained(base_dir, local_files_only=True, dtype=dtype).to(device)

    torch.manual_seed(args.seed)
    model, regex = load_host(), None
    if args.mode == "joint":
        regex = train_mod.resolve_lora_regex({"lora_target_regex": args.lora_target_regex}, model.config.model_type,
                                             [n for n, _ in model.named_modules()])
        model = peft.get_peft_model(model, peft.LoraConfig(r=args.rank, lora_alpha=args.alpha, target_modules=regex,
                                                           bias="none"))
    encoder = load_encoder(encoder_dir)
    hidden = model.config.text_config.hidden_size
    projector = load_projector(args.projector) if args.projector else Projector(encoder.cfg.d_model, hidden)
    if projector.host_hidden != hidden or projector.d_encoder != encoder.cfg.d_model:
        raise Refused(f"projector maps {projector.d_encoder}->{projector.host_hidden}; encoder {encoder.cfg.d_model}, "
                      f"host {hidden}")
    cfg = config_of(args)
    if max_steps:
        cfg["max_steps"] = max_steps
    run_dir = Path(run_dir)
    meta = {"config": config_of(args), "device": device, "dtype": dtype_name}
    rows_sha = sha256_file(rows_file)
    kw = dict(mode=args.mode, kind=args.kind, base_sha256=base["weights_sha256"], n_board_tokens=encoder.cfg.n_tokens,
              rows_sha256=rows_sha, corpus_manifest_sha256=corpus_manifest_sha256, compiler_sha256=compiler_sha256)

    def snapshot(step):
        write_outputs(run_dir / "snapshots" / f"step-{step:06d}", model, projector, encoder_dir,
                      train=dict(meta, step=step), steps=step, **kw)
        projector.to(device)
        emit({"kind": "saved", "step": step})
    cfg.update(snapshot=snapshot, checkpoint_dir=run_dir / "resume", resume=bool(getattr(args, "resume", False)))
    summary = train_loop(model, encoder, projector, processor, data, [Path(rows_file)], cfg, emit, device=device)
    outputs = write_outputs(run_dir, model, projector, encoder_dir, train=dict(meta, steps=summary["steps"]),
                            final_loss=summary["last"]["lm_loss"], steps=summary["steps"], **kw)
    projector.to(device)
    sample = [json.loads(line) for line in Path(rows_file).read_text().splitlines()[:3]]
    worst = reload_equivalence(load_host(), run_dir, encoder_dir, model, encoder, projector, sample, processor,
                               base["weights_sha256"], args.kind, device=device)
    if worst > reload_atol:
        raise RuntimeError(f"reloaded adapter and projector differ from the trained ones: max logit error {worst}")
    summary.update(outputs=outputs, reload_max_logit_error=worst, device=device, dtype=dtype_name,
                   adapter_target_regex=regex)
    (run_dir / "train_summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    return summary


def plan_for(args):
    from model.train.run import Refused as RunRefused, check_corpus, corpus_path
    from model.policy_head.train import load_stack, resources
    corpus = corpus_path(args.corpus)
    args.corpus = corpus
    run_dir = Path(args.run_root) / (args.run or "<run>")
    plan = {"entry_point": "model.encoder.task", "arm": {"play-enc": "T4", "play-enc-cgt": "T5"}[args.kind],
            "kind": args.kind, "mode": args.mode, "run": args.run, "run_dir": str(run_dir), "checks": {},
            "config": config_of(args)}
    if args.run and run_dir.exists() and not args.resume:
        raise Refused(f"run directory {run_dir} exists; choose a new --run, or pass --resume to continue a crashed "
                      "run from its last checkpoint (a finished run is never extended)")
    if args.resume and not (run_dir / "resume" / "state.pt").is_file():
        raise Refused(f"--resume needs a checkpoint at {run_dir / 'resume' / 'state.pt'}")
    try:
        train, _, audit = check_corpus(corpus)
    except RunRefused as error:
        raise Refused(str(error)) from error
    plan["checks"]["corpus_audit"] = audit
    binary = None
    if args.kind == "play-enc-cgt":
        binary = prompt_module.knightlm_binary(args.knightlm)
        plan["compiler_sha256"] = sha256_file(binary)
    try:
        converted = rows_module.convert(train[:args.rows], args.kind, binary)
    except rows_module.RowError as error:
        raise Refused(str(error)) from error
    plan["checks"]["rows"] = check("ok", rows_converted=len(converted), rows_requested=args.rows, kind=args.kind,
                                   with_segments=sum(1 for r in converted if r.get("segments")))
    if len(converted) < args.rows:
        plan["checks"]["rows"]["note"] = f"the corpus has {len(train)} train rows"
    plan["_rows"], plan["_binary"] = converted, binary
    names = ("base_weights", "prompt_parity", "cpu_steps")
    scratch_root = Path(args.scratch)
    if args.skip_model:
        for name in names:
            plan["checks"][name] = check("skipped", reason="--skip-model")
    else:
        try:
            data, _, processor, _, base, _ = load_stack(args)
        except ImportError as error:
            for name in names:
                plan["checks"][name] = check("skipped", reason=f"training stack unavailable here: {error} "
                                                              "(run with the image-text venv's python)")
        else:
            plan["checks"]["base_weights"] = base
            sample = converted[:SAMPLE_ROWS]
            plan["checks"]["prompt_parity"] = check_serve_parity(sample, processor, data, args.kind, binary=binary)
            if args.skip_cpu_steps:
                plan["checks"]["cpu_steps"] = check("skipped", reason="--skip-cpu-steps")
            else:
                scratch = Path(tempfile.mkdtemp(prefix="board-task-preflight-", dir=scratch_root))
                try:
                    encoder_dir, label = load_encoder_dir(args, scratch)
                    shard = rows_module.write_jsonl(scratch / "train-00000.jsonl", converted[:CPU_STEP_ROWS])
                    cpu_args = argparse.Namespace(**{**vars(args), "batch_size": 2, "grad_accum": 1, "epochs": 1,
                                                     "shuffle_buffer": 1, "save_steps": 0, "logging_steps": 1,
                                                     "resume": False})
                    started = time.monotonic()
                    summary = execute(cpu_args, shard, scratch / "run", encoder_dir, "cpu", "float32", max_steps=2,
                                      reload_atol=1e-4, emit=lambda _e: None)
                    plan["checks"]["cpu_steps"] = check(
                        "ok", steps=summary["steps"], first=summary["first"], last=summary["last"],
                        reload_max_logit_error=summary["reload_max_logit_error"], encoder=label,
                        trainable_adapter_parameters=summary["trainable_adapter_parameters"],
                        projector_parameters=summary["projector_parameters"],
                        seconds=round(time.monotonic() - started, 1),
                        note="the --gpu code path (execute) on CPU float32 with 2 steps; adapter, projector and task "
                             "manifest reloaded through the serve loader (Predictor._load_board)")
                finally:
                    shutil.rmtree(scratch, ignore_errors=True)
    per_step = args.batch_size * args.grad_accum
    steps = args.max_steps or (args.rows * args.epochs) // per_step
    plan["estimate"] = {"rows": args.rows, "epochs": args.epochs, "optimizer_steps": steps,
                        "minutes": [round(args.rows * args.epochs * ms / 60000, 1) for ms in MS_PER_ROW[args.kind]],
                        "note": "UNMEASURED: mean tokens per row over 26k..6.5k tokens/s; re-measure on the first 200 steps"}
    plan["resources"] = resources(args.run_root)
    plan["resources"]["vram_estimate_gb"] = VRAM_GB_ESTIMATE
    plan["resources"]["refusals"] = [r for r in plan["resources"]["refusals"] if "GPU memory" not in r] + (
        [f"{plan['resources']['gpu']['free_gb']} GB GPU memory free; the plan assumes {MIN_FREE_VRAM_GB}"]
        if plan["resources"]["gpu"] and plan["resources"]["gpu"]["free_gb"] < MIN_FREE_VRAM_GB else [])
    if not args.encoder:
        plan["resources"]["refusals"].append("no --encoder: --gpu needs a pretrained P0 encoder directory")
    plan["serve"] = (f"python -m model.serve.server --base-dir {BASES[args.base]['dir']} "
                     + (f"--adapter-dir {run_dir}/adapter " if args.mode == "joint" else "")
                     + f"--encoder-dir {args.encoder or '<encoder>'} --projector-dir {run_dir}/projector "
                     f"--play-prompt-kind {args.kind}")
    plan["skipped_checks"] = [n for n, c in plan["checks"].items() if c["status"] == "skipped"]
    return plan


def launch(args, plan):
    """The prepared GPU run. Refuses unless every check ran, the encoder is real and the GPU has room."""
    if plan["skipped_checks"]:
        raise Refused(f"--gpu needs every preflight check to have run; skipped: {plan['skipped_checks']}")
    if plan["resources"]["refusals"]:
        raise Refused("; ".join(plan["resources"]["refusals"]))
    import torch
    if not torch.cuda.is_available() or not torch.cuda.is_bf16_supported():
        raise Refused("training needs CUDA with bfloat16 support")
    run_dir = Path(plan["run_dir"])
    run_dir.mkdir(parents=True, exist_ok=args.resume)
    rows_file = run_dir / "rows" / "train-00000.jsonl"
    if not args.resume:
        rows_module.write_jsonl(rows_file, plan["_rows"])
    entry = {"schema": "knightlm.board-task-entry/v1", "status": "resumed" if args.resume else "started",
             "started_utc": time.strftime("%Y-%m-%dT%H:%M:%S+00:00", time.gmtime()), "arm": plan["arm"],
             "kind": args.kind, "config": plan["config"], "corpus": str(args.corpus), "base": args.base,
             "encoder": str(args.encoder), "rows_sha256": sha256_file(rows_file)}
    entry_path = run_dir / "entry.json"
    entry_path.write_text(json.dumps(entry, indent=2) + "\n")
    manifest_sha = plan["checks"]["corpus_audit"]["manifest_sha256"]
    try:
        with (run_dir / "train.log").open("a") as log:
            summary = execute(args, rows_file, run_dir, args.encoder, "cuda", "bfloat16",
                              emit=lambda e: (log.write(json.dumps(e) + "\n"), log.flush()),
                              corpus_manifest_sha256=manifest_sha, compiler_sha256=plan.get("compiler_sha256"))
    except Exception as error:  # recorded, then reported by main
        entry.update(status="failed", error=f"{type(error).__name__}: {error}")
        entry_path.write_text(json.dumps(entry, indent=2) + "\n")
        raise
    entry.update(status="succeeded", outputs=summary["outputs"], reload_max_logit_error=summary["reload_max_logit_error"])
    entry_path.write_text(json.dumps(entry, indent=2) + "\n")
    return entry


def print_plan(plan):
    print(f"Board-encoder task training plan ({plan['arm']} {plan['kind']}, {plan['mode']}, run {plan['run']})")
    for name, c in plan["checks"].items():
        print(f"  [{c['status']:7}] {name}" + (f" - {c['reason']}" if c.get("reason") else ""))
    e, r = plan["estimate"], plan["resources"]
    print(f"  {e['optimizer_steps']} optimizer steps, {e['minutes'][0]}-{e['minutes'][1]} min (UNMEASURED)")
    print(f"  disk free {r['out_dir_free_gb']} GB, GPU {r['gpu']}, RAM available {r['ram_available_gb']} GB, "
          f"VRAM estimate {r['vram_estimate_gb']} GB (unmeasured)")
    for line in r["refusals"]:
        print(f"  --gpu would refuse: {line}")
    print("  serve:", plan["serve"])


def build_parser():
    d = RECOMMENDED
    p = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    p.add_argument("--corpus", type=Path, required=True, help="an audited corpus (S0 rules QA, or a reasoning corpus)")
    p.add_argument("--kind", choices=prompt_module.ENC_KINDS, default="play-enc", help="play-enc = T4, play-enc-cgt = T5")
    p.add_argument("--encoder", type=Path, help="the P0 encoder directory (model.encoder.pretrain's <out>/encoder)")
    p.add_argument("--projector", type=Path, help="start from this P1-aligned projector (default: a fresh one)")
    p.add_argument("--knightlm", type=Path, help="compiler binary (play-enc-cgt)")
    p.add_argument("--rows", type=int, default=20000, help="train rows converted and trained on")
    p.add_argument("--epochs", type=int, default=d["epochs"])
    p.add_argument("--mode", choices=MODES, default="joint")
    p.add_argument("--base", choices=sorted(BASES), default="smolvlm-500m")
    p.add_argument("--base-dir", type=Path)
    p.add_argument("--run", help="new run name (directory under --run-root); required to launch")
    p.add_argument("--run-root", type=Path, default=DEFAULT_RUN_ROOT)
    p.add_argument("--gpu", action="store_true", help="launch the training run (default: preflight only)")
    p.add_argument("--resume", action="store_true", help="continue the existing --run from its last checkpoint "
                   "(<run>/resume/state.pt, every --save-steps steps); every setting must match the run's")
    p.add_argument("--batch-size", type=int, default=d["batch_size"])
    p.add_argument("--grad-accum", type=int, default=d["grad_accum"])
    p.add_argument("--seq-len", type=int, default=d["seq_len"])
    p.add_argument("--lr", type=float, default=d["lr"], help="adapter learning rate")
    p.add_argument("--projector-lr-mult", type=float, default=5.0, help="projector lr = lr x this")
    p.add_argument("--rank", type=int, default=d["rank"])
    p.add_argument("--alpha", type=int, default=d["alpha"])
    p.add_argument("--seed", type=int, default=d["seed"])
    p.add_argument("--lr-scheduler", default=d["lr_scheduler"])
    p.add_argument("--warmup-ratio", type=float, default=d["warmup_ratio"])
    p.add_argument("--max-grad-norm", type=float, default=d["max_grad_norm"])
    p.add_argument("--save-steps", type=int, default=d["save_steps"], help="snapshot and resume checkpoint every N steps")
    p.add_argument("--shuffle-buffer", type=int, default=1000)
    p.add_argument("--logging-steps", type=int, default=10)
    p.add_argument("--max-steps", type=int, help="stop after N optimizer steps (smokes)")
    p.add_argument("--lora-target-regex")
    p.add_argument("--skip-model", action="store_true", help="skip every check that needs the base weights")
    p.add_argument("--skip-cpu-steps", action="store_true")
    p.add_argument("--scratch", type=Path, default=Path("/mnt/4tb/tmp"), help="where the CPU preflight writes (removed after)")
    p.add_argument("--json", action="store_true")
    return p


def main(argv=None):
    args = build_parser().parse_args(argv)
    try:
        if args.gpu and not args.run:
            raise Refused("--gpu needs --run NAME (a new run directory)")
        if args.gpu and (args.skip_model or args.skip_cpu_steps):
            raise Refused("--gpu does not run with --skip-model or --skip-cpu-steps")
        plan = plan_for(args)
        public = {k: v for k, v in plan.items() if not k.startswith("_")}
        if args.json:
            print(json.dumps(public, indent=2))
        else:
            print_plan(public)
        if not args.gpu:
            print("Nothing launched. Add --gpu (and --run NAME, --encoder DIR) to train.", file=sys.stderr)
            return 0
        entry = launch(args, plan)
        print(json.dumps({k: entry.get(k) for k in ("status", "outputs", "reload_max_logit_error")}))
        return 0
    except Refused as error:
        print(f"REFUSED: {error}", file=sys.stderr)
        return 2
    except Exception as error:
        print(f"FAILED: {type(error).__name__}: {error}", file=sys.stderr)
        return 3


if __name__ == "__main__":
    sys.exit(main())
