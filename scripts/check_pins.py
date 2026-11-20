"""Fail when shared dependency pins disagree across the workspace and the excluded apps.

The apps under apps/ are separate Cargo workspaces with their own manifests and
lockfiles, so a version the root workspace pins (shakmaty =0.30.2, ...) is
hand-copied into each of them. Two rules, both on declared pins, not on
transitive resolution (which legitimately differs per lockfile):

1. A dependency declared in the root `[workspace.dependencies]` and in an app
   manifest must carry the same version requirement string in both.
2. An exact pin (`=x.y.z`) anywhere must be the only resolved version of that
   crate in every lockfile that contains it (the lockfile next to the manifest
   that declares it, plus any other lockfile that resolves the crate).

Usage: check_pins.py [--root DIR]. Reads files only; writes nothing.
"""
import argparse
from pathlib import Path
import sys
import tomllib

SKIP_DIRS = {"target", "node_modules", ".git", "dist"}
DEP_TABLES = ("dependencies", "dev-dependencies", "build-dependencies")


def req_of(spec):
    """The version requirement of a dependency spec, or None for path/git-only."""
    if isinstance(spec, str):
        return spec
    if isinstance(spec, dict):
        return spec.get("version")
    return None


def declared(manifest):
    """name -> requirement for every versioned dependency a manifest declares."""
    found = {}

    def take(table):
        for name, spec in (table or {}).items():
            req = req_of(spec)
            if req is not None:
                found[name] = req

    for key in DEP_TABLES:
        take(manifest.get(key))
    take(manifest.get("workspace", {}).get("dependencies"))
    for target in manifest.get("target", {}).values():
        for key in DEP_TABLES:
            take(target.get(key))
    return found


def manifests(root):
    out = []
    for path in sorted(root.rglob("Cargo.toml")):
        rel = path.relative_to(root)
        if SKIP_DIRS.intersection(rel.parts):
            continue
        out.append(path)
    return out


def lock_versions(path):
    versions = {}
    for package in tomllib.loads(path.read_text()).get("package", []):
        versions.setdefault(package["name"], set()).add(package["version"])
    return versions


def check(root):
    root = Path(root)
    problems = []
    root_manifest = root / "Cargo.toml"
    root_decl = declared(tomllib.loads(root_manifest.read_text()))
    sources = {}  # manifest path -> declared
    for path in manifests(root):
        sources[path] = declared(tomllib.loads(path.read_text()))
    # Rule 1: same requirement string wherever the root workspace and an app agree on a name.
    for path, decl in sources.items():
        if path == root_manifest:
            continue
        for name, req in sorted(decl.items()):
            if name in root_decl and root_decl[name] != req:
                problems.append(
                    f"{path.relative_to(root)}: {name} requires {req!r}, "
                    f"root Cargo.toml requires {root_decl[name]!r}"
                )
    # Rule 2: an exact pin resolves to that version, alone, in every lockfile that has the crate.
    pins = {}
    for path, decl in sources.items():
        for name, req in decl.items():
            if req.startswith("="):
                pins.setdefault(name, {}).setdefault(req[1:].strip(), []).append(path)
    for name, by_version in sorted(pins.items()):
        if len(by_version) > 1:
            problems.append(
                f"{name}: exact pins disagree: "
                + ", ".join(f"={v} in {', '.join(str(p.relative_to(root)) for p in ps)}" for v, ps in sorted(by_version.items()))
            )
    for lock in sorted(p for p in root.rglob("Cargo.lock") if not SKIP_DIRS.intersection(p.relative_to(root).parts)):
        resolved = lock_versions(lock)
        for name, by_version in pins.items():
            if name in resolved:
                want = next(iter(by_version)) if len(by_version) == 1 else None
                if want is not None and resolved[name] != {want}:
                    problems.append(
                        f"{lock.relative_to(root)}: {name} resolves to {sorted(resolved[name])}, pinned ={want}"
                    )
    return problems


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    problems = check(args.root)
    if problems:
        print("shared pin check FAILED:", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        return 1
    print("shared pin check: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
