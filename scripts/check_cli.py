"""Black-box CLI acceptance checks. Outputs are temporary fixture artifacts."""
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
BIN = ROOT / "target/debug/knightlm"


def run(*args, succeeds=True):
    result = subprocess.run([str(BIN), *map(str, args)], cwd=ROOT, text=True, capture_output=True)
    if (result.returncode == 0) != succeeds:
        raise AssertionError(f"unexpected exit {result.returncode}: {args}: {result.stderr}")
    return result.stdout


def main():
    packet = json.loads(run("compile", "fixtures/repetition.json"))
    assert packet["facts"]["repetition_count"] == 3
    assert packet["facts"]["threefold_claim_now"] is True
    report = json.loads(run("evaluate-fixtures", "fixtures/start.json"))
    assert report["model_evaluated"] is False and len(report["runs"]) == 8
    masked = [r for r in report["runs"] if r["config"]["arm"] in ("legal_masking", "combined")]
    assert all(r["selected_legal"] and not r["raw_legal"] for r in masked)
    probes = json.loads(run("probes", "fixtures/start.json"))
    assert len(probes) == 61 and len({p["id"] for p in probes}) == 61
    svg = run("render", "fixtures/start.json", "--black-bottom", "--blue-theme")
    assert svg.count("<rect ") == 64 and svg.count("<text ") == 32
    assert "rnbqkbnr/" not in svg
    assert json.loads(run("validate-evals", "fixtures/evaluations.jsonl"))["validated"] == 1
    run("audit-splits", "fixtures/splits.json")
    run("validate-corpora", "fixtures/corpora.json")
    run("verify-observation", "fixtures/start.json", "fixtures/observation.json")
    replies = json.loads(run("simulate", "fixtures/start.json", "e2e4", "e7e5", "--budget", "2"))
    assert [r["branch"] for r in replies] == [1, 2]
    run("simulate", "fixtures/start.json", "e2e4", "e7e5", "--budget", "1", succeeds=False)
    for profile in ("smolvlm-500m", "qwen35-4b", "qwen35-9b"):
        admission = json.loads(run("verify-model", f"configs/models/{profile}.json", succeeds=False))
        assert admission["admitted"] is False and admission["missing"]
    with tempfile.TemporaryDirectory(prefix="knightlm-cli-") as folder:
        checkpoint = Path(folder) / "smoke.json"
        smoke = json.loads(run("train-smoke", checkpoint))
        assert smoke["evidence"] == "infrastructure_only"
        assert smoke["parameters_changed"] and smoke["reload_max_error"] == 0
        assert smoke["final_loss"] < smoke["initial_loss"] * 0.01
        original = checkpoint.read_bytes()
        run("train-smoke", checkpoint, succeeds=False)
        assert checkpoint.read_bytes() == original
        observation = Path(folder) / "wrong.json"
        observation.write_text(json.dumps({"schema_version": 1, "image_id": "wrong", "placement": "8/8/8/8/8/8/8/8"}))
        run("verify-observation", "fixtures/start.json", observation, succeeds=False)
    print(json.dumps({"cli_acceptance": "PASS", "arms": 8, "probes": 61, "pretrained_admission": "correctly_refused"}))


if __name__ == "__main__":
    main()
