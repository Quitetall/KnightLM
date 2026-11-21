"""Training entry point of the human-only move head (arm S1): CPU preflight, and the prepared `--gpu` run.

    python3 -m model.policy_head.train --corpus CORPUS --run NAME            # CPU preflight, prints the plan
    python3 -m model.policy_head.train --corpus CORPUS --run NAME --gpu      # the same, then trains

Joint training of the LoRA adapter and the head (`--mode joint`, default) or the cheap frozen-host probe
(`--mode head-only`), through `model.policy_head.joint`, which reuses the BLUT image-text trainer's data code,
LoRA configuration and scheduler (docs/model/move-head.md). Output under RUN_ROOT/NAME: `adapter/` and `head/`
(+ `manifest.json` binding base, adapter, ctok and rules revisions) for `model.serve.server --adapter-dir
--head-dir`. The `--gpu` run has NOT been executed in this repository; the preflight runs the same `execute`
function for two optimiser steps on CPU on the real base, then reloads what it wrote through the serve loader.

Preflight checks (each reported ok / skipped with the reason / refusal):
  rows            every sampled row is in the move space, its target legal, a reasoning row for joint mode
  corpus_audit    the corpus audit of `model.train.run` (when the corpus has a manifest)
  mask_legality   masks match the packet legal lists; a random head's masked argmax is always legal
  base_weights    the pinned base snapshot hash
  prompt_parity   head-read ids and conditioned ids equal the serve ids, the completion agrees with its target
  cpu_steps       two steps on CPU, outputs written, reloaded through the serve loader, same logits
  resources       free disk, GPU memory, RAM; the estimate (unmeasured)

Exit status: 0 plan ready (or run succeeded), 2 refused, 3 run failed.
"""

import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

from model.corpus.pipeline import MANIFEST, sha256_file
from model.policy_head import joint, space
from model.train.bases import BASES, DEFAULT_CORPUS, DEFAULT_RUN_ROOT, RECOMMENDED

BATCH_ROWS = 32  # rows per optimiser step of the legacy estimate (batch 8 x accumulation 4)
SAMPLE_ROWS = 64
CPU_STEP_ROWS = 8
# Unmeasured: Run 1 cost ~75 ms/row (625 steps x 32 rows in 25 min) including ~100+ generated tokens. A joint row
# is a head-read prompt pass plus a conditioned prompt + completion pass, so it is planned at the Run 1 row cost
# plus one more prompt forward: 75-110 ms/row. The head-only probe is a prompt-only no-grad pass: 8-15 ms/row.
# Re-measure on the first 200 steps before committing to the full pass.
MS_PER_ROW = {"joint": (75, 110), "head-only": (8, 15)}
VRAM_GB_ESTIMATE = 10  # unmeasured: bf16 host about 1 GB, two prompt-length passes at batch 8, no logits kept
MIN_FREE_DISK_GB = 5
MIN_FREE_VRAM_GB = 12


def shards_of(corpus):
    shards = sorted(Path(corpus).glob("train-*.jsonl"))
    if not shards:
        raise joint.Refused(f"no train shards in {corpus}")
    return shards


def preflight(corpus, rows, epochs, mode="head-only"):
    """Audit up to `rows` train rows against the move space and give the plan numbers. In joint mode a row also
    needs its reasoning (the completion). Rows that cannot train are counted, never skipped silently."""
    seen = refused = 0
    for shard in shards_of(corpus):
        with shard.open() as stream:
            for line in stream:
                if seen + refused >= rows:
                    break
                row = json.loads(line)
                try:
                    space.target_index(row["target"], row["packet"]["legal_moves"])
                    space.indices(row["packet"]["legal_moves"])
                    if mode == "joint" and "reasoning" not in row:
                        raise space.MoveSpaceError("no reasoning")
                    seen += 1
                except space.MoveSpaceError:
                    refused += 1
    total = seen * epochs
    return {"arm": "human-only move head", "corpus": str(corpus), "mode": mode, "rows_usable": seen,
            "rows_refused": refused, "epochs": epochs, "steps": total // BATCH_ROWS,
            "estimate_minutes": [round(total * ms / 60000, 1) for ms in MS_PER_ROW[mode]],
            "estimate_note": "unmeasured; see docs/model/training-gate.md"}


def sample_rows(corpus, n):
    rows = []
    for shard in shards_of(corpus):
        with shard.open() as stream:
            for line in stream:
                rows.append(json.loads(line))
                if len(rows) >= n:
                    return rows
    return rows


def check(status, **detail):
    return {"status": status, **detail}


def resources(out_root):
    """Disk, RAM and GPU facts, and the refusals `--gpu` makes from them. Nothing here measures speed."""
    root = Path(out_root)
    probe = root if root.exists() else next(p for p in root.parents if p.exists())
    free_gb = round(shutil.disk_usage(probe).free / 1e9, 1)
    info = {"out_dir_free_gb": free_gb, "root_free_gb": round(shutil.disk_usage("/").free / 1e9, 1), "gpu": None,
            "refusals": []}
    try:
        with open("/proc/meminfo") as stream:
            meminfo = {k: int(v.split()[0]) for k, v in (line.split(":", 1) for line in stream)}
        info["ram_available_gb"] = round(meminfo["MemAvailable"] / 1e6, 1)
    except (OSError, KeyError, ValueError):
        info["ram_available_gb"] = None
    try:
        out = subprocess.run(["nvidia-smi", "--query-gpu=name,memory.free,memory.total",
                              "--format=csv,noheader,nounits"], capture_output=True, text=True, timeout=20)
        if out.returncode == 0 and out.stdout.strip():
            name, free, total = [x.strip() for x in out.stdout.strip().splitlines()[0].split(",")]
            info["gpu"] = {"name": name, "free_gb": round(int(free) / 1024, 1), "total_gb": round(int(total) / 1024, 1)}
    except (OSError, subprocess.TimeoutExpired):
        pass
    info["vram_estimate_gb"] = VRAM_GB_ESTIMATE
    info["vram_estimate_note"] = "unmeasured"
    if free_gb < MIN_FREE_DISK_GB:
        info["refusals"].append(f"{free_gb} GB free under {probe}; need {MIN_FREE_DISK_GB}")
    if info["gpu"] is None:
        info["refusals"].append("no GPU answered nvidia-smi")
    elif info["gpu"]["free_gb"] < MIN_FREE_VRAM_GB:
        info["refusals"].append(f"{info['gpu']['free_gb']} GB GPU memory free; the plan assumes {MIN_FREE_VRAM_GB} "
                                "(the 4090 is shared: stop the other job or wait)")
    return info


def config_of(args):
    return {"mode": args.mode, "epochs": args.epochs, "batch_size": args.batch_size, "grad_accum": args.grad_accum,
            "seq_len": args.seq_len, "lr": args.lr, "head_lr_mult": args.head_lr_mult, "head_weight": args.head_weight,
            "rank": args.rank, "alpha": args.alpha, "seed": args.seed, "warmup_ratio": args.warmup_ratio,
            "lr_scheduler": args.lr_scheduler, "max_grad_norm": args.max_grad_norm, "save_steps": args.save_steps,
            "shuffle_buffer": args.shuffle_buffer, "max_steps": args.max_steps, "logging_steps": args.logging_steps}


def load_stack(args):
    """Backend data/train modules of the pinned BLUT trainer, the processor, and the checked base directory."""
    from model.train import preflight as stack
    from model.train.run import Refused, check_base
    profile = BASES[args.base]
    base_dir = Path(args.base_dir) if args.base_dir else Path(profile["dir"])
    try:
        base = check_base(profile, base_dir)
    except Refused as error:
        raise joint.Refused(str(error)) from error
    backend = stack.locate_backend_python()
    if backend is None:
        raise ImportError("hf_image_text_data.py not found (set BLUT_HF_PYTHON_DIR or build the blut-knightlm crate)")
    data, train_mod = stack.load_backend(backend)
    return data, train_mod, stack.load_processor(base_dir), base_dir, base, backend


def execute(args, shards, run_dir, device, dtype_name, max_steps=None, reload_atol=1e-3, emit=print):
    """Load the host, build adapter and head, run `joint.train_loop`, write the outputs, reload them through the
    serve loader and compare logits. The `--gpu` run is this function with device cuda and bfloat16; the CPU
    preflight is the same function with cpu, float32 and two steps."""
    import peft
    import torch
    from transformers import AutoModelForImageTextToText
    from model.policy_head.head import MoveHead
    data, train_mod, processor, base_dir, base, _ = load_stack(args)
    dtype = getattr(torch, dtype_name)

    def load_host():
        host = AutoModelForImageTextToText.from_pretrained(base_dir, local_files_only=True, dtype=dtype)
        return host.to(device)

    torch.manual_seed(args.seed)
    model, regex = load_host(), None
    if args.mode == "joint":
        regex = train_mod.resolve_lora_regex({"lora_target_regex": args.lora_target_regex}, model.config.model_type,
                                             [n for n, _ in model.named_modules()])
        model = peft.get_peft_model(model, peft.LoraConfig(r=args.rank, lora_alpha=args.alpha, target_modules=regex,
                                                           bias="none"))
    head = MoveHead(model.config.text_config.hidden_size)
    cfg = config_of(args)
    if max_steps:
        cfg["max_steps"] = max_steps
    manifest_sha = sha256_file(Path(args.corpus) / MANIFEST) if (Path(args.corpus) / MANIFEST).is_file() else None
    run_dir = Path(run_dir)
    meta = {"config": config_of(args), "device": device, "dtype": dtype_name}

    def snapshot(step):
        joint.write_outputs(run_dir / "snapshots" / f"step-{step:06d}", model, head, mode=args.mode,
                            base_sha256=base["weights_sha256"], corpus_manifest_sha256=manifest_sha,
                            train=dict(meta, step=step))
        emit({"kind": "saved", "step": step})
    cfg["snapshot"] = snapshot
    cfg["checkpoint_dir"] = run_dir / "resume"  # written every save_steps (model/train/resume.py)
    cfg["resume"] = bool(getattr(args, "resume", False))
    summary = joint.train_loop(model, head, processor, data, shards, cfg, emit, device=device)
    outputs = joint.write_outputs(run_dir, model, head, mode=args.mode, base_sha256=base["weights_sha256"],
                                  corpus_manifest_sha256=manifest_sha, train=dict(meta, steps=summary["steps"]))
    packets = [json.loads(line)["packet"] for line in Path(shards[0]).read_text().splitlines()[:3]]
    worst = joint.reload_equivalence(load_host(), outputs["adapter_dir"], outputs["head_dir"], model, head, packets,
                                     processor, base_sha256=base["weights_sha256"], device=device)
    if worst > reload_atol:
        raise RuntimeError(f"reloaded adapter and head differ from the trained ones: max logit error {worst}")
    summary.update(outputs=outputs, reload_max_logit_error=worst, device=device, dtype=dtype_name,
                   adapter_target_regex=regex)
    (run_dir / "train_summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    return summary


def plan_for(args):
    corpus = Path(args.corpus)
    corpus = corpus.parent if corpus.name == MANIFEST else corpus
    args.corpus = corpus
    run_dir = Path(args.run_root) / (args.run or "<run>")
    plan = {"entry_point": "model.policy_head.train", "arm": "human-only move head", "mode": args.mode,
            "run": args.run, "run_dir": str(run_dir), "checks": {}, "config": config_of(args)}
    if args.run and run_dir.exists() and not args.resume:
        raise joint.Refused(f"run directory {run_dir} exists; choose a new --run, or pass --resume to continue a "
                            "crashed run from its last checkpoint (a finished run is never extended)")
    if args.resume and not (run_dir / "resume" / "state.pt").is_file():
        raise joint.Refused(f"--resume needs a checkpoint at {run_dir / 'resume' / 'state.pt'}")
    rows = sample_rows(corpus, SAMPLE_ROWS)
    if not rows:
        raise joint.Refused(f"no train rows in {corpus}")
    counts = preflight(corpus, args.rows, args.epochs, args.mode)
    plan["checks"]["rows"] = check("ok", rows_usable=counts["rows_usable"], rows_refused=counts["rows_refused"])
    if counts["rows_refused"]:
        raise joint.Refused(f"{counts['rows_refused']} of the first {args.rows} rows cannot train the head (target "
                            "not legal, a move outside ctok/1, no legal moves"
                            + (", or no reasoning)" if args.mode == "joint" else ")"))
    if (corpus / MANIFEST).is_file():
        from model.train.run import Refused, check_corpus
        try:
            plan["checks"]["corpus_audit"] = check_corpus(corpus)[2]
        except Refused as error:
            raise joint.Refused(str(error)) from error
    else:
        plan["checks"]["corpus_audit"] = check("skipped", reason=f"no {MANIFEST} in {corpus} (fixture directory)")
    try:
        plan["checks"]["mask_legality"] = joint.check_mask_legality(rows)
    except ImportError as error:
        plan["checks"]["mask_legality"] = check("skipped", reason=f"torch unavailable: {error}")
    names = ("base_weights", "prompt_parity", "cpu_steps")
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
            plan["checks"]["prompt_parity"] = joint.check_serve_parity(rows, processor, data, args.mode == "joint")
            stub = joint.serve_stub(processor)
            lengths = [len(joint.read_ids(stub, r["packet"])) for r in rows]
            plan["checks"]["prompt_parity"]["head_read_tokens"] = {
                "mean": round(sum(lengths) / len(lengths), 1), "max": max(lengths), "seq_len": args.seq_len}
            if max(lengths) > args.seq_len:
                raise joint.Refused(f"a head-read prompt has {max(lengths)} tokens, over seq_len {args.seq_len}")
            if args.skip_cpu_steps:
                plan["checks"]["cpu_steps"] = check("skipped", reason="--skip-cpu-steps")
            else:
                scratch = Path(tempfile.mkdtemp(prefix="head-trainer-preflight-", dir=args.scratch))
                try:
                    shard = scratch / "train-00000.jsonl"
                    shard.write_text("".join(json.dumps(r) + "\n" for r in rows[:CPU_STEP_ROWS]))
                    cpu_args = argparse.Namespace(**{**vars(args), "batch_size": 2, "grad_accum": 1, "epochs": 1,
                                                     "shuffle_buffer": 1, "save_steps": 0, "logging_steps": 1,
                                                     "resume": False})
                    started = time.monotonic()
                    summary = execute(cpu_args, [shard], scratch / "run", "cpu", "float32", max_steps=2,
                                      reload_atol=1e-4, emit=lambda _e: None)
                    plan["checks"]["cpu_steps"] = check(
                        "ok", steps=summary["steps"], first=summary["first"], last=summary["last"],
                        reload_max_logit_error=summary["reload_max_logit_error"],
                        trainable_adapter_parameters=summary["trainable_adapter_parameters"],
                        head_parameters=summary["head_parameters"], seconds=round(time.monotonic() - started, 1),
                        note="the --gpu code path (execute) on CPU float32 with 2 steps; outputs reloaded through "
                             "the serve loader")
                finally:
                    shutil.rmtree(scratch, ignore_errors=True)
    per_step = args.batch_size * args.grad_accum
    plan["estimate"] = {"rows": args.rows, "epochs": args.epochs,
                        "optimizer_steps": args.max_steps or (args.rows * args.epochs) // per_step,
                        "minutes": counts["estimate_minutes"],
                        "note": "UNMEASURED: planned from Run 1 (75 ms/row); re-measure on the first 200 steps"}
    plan["resources"] = resources(args.run_root)
    plan["serve"] = (f"python -m model.serve.server --base-dir {BASES[args.base]['dir']} "
                     + (f"--adapter-dir {run_dir}/adapter " if args.mode == "joint" else "")
                     + f"--head-dir {run_dir}/head")
    plan["skipped_checks"] = [n for n, c in plan["checks"].items() if c["status"] == "skipped"]
    return plan


def launch(args, plan):
    """The prepared GPU run. Refuses unless every check ran and the GPU has room."""
    if plan["skipped_checks"]:
        raise joint.Refused(f"--gpu needs every preflight check to have run; skipped: {plan['skipped_checks']}")
    if plan["resources"]["refusals"]:
        raise joint.Refused("; ".join(plan["resources"]["refusals"]))
    import torch
    if not torch.cuda.is_available() or not torch.cuda.is_bf16_supported():
        raise joint.Refused("training needs CUDA with bfloat16 support")
    run_dir = Path(plan["run_dir"])
    run_dir.mkdir(parents=True, exist_ok=args.resume)
    entry = {"schema": "knightlm.head-train-entry/v1", "status": "resumed" if args.resume else "started",
             "started_utc": time.strftime("%Y-%m-%dT%H:%M:%S+00:00", time.gmtime()), "arm": "human-only move head",
             "config": plan["config"], "corpus": str(args.corpus), "base": args.base}
    entry_path = run_dir / "entry.json"
    entry_path.write_text(json.dumps(entry, indent=2) + "\n")
    try:
        with (run_dir / "train.log").open("a") as log:
            summary = execute(args, shards_of(args.corpus), run_dir, "cuda", "bfloat16",
                              emit=lambda e: (log.write(json.dumps(e) + "\n"), log.flush()))
    except Exception as error:  # recorded, then reported by main
        entry.update(status="failed", error=f"{type(error).__name__}: {error}")
        entry_path.write_text(json.dumps(entry, indent=2) + "\n")
        raise
    entry.update(status="succeeded", outputs=summary["outputs"],
                 reload_max_logit_error=summary["reload_max_logit_error"])
    entry_path.write_text(json.dumps(entry, indent=2) + "\n")
    return entry


def print_plan(plan):
    print(f"Move-head training plan ({plan['mode']}, run {plan['run']})")
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
    p.add_argument("--corpus", type=Path, default=DEFAULT_CORPUS)
    p.add_argument("--rows", type=int, default=20000, help="rows the plan counts and audits")
    p.add_argument("--epochs", type=int, default=d["epochs"])
    p.add_argument("--mode", choices=joint.MODES, default="joint")
    p.add_argument("--base", choices=sorted(BASES), default="smolvlm-500m")
    p.add_argument("--base-dir", type=Path)
    p.add_argument("--run", help="new run name (directory under --run-root); required to launch")
    p.add_argument("--run-root", type=Path, default=DEFAULT_RUN_ROOT)
    p.add_argument("--gpu", action="store_true", help="launch the training run (default: preflight only)")
    p.add_argument("--resume", action="store_true", help="continue the existing --run from its last checkpoint "
                   "(<run>/resume/state.pt, written every --save-steps steps); every setting must match the run's")
    p.add_argument("--batch-size", type=int, default=d["batch_size"])
    p.add_argument("--grad-accum", type=int, default=d["grad_accum"])
    p.add_argument("--seq-len", type=int, default=d["seq_len"])
    p.add_argument("--lr", type=float, default=d["lr"], help="adapter learning rate")
    p.add_argument("--head-lr-mult", type=float, default=10.0, help="head lr = lr x this (about 10x the adapter's)")
    p.add_argument("--head-weight", type=float, default=1.0, help="weight of the head loss beside the language loss")
    p.add_argument("--rank", type=int, default=d["rank"])
    p.add_argument("--alpha", type=int, default=d["alpha"])
    p.add_argument("--seed", type=int, default=d["seed"])
    p.add_argument("--lr-scheduler", default=d["lr_scheduler"])
    p.add_argument("--warmup-ratio", type=float, default=d["warmup_ratio"])
    p.add_argument("--max-grad-norm", type=float, default=d["max_grad_norm"])
    p.add_argument("--save-steps", type=int, default=d["save_steps"], help="snapshot adapter+head every N steps")
    p.add_argument("--shuffle-buffer", type=int, default=1000)
    p.add_argument("--logging-steps", type=int, default=10)
    p.add_argument("--max-steps", type=int, help="stop after N optimizer steps (smokes)")
    p.add_argument("--lora-target-regex")
    p.add_argument("--skip-model", action="store_true", help="skip every check that needs the base weights")
    p.add_argument("--skip-cpu-steps", action="store_true")
    p.add_argument("--scratch", type=Path, default=Path("/mnt/4tb/tmp"),
                   help="where the CPU preflight writes (removed after)")
    p.add_argument("--json", action="store_true")
    return p


def main(argv=None):
    args = build_parser().parse_args(argv)
    try:
        if args.gpu and not args.run:
            raise joint.Refused("--gpu needs --run NAME (a new run directory)")
        if args.gpu and (args.skip_model or args.skip_cpu_steps):
            raise joint.Refused("--gpu does not run with --skip-model or --skip-cpu-steps")
        plan = plan_for(args)
        if args.json:
            print(json.dumps(plan, indent=2))
        else:
            print_plan(plan)
        if not args.gpu:
            print("Nothing launched. Add --gpu (and --run NAME) to train.", file=sys.stderr)
            return 0
        entry = launch(args, plan)
        print(json.dumps({k: entry.get(k) for k in ("status", "outputs", "reload_max_logit_error")}))
        return 0
    except joint.Refused as error:
        print(f"REFUSED: {error}", file=sys.stderr)
        return 2
    except Exception as error:
        print(f"FAILED: {type(error).__name__}: {error}", file=sys.stderr)
        return 3


if __name__ == "__main__":
    sys.exit(main())
