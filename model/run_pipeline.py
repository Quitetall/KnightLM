"""One command from a source corpus to a trained, evaluated, registered adapter.

    python3 -m model.run_pipeline --source CORPUS --work DIR                 # plan only
    python3 -m model.run_pipeline --source CORPUS --work DIR --gpu --steps 30 # train too

Stages (the CPU ones always run; training only with --gpu):

1. ingest   cut a snapshot from the source corpus (the first N train and M holdout
            games, or more of them to add input), audit it, and publish it under
            DIR/snapshots/<manifest sha12>/ only if the audit passes. A snapshot
            is never modified; a rejected candidate is deleted and the previous
            snapshots stay as they were.
2. plan     `model.train.run` preflight on that exact snapshot (prints what would run).
3. train    with --gpu: `model.train.run --gpu` launches the BLUT recipe on the
            snapshot it just verified (materialize -> train -> evaluate -> register).
4. link     checks the snapshot manifest digest is the same in the snapshot, the
            entry manifest and the run manifest, and writes DIR/pipeline-report.json.

The source here is a corpus directory (the sharded v2 reader is the contract);
a real ETL step would produce the same kind of directory. `ingest()` is the seam.
"""

import argparse
import json
import os
from pathlib import Path
import shutil
import sys

from model.corpus.pipeline import MANIFEST, read_corpus, sha256_file
from model.tests.fixtures import slice_corpus
from model.train import run as entry

DEFAULT_SOURCE = Path("/mnt/4tb/artifacts/KnightLM/expire/corpus/run1-2026-09-a")


class Rejected(Exception):
    """The candidate snapshot failed the audit; nothing was published."""


def ingest(source, work, train_games, holdout_games, shard_games=64):
    """Return the verified snapshot directory for this slice of `source`.

    The candidate is built next to the snapshots and moved into place only after
    the audit passes. The snapshot name is its manifest digest, so an unchanged
    input maps to the same directory (nothing is rewritten) and added input maps
    to a new one (the previous snapshot is untouched)."""
    snapshots = Path(work) / "snapshots"
    snapshots.mkdir(parents=True, exist_ok=True)
    candidate = snapshots / f".candidate-{os.getpid()}"
    shutil.rmtree(candidate, ignore_errors=True)
    try:
        slice_corpus(source, candidate, train_games, holdout_games, shard_games)
        read_corpus(candidate)  # the audit: checksums, counts, leakage, time order
    except (OSError, ValueError, KeyError) as error:
        shutil.rmtree(candidate, ignore_errors=True)
        raise Rejected(f"candidate snapshot rejected: {error}") from error
    final = snapshots / sha256_file(candidate / MANIFEST)[:12]
    if final.exists():
        shutil.rmtree(candidate)
        return final, False
    candidate.rename(final)
    return final, True


def entry_argv(snapshot, work, args, gpu):
    argv = ["--corpus", str(snapshot), "--run", args.run, "--run-root", str(Path(work) / "runs"),
            "--base", args.base, "--batch-size", str(args.batch_size), "--grad-accum", str(args.grad_accum),
            "--max-steps", str(args.steps), "--save-steps", str(args.save_steps),
            "--eval-rows", str(args.eval_rows), "--eval-out-root", str(Path(work) / "eval")]
    for flag in ("base_dir", "blut_bin", "eval_repo"):
        if getattr(args, flag):
            argv += [f"--{flag.replace('_', '-')}", str(getattr(args, flag))]
    for extra in args.eval_arg or []:
        argv.append(f"--eval-arg={extra}")
    if args.skip_model:
        argv.append("--skip-model")
    if args.no_eval:
        argv.append("--no-eval")
    if gpu:
        argv.append("--gpu")
    return argv + (args.pass_through or [])


def link_report(snapshot, work, run):
    """The corpus digest must be the same in the snapshot and both run manifests."""
    run_dir = Path(work) / "runs" / run
    digest = sha256_file(snapshot / MANIFEST)
    found = {"snapshot": digest}
    for name, path, key in (("entry", run_dir / "entry.json", None), ("run", run_dir / "run-manifest.json", None)):
        if path.is_file():
            body = json.loads(path.read_text())
            found[name] = body["corpus_manifest_sha256"] if name == "run" else body["corpus"]["manifest_sha256"]
    return {"digests": found, "same": len(set(found.values())) == 1 and len(found) == 3,
            "entry_manifest": str(run_dir / "entry.json"), "run_manifest": str(run_dir / "run-manifest.json")}


def main(argv=None, launcher=None):
    p = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    p.add_argument("--source", type=Path, default=DEFAULT_SOURCE)
    p.add_argument("--work", type=Path, required=True)
    p.add_argument("--train-games", type=int, default=192)
    p.add_argument("--holdout-games", type=int, default=64)
    p.add_argument("--run", default="pipeline-smoke")
    p.add_argument("--base", default="smolvlm-500m")
    p.add_argument("--base-dir", type=Path)
    p.add_argument("--steps", type=int, default=30, help="optimizer steps of the bounded run (<= 30 for a smoke)")
    p.add_argument("--batch-size", type=int, default=2)
    p.add_argument("--grad-accum", type=int, default=2)
    p.add_argument("--save-steps", type=int, default=10)
    p.add_argument("--eval-rows", type=int, default=50)
    p.add_argument("--eval-repo", type=Path)
    p.add_argument("--eval-arg", action="append")
    p.add_argument("--no-eval", action="store_true")
    p.add_argument("--blut-bin", type=Path)
    p.add_argument("--skip-model", action="store_true")
    p.add_argument("--gpu", action="store_true", help="train (default: show the plan only)")
    p.add_argument("--pass-through", nargs=argparse.REMAINDER, help="remaining flags go to model.train.run")
    args = p.parse_args(argv)
    run_entry = launcher or entry.main

    try:
        snapshot, created = ingest(args.source, args.work, args.train_games, args.holdout_games)
    except Rejected as error:
        print(f"REJECTED: {error}\nprevious snapshots untouched; no training started", file=sys.stderr)
        return 2
    print(f"snapshot {snapshot} ({'new' if created else 'unchanged, reused'})")
    code = run_entry(entry_argv(snapshot, args.work, args, gpu=False))
    if code != 0 or not args.gpu:
        return code
    code = run_entry(entry_argv(snapshot, args.work, args, gpu=True))
    report = link_report(snapshot, args.work, args.run)
    report["train_exit_code"] = code
    (Path(args.work) / "pipeline-report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))
    return code if code else (0 if report["same"] else 4)


if __name__ == "__main__":
    sys.exit(main())
