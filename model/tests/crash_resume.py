"""GPU check (opt-in, not part of unittest discovery): kill the recipe mid-training, rerun, compare.

    BLUT_HF_VENV_IMAGE_TEXT=... LAMU_TRAIN_JOBS_DIR=... \
        <image-text venv python> -m model.tests.crash_resume --work DIR --blut-bin BIN \
        [--steps 60] [--save-steps 6] [--kill-after 18]

1. Run A trains `--steps` optimizer steps uninterrupted; control run C repeats it.
2. Run B (same arguments, another run name) is SIGKILLed, process group and all, as
   soon as a checkpoint at or after step `--kill-after` exists.
3. Run B is started again with the same arguments. `claim_run` finds the dead run's
   marker (pid gone) and the trainer resumes from the newest checkpoint.
4. Compare B with A against the noise between the two uninterrupted runs A and C.

Two identical uninterrupted runs on this GPU are not bitwise equal (non-deterministic
CUDA kernels in the backward pass), so bitwise equality cannot be the criterion and
PR #3's own resume test uses a tolerance too. The check passes when the recipe-level
mechanics hold (marker left `running`, `claim_run` resumes, the trainer resumes at the
newest checkpoint, every later step runs exactly once, the run ends `finished`) and
the resumed run's loss deviation from A is no larger than twice the A-versus-C
deviation. The report carries all the numbers.
"""

import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

from model.corpus.pipeline import sha256_file
from model.run_pipeline import DEFAULT_SOURCE, ingest

CRATE = Path(__file__).resolve().parents[2] / "apps" / "blut-knightlm"


def recipe_args(args, snapshot, run):
    cfg = {"run": run, "run_root": str(args.work / "runs"), "corpus_dir": str(snapshot),
           "batch_size": 2, "grad_accum": 2, "max_steps": args.steps, "save_steps": args.save_steps,
           "logging_steps": 1, "evaluate": False, "heartbeat_secs": 1, "keep_checkpoints": 50}
    return json.dumps(cfg, separators=(",", ":"))


def start(args, snapshot, run, log):
    cmd = [str(args.blut_bin), "recipe", "run", "knightlm_lora", "--args", recipe_args(args, snapshot, run)]
    return subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT, start_new_session=True, env=os.environ)


def job_losses(jobs_dir, run):
    """{job_id: {step: loss}} for every job whose recipe args name `run`, oldest first."""
    found = {}
    for job in sorted(Path(jobs_dir).iterdir()):
        try:
            if json.loads((job / "args.json").read_text()).get("run") != run:
                continue
        except (OSError, ValueError):
            continue
        steps, resumed = {}, None
        for line in (job / "status.jsonl").read_text().splitlines():
            event = json.loads(line)
            update = event.get("update") or {}
            if update.get("kind") == "hf_step":
                steps[update["step"]] = update["loss"]
            elif update.get("kind") == "hf_resumed":
                resumed = update["step"]
        found[job.name] = {"losses": steps, "resumed_from_step": resumed}
    return found


def adapter_diff(a_dir, b_dir):
    """Largest absolute difference between two adapters' tensors."""
    from safetensors.torch import load_file
    a, b = (load_file(d / "adapter" / "adapter_model.safetensors") for d in (a_dir, b_dir))
    return max(float((a[k].float() - b[k].float()).abs().max()) for k in a)


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    p.add_argument("--work", type=Path, required=True)
    p.add_argument("--blut-bin", type=Path, default=CRATE / "target/debug/blut-knightlm")
    p.add_argument("--source", type=Path, default=DEFAULT_SOURCE)
    p.add_argument("--steps", type=int, default=60)
    p.add_argument("--save-steps", type=int, default=6)
    p.add_argument("--kill-after", type=int, default=18)
    p.add_argument("--timeout", type=int, default=900)
    args = p.parse_args(argv)
    args.work.mkdir(parents=True, exist_ok=True)
    jobs = Path(os.environ["LAMU_TRAIN_JOBS_DIR"])
    snapshot, _ = ingest(args.source, args.work, 240, 16)
    log = (args.work / "crash-resume.log").open("ab")

    a = start(args, snapshot, "ref-uninterrupted", log)
    if a.wait(args.timeout) != 0:
        raise SystemExit("reference run failed; see crash-resume.log")

    c = start(args, snapshot, "control-uninterrupted", log)
    if c.wait(args.timeout) != 0:
        raise SystemExit("control run failed; see crash-resume.log")

    b = start(args, snapshot, "killed-then-resumed", log)
    ckpts = args.work / "runs" / "killed-then-resumed" / "checkpoints"
    deadline = time.monotonic() + args.timeout
    while time.monotonic() < deadline and b.poll() is None:
        steps = [int(d.name.split("-")[1]) for d in ckpts.glob("step-*")] if ckpts.is_dir() else []
        if steps and max(steps) >= args.kill_after:
            break
        time.sleep(0.05)
    if b.poll() is not None:
        raise SystemExit("run B finished before it could be killed; raise --steps")
    killed_at = max(int(d.name.split("-")[1]) for d in ckpts.glob("step-*"))
    os.killpg(b.pid, signal.SIGKILL)
    b.wait()
    state = json.loads((args.work / "runs" / "killed-then-resumed" / "state.json").read_text())

    b2 = start(args, snapshot, "killed-then-resumed", log)
    if b2.wait(args.timeout) != 0:
        raise SystemExit("resumed run failed; see crash-resume.log")

    ref_dir, res_dir = args.work / "runs" / "ref-uninterrupted", args.work / "runs" / "killed-then-resumed"
    ctl_dir = args.work / "runs" / "control-uninterrupted"
    sha = lambda d: sha256_file(d / "adapter" / "adapter_model.safetensors")  # noqa: E731
    a_loss = list(job_losses(jobs, "ref-uninterrupted").values())[0]["losses"]
    c_loss = list(job_losses(jobs, "control-uninterrupted").values())[0]["losses"]
    resumed = list(job_losses(jobs, "killed-then-resumed").values())[-1]
    after = {s: l for s, l in resumed["losses"].items() if s > (resumed["resumed_from_step"] or 0)}
    rel = lambda x, y: abs(x - y) / max(abs(y), 1e-9)  # noqa: E731
    resume_dev = [rel(l, a_loss[s]) for s, l in after.items()]
    noise_dev = [rel(c_loss[s], a_loss[s]) for s in after]
    mean = lambda v: sum(v) / len(v) if v else float("nan")  # noqa: E731
    ledger = [json.loads(line) for line in (res_dir / "ledger.jsonl").read_text().splitlines()]
    report = {
        "steps": args.steps, "save_steps": args.save_steps, "newest_checkpoint_when_killed": killed_at,
        "state_after_kill": state["status"], "resumed_from_step": resumed["resumed_from_step"],
        "claim_decisions": [r["status"] for r in ledger if r["stage"] == "claim_run"],
        "steps_run_after_resume": sorted(after) == list(range(killed_at + 1, args.steps + 1)),
        "final_state": json.loads((res_dir / "state.json").read_text())["status"],
        "mean_rel_loss_dev_resumed_vs_uninterrupted": mean(resume_dev),
        "mean_rel_loss_dev_control_vs_uninterrupted": mean(noise_dev),
        "max_rel_loss_dev_resumed": max(resume_dev), "max_rel_loss_dev_control": max(noise_dev),
        "adapter_max_abs_diff_resumed_vs_uninterrupted": adapter_diff(ref_dir, res_dir),
        "adapter_max_abs_diff_control_vs_uninterrupted": adapter_diff(ref_dir, ctl_dir),
        "adapter_sha256": {"uninterrupted": sha(ref_dir), "control": sha(ctl_dir), "resumed": sha(res_dir)},
        "bitwise_identical_adapters": len({sha(ref_dir), sha(ctl_dir), sha(res_dir)}) == 1,
    }
    report["mechanics_ok"] = (report["state_after_kill"] == "running" and report["claim_decisions"] == ["fresh", "resume"]
                              and report["resumed_from_step"] == killed_at and report["steps_run_after_resume"]
                              and report["final_state"] == "finished")
    report["within_run_to_run_noise"] = (report["mean_rel_loss_dev_resumed_vs_uninterrupted"]
                                         <= 2 * report["mean_rel_loss_dev_control_vs_uninterrupted"] + 1e-6)
    (args.work / "crash-resume-report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["mechanics_ok"] and report["within_run_to_run_noise"] else 1


if __name__ == "__main__":
    sys.exit(main())
