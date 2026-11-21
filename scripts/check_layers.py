#!/usr/bin/env python3
"""Fail when the rules layer depends on anything above it.

The rules layer is the code whose output defines what is true on the board: `crates/core`
(the compiler and packet) and `crates/notation` (FEN/PGN/CGT, built on shakmaty). Corpora pin its
git tree ids (model/corpus/s0.py), so anything that can change a rule must live inside it. It may
not depend on the app and model layers: crates/ai, crates/play, crates/player, crates/model,
crates/cli, crates/contract, or anything under apps/. brick-chess* is not part of the set: the
compiler does not need it (confirmed with `cargo metadata`; docs/model/seam.md).

Rule, on declared dependencies of every rules crate (normal, dev, build and target tables):
a path dependency (directly or through `workspace = true`) must resolve to another rules crate,
and no dependency may carry the package name of a forbidden crate. Reads files only.

Usage: check_layers.py [--root DIR]
"""
import argparse
from pathlib import Path
import sys
import tomllib

RULES = ("crates/core", "crates/notation")
FORBIDDEN_DIRS = ("crates/ai", "crates/play", "crates/player", "crates/model", "crates/cli", "crates/contract")
FORBIDDEN_TREES = ("apps",)  # every manifest below these directories
DEP_TABLES = ("dependencies", "dev-dependencies", "build-dependencies")


def load(path):
    return tomllib.loads(Path(path).read_text())


def package_name(manifest_path):
    return load(manifest_path).get("package", {}).get("name")


def forbidden_names(root):
    names = {}
    for rel in FORBIDDEN_DIRS:
        manifest = root / rel / "Cargo.toml"
        if manifest.is_file():
            names[package_name(manifest)] = rel
    for tree in FORBIDDEN_TREES:
        for manifest in sorted((root / tree).rglob("Cargo.toml")) if (root / tree).is_dir() else []:
            if "target" not in manifest.parts and package_name(manifest):
                names[package_name(manifest)] = str(manifest.parent.relative_to(root))
    return names


def dep_tables(manifest):
    for key in DEP_TABLES:
        yield manifest.get(key) or {}
    for target in (manifest.get("target") or {}).values():
        for key in DEP_TABLES:
            yield target.get(key) or {}


def check(root):
    root = Path(root).resolve()
    workspace = load(root / "Cargo.toml").get("workspace", {}).get("dependencies", {})
    banned = forbidden_names(root)
    allowed = {(root / rel).resolve() for rel in RULES}
    problems = []
    for rel in RULES:
        crate = (root / rel).resolve()
        manifest_path = crate / "Cargo.toml"
        if not manifest_path.is_file():
            problems.append(f"{rel}: rules crate not found")
            continue
        for table in dep_tables(load(manifest_path)):
            for name, spec in table.items():
                spec = spec if isinstance(spec, dict) else {"version": spec}
                resolved = workspace.get(name, {}) if spec.get("workspace") else spec
                resolved = resolved if isinstance(resolved, dict) else {}
                package = resolved.get("package") or spec.get("package") or name
                if package in banned:
                    problems.append(f"{rel}: depends on {package} ({banned[package]}), which is above the rules layer")
                path = resolved.get("path") or spec.get("path")
                if path is not None:
                    target = (crate / path).resolve() if not spec.get("workspace") else (root / path).resolve()
                    if target not in allowed:
                        shown = target.relative_to(root) if root in target.parents else target
                        problems.append(f"{rel}: path dependency {name} -> {shown} is outside the rules layer {list(RULES)}")
    return problems


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    problems = check(args.root)
    if problems:
        print("rules-layer boundary check FAILED:", file=sys.stderr)
        for problem in problems:
            print(f"  {problem}", file=sys.stderr)
        return 1
    print(f"rules-layer boundary check: ok ({', '.join(RULES)} depend on no app or model crate)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
