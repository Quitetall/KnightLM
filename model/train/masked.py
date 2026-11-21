"""Adapter training with a loss mask: reasoning v2 variant b (`play-reads`) and any corpus whose rows carry `segments`.

    python3 -m model.train.masked --corpus CORPUS --run NAME                  # CPU preflight, prints the plan
    python3 -m model.train.masked --corpus CORPUS --run NAME --gpu            # the same, then trains

Why this exists: the BLUT recipe (`model.train.run`, `hf_sft_train`) reads `{prompt, completion}` only and puts loss
on every completion token, so the tool results of a variant b transcript would be trained on; `model.train.run` refuses
such a corpus (`--allow-unmasked-results` overrides). This entry trains the same LoRA (rank 8, alpha 16, the recipe's
regex, scheduler, clipping, token-weighted accumulation) with the loop of `model.encoder.task` in its text-only form:
`labels` are -100 over the prompt, the padding and every span a row's `segments` mark `loss: false`
(`model/train/segments.py`). Resume checkpoints every `--save-steps` steps (`--resume`).

Output under RUN_ROOT/NAME: `adapter/` (serve `--adapter-dir`), `manifest.json` (base, adapter, rows, corpus hashes, the
prompt kind and how many tokens the mask removed), `rows/`, `resume/`, `train_summary.json`, `entry.json`, `train.log`.
Checks (ok, skipped with the reason, or a refusal): corpus_audit, rows (a prompt kind of the corpus, a mask on every
variant b row), base_weights, prompt_parity (trainer prompt ids == the server's `_play_prompt_ids` for the kind; the
final call parses), cpu_steps (two optimiser steps on the real base on CPU, the adapter reloaded, same logits).
`--gpu` refuses on any skipped check and on too little disk or GPU memory. Exit status: 0 ready or done, 2 refused, 3 failed.
"""

import argparse
import json
from pathlib import Path
import shutil
import sys
import tempfile
import time

from model import prompt as prompt_module
from model.corpus.pipeline import MANIFEST, sha256_file
from model.encoder import rows as rows_module, task
from model.policy_head import joint
from model.train import segments as segments_module
from model.train.bases import BASES, DEFAULT_RUN_ROOT, RECOMMENDED

Refused = joint.Refused
SAMPLE_ROWS = 64
CPU_STEP_ROWS = 8
# Unmeasured: variant b rows average 1331 tokens (first 5,000 train rows, measured); over 26k..6.5k tokens/s.
MS_PER_ROW = (51, 205)
VRAM_GB_ESTIMATE = 15  # the Run 1 short run peaked at 14.5 GB (measured); variant b rows are about 25% longer
MIN_FREE_VRAM_GB = 16


def kinds_of(rows):
    return sorted({r.get("prompt_kind", "play") for r in rows})


def serve_stub(processor, kind):
    from model.serve.inference import Predictor
    stub = object.__new__(Predictor)
    stub.processor, stub.tokenizer = processor, processor.tokenizer
    stub.prompt_kind, stub.n_board_tokens = kind, prompt_module.BOARD_TOKENS
    return stub


def check_serve_parity(rows, processor, data):
    """Per row: the stored prompt is `model.prompt.play_prompt(kind, packet)`, the trainer's prompt ids equal the
    server's, the final call parses, and the loss mask leaves tokens to train on."""
    removed = trained = 0
    for row in rows:
        kind = row.get("prompt_kind", "play")
        if kind not in prompt_module.PLAY_KINDS or kind in prompt_module.ENC_KINDS or kind in prompt_module.HEAD_KINDS:
            raise Refused(f"row {row.get('id')}: prompt kind {kind!r} is not a text prompt this entry trains")
        if row["prompt"] != prompt_module.play_prompt(kind, row["packet"]) and "qa" not in row:
            raise Refused(f"row {row.get('id')}: the stored prompt differs from model.prompt ({kind})")
        served = serve_stub(processor, kind)._play_prompt_ids(row["packet"])[1] if "qa" not in row else None
        prompt_ids, ids, labels = segments_module.encode(row["prompt"], task.loss_segments(row), processor, data)
        if served is not None and prompt_ids != served:
            raise Refused(f"row {row.get('id')}: trainer prompt ids differ from the server's ({kind})")
        try:
            prompt_module.parse_play_completion(task.loss_segments(row)[-1]["text"])
        except prompt_module.PlayCompletionError as error:
            raise Refused(f"row {row.get('id')}: the final call does not parse ({error.code})") from error
        total = len(ids) - len(prompt_ids)
        kept = sum(1 for x in labels if x != segments_module.IGNORE)
        removed, trained = removed + total - kept, trained + kept
    return {"status": "ok", "rows": len(rows), "kinds": kinds_of(rows), "completion_tokens_trained": trained,
            "completion_tokens_masked": removed,
            "contract": "trainer prompt ids == Predictor._play_prompt_ids for the row's kind; labels -100 over results"}


def reload_equivalence(fresh_host, adapter_dir, trained_model, rows, processor, device="cpu"):
    """Largest absolute difference of the next-token logits at the end of each row's prompt, trained vs reloaded."""
    import peft
    import torch
    served = peft.PeftModel.from_pretrained(fresh_host, adapter_dir).eval()
    trained_model.eval()
    worst = 0.0
    for row in rows:
        text = processor.apply_chat_template(prompt_module.mode_chat(row["prompt"]), tokenize=False,
                                             add_generation_prompt=True)
        ids = torch.tensor([processor.tokenizer.encode(text, add_special_tokens=False)], device=device)
        with torch.no_grad():
            want = trained_model(input_ids=ids).logits[0, -1].float()
            got = served(input_ids=ids).logits[0, -1].float()
        worst = max(worst, float((want - got).abs().max()))
    return worst


def config_of(args):
    return {"mode": "joint", "epochs": args.epochs, "batch_size": args.batch_size, "grad_accum": args.grad_accum,
            "seq_len": args.seq_len, "lr": args.lr, "projector_lr_mult": 1.0, "rank": args.rank, "alpha": args.alpha,
            "seed": args.seed, "warmup_ratio": args.warmup_ratio, "lr_scheduler": args.lr_scheduler,
            "max_grad_norm": args.max_grad_norm, "save_steps": args.save_steps, "shuffle_buffer": args.shuffle_buffer,
            "max_steps": args.max_steps, "logging_steps": args.logging_steps}


def execute(args, rows_file, run_dir, device, dtype_name, max_steps=None, reload_atol=1e-3, emit=print,
            corpus_manifest_sha256=None):
    import peft
    import torch
    from transformers import AutoModelForImageTextToText
    from model.policy_head.train import load_stack
    data, train_mod, processor, base_dir, base, _ = load_stack(args)
    dtype = getattr(torch, dtype_name)

    def load_host():
        return AutoModelForImageTextToText.from_pretrained(base_dir, local_files_only=True, dtype=dtype).to(device)

    torch.manual_seed(args.seed)
    model = load_host()
    regex = train_mod.resolve_lora_regex({"lora_target_regex": args.lora_target_regex}, model.config.model_type,
                                         [n for n, _ in model.named_modules()])
    model = peft.get_peft_model(model, peft.LoraConfig(r=args.rank, lora_alpha=args.alpha, target_modules=regex,
                                                       bias="none"))
    cfg = config_of(args)
    if max_steps:
        cfg["max_steps"] = max_steps
    run_dir = Path(run_dir)
    meta = {"config": config_of(args), "device": device, "dtype": dtype_name}

    def snapshot(step):
        model.save_pretrained(run_dir / "snapshots" / f"step-{step:06d}")
        emit({"kind": "saved", "step": step})
    cfg.update(snapshot=snapshot, checkpoint_dir=run_dir / "resume", resume=bool(getattr(args, "resume", False)))
    summary = task.train_loop(model, None, None, processor, data, [Path(rows_file)], cfg, emit, device=device)
    model.save_pretrained(run_dir / "adapter")
    adapter_sha = sha256_file(run_dir / "adapter" / "adapter_model.safetensors")
    rows = [json.loads(line) for line in Path(rows_file).read_text().splitlines()]
    manifest = {"schema": "knightlm.masked-train/v1", "base_sha256": base["weights_sha256"], "adapter_sha256": adapter_sha,
                "rows_sha256": sha256_file(rows_file), "corpus_manifest_sha256": corpus_manifest_sha256,
                "prompt_kinds": kinds_of(rows), "train": dict(meta, steps=summary["steps"])}
    (run_dir / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    worst = reload_equivalence(load_host(), run_dir / "adapter", model, rows[:3], processor, device)
    if worst > reload_atol:
        raise RuntimeError(f"the reloaded adapter differs from the trained one: max logit error {worst}")
    summary.update(outputs={"adapter_dir": str(run_dir / "adapter"), "adapter_sha256": adapter_sha},
                   reload_max_logit_error=worst, device=device, dtype=dtype_name, adapter_target_regex=regex)
    (run_dir / "train_summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    return summary


def plan_for(args):
    from model.policy_head.train import load_stack, resources
    from model.train.run import Refused as RunRefused, check_corpus, corpus_path
    corpus = corpus_path(args.corpus)
    args.corpus = corpus
    run_dir = Path(args.run_root) / (args.run or "<run>")
    plan = {"entry_point": "model.train.masked", "run": args.run, "run_dir": str(run_dir), "checks": {},
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
    rows = train[:args.rows]
    masked = sum(1 for r in rows if r.get("segments") and segments_module.masked_spans(r["segments"]))
    plan["checks"]["rows"] = task.check("ok", rows=len(rows), kinds=kinds_of(rows), rows_with_a_mask=masked)
    if any(r.get("variant") == "b" for r in rows) and masked != len(rows):
        raise Refused("a variant b corpus must carry a loss mask on every row")
    names = ("base_weights", "prompt_parity", "cpu_steps")
    if args.skip_model:
        for name in names:
            plan["checks"][name] = task.check("skipped", reason="--skip-model")
    else:
        try:
            data, _, processor, _, base, _ = load_stack(args)
        except ImportError as error:
            for name in names:
                plan["checks"][name] = task.check("skipped", reason=f"training stack unavailable here: {error} "
                                                                    "(run with the image-text venv's python)")
        else:
            plan["checks"]["base_weights"] = base
            plan["checks"]["prompt_parity"] = check_serve_parity(rows[:SAMPLE_ROWS], processor, data)
            if args.skip_cpu_steps:
                plan["checks"]["cpu_steps"] = task.check("skipped", reason="--skip-cpu-steps")
            else:
                scratch = Path(tempfile.mkdtemp(prefix="masked-preflight-", dir=args.scratch))
                try:
                    shard = rows_module.write_jsonl(scratch / "train-00000.jsonl", rows[:CPU_STEP_ROWS])
                    cpu_args = argparse.Namespace(**{**vars(args), "batch_size": 2, "grad_accum": 1, "epochs": 1,
                                                     "shuffle_buffer": 1, "save_steps": 0, "logging_steps": 1,
                                                     "resume": False})
                    started = time.monotonic()
                    summary = execute(cpu_args, shard, scratch / "run", "cpu", "float32", max_steps=2,
                                      reload_atol=1e-4, emit=lambda _e: None)
                    plan["checks"]["cpu_steps"] = task.check(
                        "ok", steps=summary["steps"], first=summary["first"], last=summary["last"],
                        reload_max_logit_error=summary["reload_max_logit_error"],
                        trainable_adapter_parameters=summary["trainable_adapter_parameters"],
                        seconds=round(time.monotonic() - started, 1),
                        note="the --gpu code path (execute) on CPU float32 with 2 steps; adapter reloaded")
                finally:
                    shutil.rmtree(scratch, ignore_errors=True)
    per_step = args.batch_size * args.grad_accum
    plan["estimate"] = {"rows": len(rows), "epochs": args.epochs,
                        "optimizer_steps": args.max_steps or (len(rows) * args.epochs) // per_step,
                        "minutes": [round(len(rows) * args.epochs * ms / 60000, 1) for ms in MS_PER_ROW],
                        "note": "UNMEASURED: planned from Run 1 short (75 ms/row); re-measure on the first 200 steps"}
    res = resources(args.run_root)
    res["refusals"] = [r for r in res["refusals"] if "GPU memory" not in r] + (
        [f"{res['gpu']['free_gb']} GB GPU memory free; the plan assumes {MIN_FREE_VRAM_GB}"]
        if res["gpu"] and res["gpu"]["free_gb"] < MIN_FREE_VRAM_GB else [])
    res["vram_estimate_gb"] = VRAM_GB_ESTIMATE
    plan["resources"] = res
    plan["serve"] = (f"python -m model.eval.run --prompt-kind {kinds_of(rows)[0]} ... (scoring) ; the Python server "
                     "does not run read tools between turns (play-reads is a training and scoring kind)")
    plan["skipped_checks"] = [n for n, c in plan["checks"].items() if c["status"] == "skipped"]
    plan["_rows"] = rows
    return plan


def launch(args, plan):
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
    entry = {"schema": "knightlm.masked-train-entry/v1", "status": "resumed" if args.resume else "started",
             "started_utc": time.strftime("%Y-%m-%dT%H:%M:%S+00:00", time.gmtime()), "config": plan["config"],
             "corpus": str(args.corpus), "base": args.base, "rows_sha256": sha256_file(rows_file)}
    entry_path = run_dir / "entry.json"
    entry_path.write_text(json.dumps(entry, indent=2) + "\n")
    try:
        with (run_dir / "train.log").open("a") as log:
            summary = execute(args, rows_file, run_dir, "cuda", "bfloat16",
                              emit=lambda e: (log.write(json.dumps(e) + "\n"), log.flush()),
                              corpus_manifest_sha256=plan["checks"]["corpus_audit"]["manifest_sha256"])
    except Exception as error:
        entry.update(status="failed", error=f"{type(error).__name__}: {error}")
        entry_path.write_text(json.dumps(entry, indent=2) + "\n")
        raise
    entry.update(status="succeeded", outputs=summary["outputs"], reload_max_logit_error=summary["reload_max_logit_error"])
    entry_path.write_text(json.dumps(entry, indent=2) + "\n")
    return entry


def print_plan(plan):
    print(f"Masked adapter training plan (run {plan['run']})")
    for name, c in plan["checks"].items():
        print(f"  [{c['status']:7}] {name}" + (f" - {c['reason']}" if c.get("reason") else ""))
    e, r = plan["estimate"], plan["resources"]
    print(f"  {e['optimizer_steps']} optimizer steps, {e['minutes'][0]}-{e['minutes'][1]} min (UNMEASURED)")
    print(f"  disk free {r['out_dir_free_gb']} GB, GPU {r['gpu']}, RAM available {r['ram_available_gb']} GB, "
          f"VRAM estimate {r['vram_estimate_gb']} GB")
    for line in r["refusals"]:
        print(f"  --gpu would refuse: {line}")


def build_parser():
    d = RECOMMENDED
    p = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    p.add_argument("--corpus", type=Path, required=True, help="an audited corpus whose rows carry prompt, completion, segments")
    p.add_argument("--rows", type=int, default=20000)
    p.add_argument("--epochs", type=int, default=d["epochs"])
    p.add_argument("--base", choices=sorted(BASES), default="smolvlm-500m")
    p.add_argument("--base-dir", type=Path)
    p.add_argument("--run")
    p.add_argument("--run-root", type=Path, default=DEFAULT_RUN_ROOT)
    p.add_argument("--gpu", action="store_true")
    p.add_argument("--resume", action="store_true", help="continue --run from <run>/resume/state.pt")
    p.add_argument("--batch-size", type=int, default=d["batch_size"])
    p.add_argument("--grad-accum", type=int, default=d["grad_accum"])
    p.add_argument("--seq-len", type=int, default=d["seq_len"])
    p.add_argument("--lr", type=float, default=d["lr"])
    p.add_argument("--rank", type=int, default=d["rank"])
    p.add_argument("--alpha", type=int, default=d["alpha"])
    p.add_argument("--seed", type=int, default=d["seed"])
    p.add_argument("--lr-scheduler", default=d["lr_scheduler"])
    p.add_argument("--warmup-ratio", type=float, default=d["warmup_ratio"])
    p.add_argument("--max-grad-norm", type=float, default=d["max_grad_norm"])
    p.add_argument("--save-steps", type=int, default=d["save_steps"])
    p.add_argument("--shuffle-buffer", type=int, default=1000)
    p.add_argument("--logging-steps", type=int, default=10)
    p.add_argument("--max-steps", type=int)
    p.add_argument("--lora-target-regex")
    p.add_argument("--skip-model", action="store_true")
    p.add_argument("--skip-cpu-steps", action="store_true")
    p.add_argument("--scratch", type=Path, default=Path("/mnt/4tb/tmp"))
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
        print(json.dumps(public, indent=2)) if args.json else print_plan(public)
        if not args.gpu:
            print("Nothing launched. Add --gpu (and --run NAME) to train.", file=sys.stderr)
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
