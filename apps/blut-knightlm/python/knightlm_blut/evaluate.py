"""Run the external evaluation harness on a produced adapter.

    python -m knightlm_blut.evaluate --base-dir B --adapter A --corpus C --label L --rows N
        [--compare BASELINE_SCORECARD_DIR] [--eval-repo DIR] [-- EXTRA model.eval.run ARGS]

Invokes `python -m model.eval.run` (PR #18 of KnightLM) as a subprocess and
prints one JSON line: {"status": "ok", "scorecard_dir": ..., "scorecard": ...}.
When the harness is absent the line is {"status": "skipped", "reason": ...} and
the exit code is 0, so a checkout without the harness still trains and
registers; a harness that is present but fails is an error. `--compare` runs
`python -m model.eval.compare BASELINE NEW` and keeps its per-axis output (no
composite score) in compare.txt beside the new scorecard.
"""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

from knightlm_blut.common import repo_root, sha256_file


def harness_present(eval_repo):
    return (eval_repo / "model" / "eval" / "run.py").is_file()


def _env(eval_repo, base_dir, adapter, payload):
    env = dict(os.environ)
    parts = [str(eval_repo), str(payload)]
    if env.get("PYTHONPATH"):
        parts.append(env["PYTHONPATH"])
    env["PYTHONPATH"] = os.pathsep.join(parts)
    env["KNIGHTLM_BASE_DIR"] = str(base_dir)
    env["KNIGHTLM_ADAPTER_DIR"] = str(adapter) if adapter else ""
    return env


def evaluate(args, extra):
    eval_repo = (args.eval_repo or repo_root()).resolve()
    if not harness_present(eval_repo):
        return {"status": "skipped",
                "reason": f"model.eval.run not found under {eval_repo} (KnightLM PR #18 not merged)"}
    payload = Path(__file__).resolve().parents[1]
    env = _env(eval_repo, args.base_dir, args.adapter, payload)
    command = [sys.executable, "-m", "model.eval.run", "--predictor", "knightlm_blut.predictor:factory",
               "--label", args.label, "--rows", str(args.rows), "--corpus", str(args.corpus), *extra]
    if args.out_root:
        command += ["--out-root", str(args.out_root)]
    done = subprocess.run(command, cwd=eval_repo, env=env, capture_output=True, text=True)
    if done.returncode != 0:
        raise RuntimeError(f"model.eval.run failed ({done.returncode}): {done.stderr.strip()[-2000:]}")
    lines = [line for line in done.stdout.splitlines() if line.strip()]
    scorecard_dir = Path(lines[-1].strip())
    scorecard = scorecard_dir / "scorecard.json"
    if not scorecard.is_file():
        raise RuntimeError(f"model.eval.run did not write {scorecard}")
    result = {"status": "ok", "scorecard_dir": str(scorecard_dir), "scorecard": str(scorecard),
              "harness_sha256": sha256_file(eval_repo / "model" / "eval" / "run.py"),
              "harness_repo": str(eval_repo)}
    if args.compare:
        cmp = subprocess.run([sys.executable, "-m", "model.eval.compare", str(args.compare), str(scorecard_dir)],
                             cwd=eval_repo, env=env, capture_output=True, text=True)
        if cmp.returncode != 0:
            raise RuntimeError(f"model.eval.compare failed: {cmp.stderr.strip()[-2000:]}")
        (scorecard_dir / "compare.txt").write_text(cmp.stdout)
        result["compare"] = str(scorecard_dir / "compare.txt")
        result["baseline"] = str(args.compare)
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--base-dir", type=Path, required=True)
    parser.add_argument("--adapter", type=Path)
    parser.add_argument("--corpus", type=Path, required=True)
    parser.add_argument("--label", required=True)
    parser.add_argument("--rows", type=int, required=True)
    parser.add_argument("--compare", type=Path)
    parser.add_argument("--eval-repo", type=Path)
    parser.add_argument("--out-root", type=Path)
    args, extra = parser.parse_known_args(argv)
    extra = [a for a in extra if a != "--"]
    try:
        result = evaluate(args, extra)
    except (RuntimeError, OSError) as error:
        print(f"evaluate failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result))
    return 0


if __name__ == "__main__":
    sys.exit(main())
