#!/usr/bin/env python3
"""The files the app ships so it can run the model server (docs/app/models.md).

`packaging/model-runtime.files` lists them, one repo-relative path per line. Both installers
(packaging/install.sh, apps/rusty-brick-chess/install.sh) copy exactly that list under
`share/rustybrickchess/model-runtime/`, and the Tauri bundle carries the same files as resources
(`apps/rusty-brick-chess/src-tauri/tauri.conf.json`). This check fails when

- a listed file does not exist,
- `model.serve.server` or `model.serve.fake_server` imports a `model.*` module the list lacks
  (the installed app would die with ImportError),
- the Tauri resources differ from the list.

`--root DIR` checks another tree (the self-test plants violations in a copy).
"""
import argparse
import ast
import json
import sys
from pathlib import Path

ENTRY_POINTS = ("model.serve.server", "model.serve.fake_server")
DATA_FILES = {
    "crates/contract/schemas/model_tools.json",
    "model/configs/models/smolvlm-500m.json",
    "model/serve/requirements.txt",
}


def module_file(root: Path, name: str):
    base = name.replace(".", "/")
    for candidate in (f"{base}.py", f"{base}/__init__.py"):
        if (root / candidate).exists():
            return candidate
    return None


def closure(root: Path):
    seen, todo = set(), list(ENTRY_POINTS)
    while todo:
        name = todo.pop()
        if name in seen:
            continue
        path = module_file(root, name)
        if path is None:
            continue
        seen.add(name)
        parts = name.split(".")
        todo.extend(".".join(parts[:i]) for i in range(1, len(parts)))
        for node in ast.walk(ast.parse((root / path).read_text())):
            if isinstance(node, ast.Import):
                todo.extend(a.name for a in node.names if a.name.startswith("model"))
            elif isinstance(node, ast.ImportFrom) and node.module and node.module.startswith("model"):
                todo.append(node.module)
                todo.extend(f"{node.module}.{a.name}" for a in node.names)
    return {module_file(root, name) for name in seen}


def check(root: Path):
    errors = []
    listed = [line.strip() for line in (root / "packaging/model-runtime.files").read_text().splitlines()
              if line.strip() and not line.startswith("#")]
    for path in listed:
        if not (root / path).is_file():
            errors.append(f"listed but missing: {path}")
    for path in sorted(closure(root) | DATA_FILES):
        if path not in listed:
            errors.append(f"needed by the model server but not listed: {path}")
    conf = json.loads((root / "apps/rusty-brick-chess/src-tauri/tauri.conf.json").read_text())
    resources = conf["bundle"]["resources"]
    shipped = {dest[len("model-runtime/"):] for dest in resources.values() if dest.startswith("model-runtime/")}
    for path in listed:
        if path not in shipped:
            errors.append(f"not in the Tauri bundle resources: {path}")
    for path in shipped - set(listed):
        errors.append(f"in the Tauri bundle but not listed: {path}")
    return errors


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    errors = check(parser.parse_args().root)
    for error in errors:
        print(f"model runtime files: {error}", file=sys.stderr)
    if errors:
        return 1
    print("model runtime files: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
