"""Training entry point: corpus manifest -> verified plan -> (opt-in) BLUT LoRA run.

    python3 -m model.train.run --corpus CORPUS --run NAME            # CPU preflight, prints the plan
    python3 -m model.train.run --corpus CORPUS --run NAME --gpu      # the same, then launches

The default is a CPU preflight that changes nothing: it audits the corpus,
checks the pinned base weights, checks the training prompt against
`model.prompt`, tokenises a sample with the trainer's own data code, estimates
steps, time and VRAM, takes two optimiser steps on CPU, and compares the
configuration with the recommended defaults. It prints what would run. Only
`--gpu` launches the `blut-knightlm` recipe `knightlm_lora`, which re-audits and
materializes the corpus, trains, evaluates and registers the adapter. The run
directory must be new (a deliberate retry uses a new --run); `--resume`
continues a crashed run of the same name from its checkpoints.

This is the BLUT-based entry point for Run 1. `model/train/harness.py` stays as
the pilot/reference trainer (one epoch, in-memory, on a 1k-game corpus).

Exit status: 0 plan ready (or run succeeded), 2 refused by preflight, 3 launch failed.
"""

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import subprocess
import sys

from model.corpus.pipeline import MANIFEST, read_corpus, sha256_file
from model.prompt import user_chat
from model.train.bases import BASES, DEFAULT_CORPUS, DEFAULT_RUN_ROOT, RECOMMENDED

REPO = Path(__file__).resolve().parents[2]
CRATE = REPO / "apps" / "blut-knightlm"
PAYLOAD = CRATE / "python"
DEFAULT_VENV = Path("/mnt/4tb/venvs/blut-hf-image-text")
SAMPLE_ROWS = 64


class Refused(Exception):
    """Preflight found a reason not to start; nothing was launched."""


def utc_now():
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def check(status, **detail):
    return {"status": status, **detail}


def corpus_path(value):
    path = Path(value)
    return path.parent if path.name == MANIFEST else path


def check_corpus(corpus):
    """Full audit through the same reader the trainer's materializer relies on."""
    if not (corpus / MANIFEST).is_file():
        raise Refused(f"no {MANIFEST} in {corpus}")
    try:
        train, holdout = read_corpus(corpus)
    except (OSError, ValueError, KeyError) as error:
        raise Refused(f"corpus audit failed: {error}") from error
    manifest = json.loads((corpus / MANIFEST).read_text())
    return train, holdout, check(
        "ok", manifest_sha256=sha256_file(corpus / MANIFEST), format=manifest["format"],
        train_games=len(train), holdout_games=len(holdout), source=manifest["source"],
        shards=[f["file"] for f in manifest["shards"]["train"]["files"]])


def check_base(profile, base_dir):
    weights = base_dir / profile["weights_file"]
    if not weights.is_file():
        raise Refused(f"base weights not found: {weights}")
    actual = sha256_file(weights)
    if actual != profile["weights_sha256"]:
        raise Refused(f"base weights sha256 {actual} != pinned {profile['weights_sha256']} "
                      f"({profile['repository']}@{profile['revision']})")
    return check("ok", dir=str(base_dir), weights_sha256=actual,
                 repository=profile["repository"], revision=profile["revision"])


def check_prompt_parity(rows):
    """Training prompt text == the served prompt text, byte for byte."""
    sys.path.insert(0, str(PAYLOAD))
    from knightlm_blut import materialize
    prompt = materialize._prompt_module(REPO)
    for row in rows:
        trained = materialize.convert_row(row, prompt)["prompt"]
        served = user_chat(row["packet"])[0]["content"][0]["text"]
        if trained.encode() != served.encode():
            raise Refused(f"training prompt differs from model.prompt for game {row['game_id']}")
    return check("ok", rows=len(rows), source="model/prompt.py user_text == user_chat text")


def trainer_encode_matches_harness(rows, data, processor, harness_encode):
    """Token ids from the BLUT trainer's encode vs the reference harness encode."""
    sys.path.insert(0, str(PAYLOAD))
    from knightlm_blut import materialize
    prompt = materialize._prompt_module(REPO)
    examples = []
    for row in rows:
        record = materialize.convert_row(row, prompt)
        ours = data.encode(record["prompt"], record["completion"], processor)
        examples.append(ours)
        reference = harness_encode(row, processor)
        if ours[0] != reference[0] or ours[1] != reference[1]:
            raise Refused(f"trainer tokenisation differs from model/train/harness.py for game {row['game_id']}")
    return examples


def estimate(profile_key, profile, cfg, n_rows, lengths, measured_tps):
    per_step = cfg["batch_size"] * cfg["grad_accum"]
    steps = cfg["max_steps"] or cfg["epochs"] * (n_rows // per_step)
    out = {"rows": n_rows, "rows_per_step": per_step, "optimizer_steps": steps,
           "checkpoints": steps // cfg["save_steps"] if cfg["save_steps"] else 0}
    if lengths:
        mean = sum(lengths) / len(lengths)
        out.update(tokens_per_row_mean=round(mean, 1), tokens_per_row_max=max(lengths),
                   tokens_per_row_p95=sorted(lengths)[int(0.95 * (len(lengths) - 1))],
                   rows_over_seq_len=sum(n > cfg["seq_len"] for n in lengths),
                   total_tokens=int(steps * per_step * mean))
        tps = measured_tps or profile.get("tokens_per_second")
        if tps:
            out["tokens_per_second_assumed"] = tps
            out["hours"] = round(out["total_tokens"] / tps / 3600, 2)
        else:
            out["hours"] = None
            out["time_note"] = "no measured throughput for this base; pass --tokens-per-second"
    return out


def sanity(cfg, est, base_evaluable):
    """Deviations from the recommended defaults, and settings that look wrong."""
    notes = [f"{key}={cfg[key]} (recommended {want})" for key, want in RECOMMENDED.items()
             if cfg.get(key) != want]
    warnings = []
    if not 1e-5 <= cfg["lr"] <= 1e-3:
        warnings.append(f"lr {cfg['lr']} is outside the usual LoRA range 1e-5..1e-3")
    if cfg["rank"] and cfg["alpha"] / cfg["rank"] not in (1, 2, 4):
        warnings.append(f"alpha/rank = {cfg['alpha'] / cfg['rank']:g}; the pilot used 2")
    if cfg["batch_size"] * cfg["grad_accum"] < 8:
        warnings.append("effective batch below 8 is noisy at lr 2e-4")
    if cfg["epochs"] > 1:
        warnings.append("more than one epoch over a 200k-position corpus is untested")
    if est["optimizer_steps"] < 1:
        warnings.append("fewer rows than one optimizer step")
    if cfg["save_steps"] and cfg["save_steps"] >= est["optimizer_steps"]:
        warnings.append("save_steps is not below the total steps: only the final checkpoint is written")
    if est.get("rows_over_seq_len"):
        warnings.append(f"{est['rows_over_seq_len']} sampled rows exceed seq_len {cfg['seq_len']} and would be dropped")
    if not base_evaluable:
        warnings.append("evaluation is skipped for this base (the serving predictor pins SmolVLM)")
    return {"deviations_from_recommended": notes, "warnings": warnings}


def recipe_args(args, corpus, run):
    cfg = {"run": run, "run_root": str(args.run_root), "corpus_dir": str(corpus), "base": args.base,
           "epochs": args.epochs, "batch_size": args.batch_size, "grad_accum": args.grad_accum,
           "seq_len": args.seq_len, "lr": args.lr, "rank": args.rank, "alpha": args.alpha,
           "seed": args.seed, "save_steps": args.save_steps, "keep_checkpoints": args.keep_checkpoints,
           "lr_scheduler": args.lr_scheduler, "warmup_ratio": args.warmup_ratio,
           "max_grad_norm": args.max_grad_norm, "eval_rows": args.eval_rows, "evaluate": not args.no_eval}
    for key in ("base_dir", "lora_target_regex", "max_steps", "baseline", "eval_repo", "eval_out_root", "model_name",
                "alias", "stale_after_secs", "heartbeat_secs"):
        value = getattr(args, key, None)
        if value is not None:
            cfg[key] = str(value) if isinstance(value, Path) else value
    if args.eval_arg:
        cfg["eval_args"] = args.eval_arg
    return cfg


def find_binary(explicit):
    candidates = [explicit] if explicit else [os.environ.get("BLUT_KNIGHTLM_BIN"),
                                              CRATE / "target/release/blut-knightlm",
                                              CRATE / "target/debug/blut-knightlm"]
    for candidate in candidates:
        if candidate and Path(candidate).is_file():
            return Path(candidate)
    return None


def resolve_venv():
    env = os.environ.get("BLUT_HF_VENV_IMAGE_TEXT")
    if env:
        return Path(env), "BLUT_HF_VENV_IMAGE_TEXT"
    if DEFAULT_VENV.is_dir():
        return DEFAULT_VENV, "default"
    return None, None


def preflight(args):
    corpus = corpus_path(args.corpus)
    profile = BASES[args.base]
    base_dir = Path(args.base_dir) if args.base_dir else Path(profile["dir"])
    run = args.run
    cfg = {key: getattr(args, key) for key in
           ("epochs", "batch_size", "grad_accum", "seq_len", "lr", "rank", "alpha", "seed", "max_steps",
            "save_steps", "keep_checkpoints", "lr_scheduler", "warmup_ratio", "max_grad_norm")}
    run_dir = args.run_root / run
    plan = {"entry_point": "model.train.run", "utc": utc_now(), "run": run, "run_dir": str(run_dir),
            "recipe": "knightlm_lora", "checks": {}}
    if run_dir.exists() and not args.resume:
        raise Refused(f"run directory {run_dir} exists; choose a new --run (a finished run is never "
                      "extended) or pass --resume to continue a crashed run")
    if args.resume and not (run_dir / "state.json").is_file():
        raise Refused(f"--resume needs an existing run with {run_dir / 'state.json'}")
    train, holdout, plan["checks"]["corpus_audit"] = check_corpus(corpus)
    plan["corpus"] = {"dir": str(corpus), "manifest_sha256": plan["checks"]["corpus_audit"]["manifest_sha256"]}
    sample = train[:SAMPLE_ROWS]
    plan["checks"]["prompt_parity"] = check_prompt_parity(sample)

    lengths, examples = [], []
    if args.skip_model:
        plan["checks"]["base_weights"] = check("skipped", reason="--skip-model")
        for name in ("tokenisation", "trainer_vs_harness_tokens", "cpu_steps"):
            plan["checks"][name] = check("skipped", reason="--skip-model")
    else:
        plan["checks"]["base_weights"] = check_base(profile, base_dir)
        try:
            from model.train import preflight as stack
            backend = stack.locate_backend_python()
            if backend is None:
                raise ImportError("hf_image_text_data.py not found (set BLUT_HF_PYTHON_DIR or build the crate)")
            data, train_mod = stack.load_backend(backend)
            processor = stack.load_processor(base_dir)
            from model.train.harness import encode as harness_encode
        except ImportError as error:
            reason = f"training stack unavailable here: {error} (run with the image-text venv's python)"
            for name in ("tokenisation", "trainer_vs_harness_tokens", "cpu_steps"):
                plan["checks"][name] = check("skipped", reason=reason)
        else:
            plan["checks"]["tokenisation"] = check("ok", sample_rows=len(sample), backend_python=str(backend))
            if args.base == "smolvlm-500m":
                examples = trainer_encode_matches_harness(sample, data, processor, harness_encode)
                plan["checks"]["trainer_vs_harness_tokens"] = check(
                    "ok", rows=len(sample), note="BLUT trainer encode == model/train/harness.py encode")
            else:
                import sys as _sys
                _sys.path.insert(0, str(PAYLOAD))
                from knightlm_blut import materialize
                prompt = materialize._prompt_module(REPO)
                examples = [data.encode(r["prompt"], r["completion"], processor)
                            for r in (materialize.convert_row(x, prompt) for x in sample)]
                plan["checks"]["trainer_vs_harness_tokens"] = check(
                    "skipped", reason="the reference harness is SmolVLM-specific")
            lengths = [len(ids) for _, ids in examples]
            try:
                plan["checks"]["cpu_steps"] = check("ok", **stack.cpu_steps(
                    base_dir, args.lora_target_regex, examples, data, train_mod, args.rank, args.alpha,
                    args.lr, seq_len=args.seq_len)) if not args.skip_cpu_steps else check(
                    "skipped", reason="--skip-cpu-steps")
            except ValueError as error:
                raise Refused(f"CPU optimiser steps failed: {error}") from error
    plan["estimate"] = estimate(args.base, profile, cfg, len(train), lengths, args.tokens_per_second)
    plan["config"] = {**cfg, "base": args.base, "base_dir": str(base_dir)}
    plan["sanity"] = sanity(cfg, plan["estimate"], profile["evaluable"])
    plan["recipe_args"] = recipe_args(args, corpus, run)
    venv, source = resolve_venv()
    plan["environment"] = {"hf_venv_image_text": str(venv) if venv else None, "venv_source": source}
    binary = find_binary(args.blut_bin)
    argv = [str(binary or CRATE / "target/release/blut-knightlm"), "recipe", "run", "knightlm_lora",
            "--args", json.dumps(plan["recipe_args"], separators=(",", ":"))]
    plan["command"] = argv
    plan["binary_found"] = binary is not None
    skipped = [name for name, c in plan["checks"].items() if c["status"] == "skipped"]
    plan["ready"] = True
    plan["skipped_checks"] = skipped
    return plan


def print_plan(plan):
    print(f"KnightLM training plan (recipe {plan['recipe']}, run {plan['run']})")
    for name, c in plan["checks"].items():
        extra = c.get("reason", "")
        print(f"  [{c['status']:7}] {name}{(' - ' + extra) if extra else ''}")
    e = plan["estimate"]
    print(f"  rows {e['rows']}, {e['optimizer_steps']} optimizer steps of {e['rows_per_step']} rows, "
          f"{e['checkpoints']} checkpoints" + (f", ~{e['hours']} h at {e['tokens_per_second_assumed']} tok/s"
                                               if e.get("hours") else ""))
    for line in plan["sanity"]["deviations_from_recommended"]:
        print(f"  differs: {line}")
    for line in plan["sanity"]["warnings"]:
        print(f"  warning: {line}")
    print("  command:", " ".join(plan["command"][:4]), "--args '<json>'")
    print("Launching (--gpu)." if plan.get("launched") else "Nothing launched. Add --gpu to launch.")


def launch(args, plan):
    binary = find_binary(args.blut_bin)
    if binary is None:
        raise Refused("blut-knightlm binary not found; build it: "
                      "cd apps/blut-knightlm && oneoff cargo build --release")
    venv, _ = resolve_venv()
    if venv is None and not args.allow_venv_provision:
        raise Refused("no image-text venv: set BLUT_HF_VENV_IMAGE_TEXT, or pass --allow-venv-provision "
                      "to let BLUT build one under ~/.local/share/blut (root disk, several GB)")
    run_dir = Path(plan["run_dir"])
    run_dir.mkdir(parents=True, exist_ok=args.resume)
    env = dict(os.environ)
    if venv:
        env["BLUT_HF_VENV_IMAGE_TEXT"] = str(venv)
    entry = {"schema": "knightlm.train-entry/v1", "status": "started", "started_utc": utc_now(),
             "command": plan["command"], "corpus": plan["corpus"], "model": BASES[args.base] | {"key": args.base},
             "seed": args.seed, "config": plan["config"], "skipped_preflight_checks": plan["skipped_checks"]}
    entry_path = run_dir / "entry.json"
    entry_path.write_text(json.dumps(entry, indent=2) + "\n")
    with (run_dir / "blut.log").open("ab") as log:
        done = subprocess.run(plan["command"], env=env, stdout=log, stderr=subprocess.STDOUT)
    manifest = run_dir / "run-manifest.json"
    ok = done.returncode == 0 and manifest.is_file()
    entry.update(status="succeeded" if ok else "failed", finished_utc=utc_now(), returncode=done.returncode,
                 run_manifest=str(manifest) if manifest.is_file() else None, log=str(run_dir / "blut.log"))
    if ok:
        run = json.loads(manifest.read_text())
        entry.update(adapter=str(run_dir / "adapter"), adapter_sha256=run["adapter_sha256"],
                     corpus_manifest_sha256=run["corpus_manifest_sha256"])
    entry_path.write_text(json.dumps(entry, indent=2) + "\n")
    return entry


def build_parser():
    p = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    p.add_argument("--corpus", type=Path, default=DEFAULT_CORPUS, help="corpus directory or its manifest.json")
    p.add_argument("--base", choices=sorted(BASES), default="smolvlm-500m")
    p.add_argument("--base-dir", type=Path, help="override the pinned snapshot directory")
    p.add_argument("--run", required=True, help="new run name (directory under --run-root)")
    p.add_argument("--run-root", type=Path, default=DEFAULT_RUN_ROOT)
    p.add_argument("--resume", action="store_true", help="continue a crashed run of this name")
    p.add_argument("--gpu", action="store_true", help="launch the BLUT recipe (default: preflight only)")
    d = RECOMMENDED
    p.add_argument("--epochs", type=int, default=d["epochs"])
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
    p.add_argument("--keep-checkpoints", type=int, default=d["keep_checkpoints"])
    p.add_argument("--max-steps", type=int, help="stop after N optimizer steps (smokes)")
    p.add_argument("--lora-target-regex")
    p.add_argument("--eval-rows", type=int, default=1000)
    p.add_argument("--no-eval", action="store_true")
    p.add_argument("--baseline", type=Path, help="baseline scorecard directory for model.eval.compare")
    p.add_argument("--eval-repo", type=Path)
    p.add_argument("--eval-out-root", type=Path, help="where model.eval.run writes (default: its keep directory)")
    p.add_argument("--eval-arg", action="append", help="extra model.eval.run flag (repeatable)")
    p.add_argument("--model-name")
    p.add_argument("--alias")
    p.add_argument("--stale-after-secs", type=int)
    p.add_argument("--heartbeat-secs", type=int)
    p.add_argument("--tokens-per-second", type=float, help="throughput for the time estimate")
    p.add_argument("--skip-model", action="store_true",
                   help="skip every check that needs the base weights (fixture/CI use); reported as skipped")
    p.add_argument("--skip-cpu-steps", action="store_true")
    p.add_argument("--blut-bin", type=Path)
    p.add_argument("--allow-venv-provision", action="store_true")
    p.add_argument("--json", action="store_true", help="print the plan as JSON")
    return p


def main(argv=None):
    args = build_parser().parse_args(argv)
    try:
        plan = preflight(args)
        plan["launched"] = args.gpu
        if args.json:
            print(json.dumps(plan, indent=2))
        else:
            print_plan(plan)
        if not args.gpu:
            return 0
        entry = launch(args, plan)
        print(json.dumps({k: entry.get(k) for k in ("status", "returncode", "run_manifest", "adapter_sha256", "log")}))
        return 0 if entry["status"] == "succeeded" else 3
    except Refused as error:
        print(f"REFUSED: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
