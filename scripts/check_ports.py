"""Exercise protocol parity against the built Rust CLI; writes only temp artifacts."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def run(command, success=True, env=None):
    result = subprocess.run([str(x) for x in command], cwd=ROOT, text=True, capture_output=True, env=env)
    if success and result.returncode:
        raise RuntimeError(result.stderr)
    if not success and result.returncode == 0:
        raise AssertionError(f"accepted invalid packet: {command}")
    return result.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--require-mojo", action="store_true")
    args = parser.parse_args()
    binary = ROOT / "target/debug/knightlm"
    if not binary.is_file():
        raise RuntimeError("run cargo build --locked first")
    local = ROOT / "ports/mojo/.venv/bin/mojo"
    mojo = local if local.is_file() else shutil.which("mojo")
    if args.require_mojo and not mojo:
        raise RuntimeError("Mojo unavailable; install pinned toolchain")
    env = os.environ.copy()
    if local.is_file():
        env["PATH"] = str(local.parent) + os.pathsep + env["PATH"]
    with tempfile.TemporaryDirectory(prefix="knightlm-parity-") as temp:
        packet_path = Path(temp) / "packet.json"
        checked = 0
        for fixture in ("start", "repetition"):
            for helpers in ([], ["--legal-choices", "--features"]):
                packet_path.write_text(run([binary, "compile", f"fixtures/{fixture}.json", *helpers, "--compact"]))
                expected = run([binary, "canonicalize", packet_path])
                commands = [[sys.executable, ROOT / "ports/python/knightlm_contract.py", packet_path]]
                if mojo:
                    commands.append([mojo, ROOT / "ports/mojo/packet.mojo", ROOT / "ports/python", packet_path])
                for command in commands:
                    assert run(command, env=env) == expected, "packet parity mismatch"
                original = json.loads(expected)
                for malformed in (dict(original, teacher_score=999), dict(original, schema_version=999)):
                    packet_path.write_text(json.dumps(malformed))
                    run([binary, "canonicalize", packet_path], success=False)
                    for command in commands:
                        run(command, success=False, env=env)
                checked += 1
        print(json.dumps({"packet_variants": checked, "python": "PASS", "mojo": "PASS" if mojo else "UNAVAILABLE", "evidence": "protocol_parity_only"}))


if __name__ == "__main__":
    main()
