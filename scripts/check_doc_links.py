#!/usr/bin/env python3
"""Check relative Markdown links (file and #anchor) in the human-facing docs.

Scope: top-level *.md, docs/ (minus machine-written OpenWarrant records), model/,
apps/*/ *.md and .github/. Absolute URLs are not fetched. Exit 1 on a broken link.

    python3 scripts/check_doc_links.py [--root DIR] [FILE ...]
"""
import argparse
import re
import sys
from pathlib import Path
from urllib.parse import unquote

# Machine-written records: their links point at ticket files, journals and run artifacts.
SKIP_PREFIXES = ("docs/work/", "docs/tickets/", "docs/warrants/", "docs/generated/", "docs/receipts/",
                 "docs/gates/", "docs/authority/", "docs/sas/", "docs/adr/")
SKIP_DIRS = {"target", "node_modules", ".git", ".venv", "venv"}
LINK = re.compile(r"(?<!\!)\[[^\]]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)|!\[[^\]]*\]\(([^)\s]+)\)")
FENCE = re.compile(r"^\s*(```|~~~)")


def slug(heading):
    s = re.sub(r"[`*_]|<[^>]+>", "", heading.strip().lower())
    s = re.sub(r"[^\w\- ]", "", s, flags=re.UNICODE)
    return s.replace(" ", "-")


def anchors(path, cache={}):
    if path not in cache:
        found, seen, fenced = set(), {}, False
        for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
            if FENCE.match(line):
                fenced = not fenced
            m = None if fenced else re.match(r"#{1,6}\s+(.*?)\s*#*\s*$", line)
            if m:
                base = slug(m.group(1))
                n = seen.get(base, 0)
                seen[base] = n + 1
                found.add(base if n == 0 else f"{base}-{n}")
        cache[path] = found
    return cache[path]


def files(root):
    for p in sorted(root.rglob("*.md")):
        rel = p.relative_to(root).as_posix()
        if SKIP_DIRS & set(p.relative_to(root).parts) or rel.startswith(SKIP_PREFIXES):
            continue
        yield p


def check(root, targets):
    bad = []
    for p in targets:
        fenced = False
        for no, line in enumerate(p.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
            if FENCE.match(line):
                fenced = not fenced
            if fenced:
                continue
            line = re.sub(r"`[^`]*`", "", line)  # inline code is not a link
            for m in LINK.finditer(line):
                url = m.group(1) or m.group(2)
                if re.match(r"[a-z][a-z0-9+.-]*:", url, re.I) or url.startswith(("//", "<")):
                    continue
                path, _, frag = url.partition("#")
                dest = p if not path else (root / unquote(path).lstrip("/") if path.startswith("/")
                                           else (p.parent / unquote(path)))
                if not dest.exists():
                    bad.append(f"{p.relative_to(root)}:{no}: missing file {url}")
                elif frag and dest.suffix == ".md" and dest.is_file() and unquote(frag).lower() not in anchors(dest):
                    bad.append(f"{p.relative_to(root)}:{no}: missing anchor {url}")
    return bad


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--root", default=Path(__file__).resolve().parent.parent, type=Path)
    ap.add_argument("files", nargs="*", type=Path)
    a = ap.parse_args()
    root = a.root.resolve()
    targets = [f.resolve() for f in a.files] or list(files(root))
    bad = check(root, targets)
    print("\n".join(bad))
    print(f"checked {len(targets)} files, {len(bad)} broken links")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
