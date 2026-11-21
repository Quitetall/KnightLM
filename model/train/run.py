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
from model.prompt import CTOK_KINDS, PLAY_KINDS, play_prompt, qa_prompt, user_chat
from model.train.bases import BASES, DEFAULT_CORPUS, DEFAULT_RUN_ROOT, RECOMMENDED

REPO = Path(__file__).resolve().parents[2]
CRATE = REPO / "apps" / "blut-knightlm"
PAYLOAD = CRATE / "python"
DEFAULT_VENV = Path("/mnt/4tb/venvs/blut-hf-image-text")
SAMPLE_ROWS = 64
# The recipe profile (apps/blut-knightlm/src/profile.rs) for the ctok/1-extended SmolVLM snapshot.
CTOK_BASE_KEY = "smolvlm-500m-ctok1"
CTOK_BASE_DIR = Path(BASES["smolvlm-500m"]["dir"]).parent / "smolvlm-500m-ctok1"
EXTENSION_FORMAT = "knightlm-ctok-extension/1"


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


def check_extended_base(profile, ext_dir):
    """The extended snapshot `ctok build-base` wrote: its record names this checkout's vocabulary and the pinned
    stock weights, and the extended weights hash to what it recorded (the recipe re-checks all of it)."""
    from model.train import ctok
    from model.tokenizer import vocab
    try:
        record = ctok.check_extension(ext_dir)
    except (OSError, ValueError, KeyError) as error:
        raise Refused(f"extended base {ext_dir}: {error}") from error
    if record.get("format") != EXTENSION_FORMAT or record.get("spec_version") != vocab.SPEC_VERSION:
        raise Refused(f"extended base {ext_dir}: not a {EXTENSION_FORMAT} record for {vocab.SPEC_VERSION}")
    if record.get("stock_weights_sha256") != profile["weights_sha256"]:
        raise Refused(f"extended base {ext_dir} was built from other weights than the pinned "
                      f"{profile['repository']}@{profile['revision']}")
    if not record.get("head_untied"):
        raise Refused(f"extended base {ext_dir} has a tied lm_head; the ctok1 recipe trains an untied head")
    weights = ext_dir / profile["weights_file"]
    actual = sha256_file(weights) if weights.is_file() else None
    if actual is None or actual != record.get("extended_weights_sha256") or actual == profile["weights_sha256"]:
        raise Refused(f"extended weights {weights}: sha256 {actual} does not match the extension record")
    return record, check("ok", dir=str(ext_dir), extended_weights_sha256=actual, stock_weights_sha256=profile["weights_sha256"],
                         vocab_sha256=record["vocab_sha256"], seed=record["seed"], base=record["base"])


def row_kind(row, kind):
    """The prompt kind a row is trained with: an S0 row carries its own, a reasoning row takes --prompt-kind."""
    return row["prompt_kind"] if "qa" in row else (kind if "reasoning" in row else "move")


def check_tokenizer_kinds(rows, kind, tokenizer):
    """ctok kinds need --tokenizer ctok1 and the other way round; a move-prompt row has no ctok form."""
    for row in rows:
        row_k = row_kind(row, kind)
        if (row_k in CTOK_KINDS) != (tokenizer == "ctok1"):
            raise Refused(f"prompt kind {row_k!r} with --tokenizer {tokenizer}: ctok arms (play-t1, play-t3, "
                          "play-t3-full) need --tokenizer ctok1, and --tokenizer ctok1 needs one of them")


def check_prompt_parity(rows, kind="play"):
    """Training prompt text == the served prompt text, byte for byte."""
    sys.path.insert(0, str(PAYLOAD))
    from knightlm_blut import materialize
    prompt = materialize._prompt_module(REPO)
    render = materialize._render_module(REPO)
    for row in rows:
        trained = materialize.convert_row(row, prompt, render, kind)["prompt"]
        # a reasoning row is trained and served with the play-mode prompt, a v2 row with the move prompt
        if "qa" in row:  # S0 rules QA: the arm's play prompt plus the question
            served = qa_prompt(row["prompt_kind"], row["packet"], row["qa"]["question"])
        else:
            served = (play_prompt(kind, row["packet"]) if "reasoning" in row
                      else user_chat(row["packet"])[0]["content"][0]["text"])
        if trained.encode() != served.encode():
            raise Refused(f"training prompt differs from model.prompt for game {row['game_id']}")
    return check("ok", rows=len(rows), source="model/prompt.py user_text == user_chat text")


def check_ctok_tokens(rows, kind, examples, processor, base):
    """T1/T3: the ctok ids in the trainer's prompt are exactly the id-level encoding of the position (offset by
    `base`), no ctok id appears in a completion, and the serve path (`Predictor._play_prompt_ids`: chat template,
    then encode) yields the trainer's prompt ids."""
    from model.prompt import cgt_text, mode_chat
    from model.tokenizer import arms

    tokenizer = processor.tokenizer
    for row, (prompt_ids, ids) in zip(rows, examples, strict=True):
        row_k = row_kind(row, kind)
        text = cgt_text(row["packet"], arms.CGT_VARIANT[row_k]) if row_k in arms.CGT_VARIANT else None
        want = [base + i for i in arms.position_ids(row_k, row["packet"], text)]
        found = [t for t in prompt_ids if t >= base]
        if found != want:
            raise Refused(f"ctok ids in the trainer prompt of {row['game_id']} differ from the id-level encoding "
                          f"({len(found)} vs {len(want)})")
        if any(t >= base for t in ids[len(prompt_ids):]):
            raise Refused(f"a ctok id appears in the completion of {row['game_id']} (arms T1/T3 change the input only)")
        question = row["qa"]["question"] if "qa" in row else None
        served_text = qa_prompt(row_k, row["packet"], question) if question else play_prompt(row_k, row["packet"])
        served = tokenizer.encode(processor.apply_chat_template(mode_chat(served_text), tokenize=False,
                                                                add_generation_prompt=True), add_special_tokens=False)
        if served != prompt_ids:
            raise Refused(f"serve tokenisation differs from the trainer's for {row['game_id']}")


def trainer_encode_matches_harness(rows, data, processor, harness_encode, kind="play"):
    """Token ids from the BLUT trainer's encode vs the reference harness encode."""
    sys.path.insert(0, str(PAYLOAD))
    from knightlm_blut import materialize
    prompt = materialize._prompt_module(REPO)
    render = materialize._render_module(REPO)
    examples = []
    for row in rows:
        record = materialize.convert_row(row, prompt, render, kind)
        ours = data.encode(record["prompt"], record["completion"], processor)
        examples.append(ours)
        if "reasoning" in row or "qa" in row:
            # harness.py encodes the move prompt only; check the structure of a reasoning pair instead:
            # the prompt ids are a prefix, loss covers the whole completion, and it ends on end-of-turn.
            tokenizer = processor.tokenizer
            tail = tokenizer.decode(ours[1][len(ours[0]):], skip_special_tokens=False)
            if not tail.lstrip().startswith(record["completion"]) or not tail.endswith("<end_of_utterance>") or ours[1][:len(ours[0])] != ours[0]:
                raise Refused(f"reasoning row for game {row['game_id']} does not encode as prompt + completion + end")
            continue
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
    if args.tokenizer == "ctok1":  # the recipe trains the extended profile; its new rows go through trainable_token_indices
        cfg["base"] = CTOK_BASE_KEY
        cfg["ctok_rows"] = args.ctok_rows
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
    ext_dir = None  # ctok1: the extended snapshot, once `ctok build-base` has written it
    if args.tokenizer == "ctok1":  # --base-dir is the extended snapshot, or (before it is built) a stock snapshot
        if args.base_dir and (Path(args.base_dir) / "ctok-extension.json").is_file():
            ext_dir, base_dir = Path(args.base_dir), Path(profile["dir"])
        else:
            ext_dir = CTOK_BASE_DIR
    extended = ext_dir is not None and (ext_dir / "ctok-extension.json").is_file()
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
    check_tokenizer_kinds(sample, args.prompt_kind, args.tokenizer)
    if args.tokenizer == "ctok1":
        if args.base != "smolvlm-500m":
            raise Refused("--tokenizer ctok1 is built and tested for the SmolVLM base only")
        from model.tokenizer import vocab
        plan["tokenizer"] = {"name": "ctok1", "spec_version": vocab.SPEC_VERSION, "vocab_sha256": vocab.vocab_sha256(),
                             "vocab_size": vocab.SIZE, "trainable_rows": args.ctok_rows,
                             "embedding_path": "peft trainable_token_indices (model/train/ctok.py)",
                             "recipe_base": CTOK_BASE_KEY, "extended_base_dir": str(ext_dir), "extended_base_built": extended}
    plan["checks"]["prompt_parity"] = check_prompt_parity(sample, args.prompt_kind)

    lengths, examples = [], []
    if args.skip_model:
        plan["checks"]["base_weights"] = check("skipped", reason="--skip-model")
        for name in ("tokenisation", "trainer_vs_harness_tokens", "cpu_steps"):
            plan["checks"][name] = check("skipped", reason="--skip-model")
        if extended:
            plan["checks"]["extended_base"] = check("skipped", reason="--skip-model")
    else:
        plan["checks"]["base_weights"] = check_base(profile, base_dir)
        ext_record = None
        if extended:
            ext_record, plan["checks"]["extended_base"] = check_extended_base(profile, ext_dir)
        try:
            from model.train import preflight as stack
            backend = stack.locate_backend_python()
            if backend is None:
                raise ImportError("hf_image_text_data.py not found (set BLUT_HF_PYTHON_DIR or build the crate)")
            data, train_mod = stack.load_backend(backend)
            processor = stack.load_processor(ext_dir if extended else base_dir)
            from model.train.harness import encode as harness_encode
        except ImportError as error:
            reason = f"training stack unavailable here: {error} (run with the image-text venv's python)"
            for name in ("tokenisation", "trainer_vs_harness_tokens", "cpu_steps"):
                plan["checks"][name] = check("skipped", reason=reason)
        else:
            ctok_base = None
            if args.tokenizer == "ctok1":
                from model.train import ctok
                from model.tokenizer import vocab as ctok_vocab
                if extended:  # the snapshot's tokenizer is already extended: it must be exactly ours
                    ctok_base = ext_record["base"]
                    if len(processor.tokenizer) != ctok_base + ctok_vocab.SIZE:
                        raise Refused(f"extended base tokenizer has {len(processor.tokenizer)} entries, "
                                      f"expected {ctok_base + ctok_vocab.SIZE}")
                else:
                    ctok_base = ctok.extend_processor(processor)
            plan["checks"]["tokenisation"] = check("ok", sample_rows=len(sample), backend_python=str(backend),
                                                   **({"ctok_base": ctok_base, "tokenizer_length": len(processor.tokenizer)}
                                                      if ctok_base is not None else {}))
            if args.base == "smolvlm-500m":
                examples = trainer_encode_matches_harness(sample, data, processor, harness_encode, args.prompt_kind)
                if ctok_base is not None:
                    check_ctok_tokens(sample, args.prompt_kind, examples, processor, ctok_base)
                reasoning_rows = "reasoning" in sample[0] or "qa" in sample[0]
                plan["checks"]["trainer_vs_harness_tokens"] = check(
                    "ok", rows=len(sample), **({"ctok": "position ids == id-level encoding at base+id; no ctok id in completions; "
                                                "serve chat-template tokenisation == trainer prompt ids"} if ctok_base is not None else {}),
                    note=("reasoning rows: prompt prefix, completion and end-of-turn checked "
                                                  "(harness.py encodes the move prompt only)" if reasoning_rows
                                                  else "BLUT trainer encode == model/train/harness.py encode"))
            else:
                import sys as _sys
                _sys.path.insert(0, str(PAYLOAD))
                from knightlm_blut import materialize
                prompt = materialize._prompt_module(REPO)
                render = materialize._render_module(REPO)
                examples = [data.encode(r["prompt"], r["completion"], processor)
                            for r in (materialize.convert_row(x, prompt, render) for x in sample)]
                plan["checks"]["trainer_vs_harness_tokens"] = check(
                    "skipped", reason="the reference harness is SmolVLM-specific")
            lengths = [len(ids) for _, ids in examples]
            try:
                plan["checks"]["cpu_steps"] = check("ok", **stack.cpu_steps(
                    ext_dir if extended else base_dir, args.lora_target_regex, examples, data, train_mod, args.rank, args.alpha,
                    args.lr, seq_len=args.seq_len,
                    extension=(processor.tokenizer, ctok_base, args.ctok_rows) if ctok_base is not None else None,
                    already_extended=extended)) if not args.skip_cpu_steps else check(
                    "skipped", reason="--skip-cpu-steps")
            except ValueError as error:
                raise Refused(f"CPU optimiser steps failed: {error}") from error
    plan["estimate"] = estimate(args.base, profile, cfg, len(train), lengths, args.tokens_per_second)
    plan["config"] = {**cfg, "base": args.base, "base_dir": str(base_dir)}
    plan["sanity"] = sanity(cfg, plan["estimate"], profile["evaluable"] and args.tokenizer != "ctok1")
    plan["recipe_args"] = recipe_args(args, corpus, run)
    if args.tokenizer == "ctok1":
        plan["recipe_args"]["base_dir"] = str(ext_dir)
    venv, source = resolve_venv()
    plan["environment"] = {"hf_venv_image_text": str(venv) if venv else None, "venv_source": source}
    binary = find_binary(args.blut_bin)
    argv = [str(binary or CRATE / "target/release/blut-knightlm"), "recipe", "run", "knightlm_lora",
            "--args", json.dumps(plan["recipe_args"], separators=(",", ":"))]
    plan["command"] = argv
    if args.tokenizer == "ctok1" and not extended:
        plan["launch_blocked"] = (
            f"no extended base at {ext_dir} (python -m model.train.ctok build-base --base-dir {profile['dir']} --out {ext_dir}); "
            "the recipe trains the new rows only on that snapshot")
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
    if plan.get("launch_blocked"):
        print("  --gpu would be refused:", plan["launch_blocked"])
    print("Launching (--gpu)." if plan.get("launched") else "Nothing launched. Add --gpu to launch.")


def launch(args, plan):
    if plan.get("launch_blocked"):
        raise Refused(plan["launch_blocked"])
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
    p.add_argument("--tokenizer", choices=("stock", "ctok1"), default="stock",
                   help="ctok1: arms T1/T3, the host tokenizer extended by the ctok/1 vocabulary (--gpu needs the extended "
                        "base from `python -m model.train.ctok build-base`; --base-dir points at it)")
    p.add_argument("--prompt-kind", choices=PLAY_KINDS, default="play",
                   help="play prompt kind of a reasoning corpus (S0 corpora carry their own)")
    p.add_argument("--ctok-rows", choices=("embed", "embed+head"), default="embed+head",
                   help="ctok1: new rows trained through trainable_token_indices (input only, or input and output head)")
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
