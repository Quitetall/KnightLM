#!/usr/bin/env python3
"""Mirror OPEN OpenWarrant tickets to GitHub issues, one issue per ticket.

DRY RUN BY DEFAULT: it prints what it would create and touches nothing. Pass --apply to
create issues (needs an authenticated `gh` with issue write access; creating an issue with a
label the repository does not have creates that label).

The ticket stays the record. The issue only points at it and says how to claim it.
Idempotent: every issue body carries a hidden marker `<!-- knightlm-ticket: <id> -->`; a ticket
whose marker already appears in any issue (open or closed) is skipped. Closing issues for done
tickets is not done here.

    python3 scripts/sync_issues.py                  # dry run (reads `war view tickets --json`)
    python3 scripts/sync_issues.py --apply          # create the missing issues
    python3 scripts/sync_issues.py --tickets-json F # use a saved `war view tickets --json` output
    python3 scripts/sync_issues.py --existing-json F  # use a saved `gh issue list --json body,...`
"""
import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_REPO = "Quitetall/KnightLM"
MARKER = "<!-- knightlm-ticket: {} -->"
MARKER_RE = re.compile(r"<!--\s*knightlm-ticket:\s*(t-[0-9a-f]+)\s*-->")


def run(argv, timeout=120):
    return subprocess.run(argv, capture_output=True, text=True, timeout=timeout, cwd=REPO_ROOT)


def load_tickets(path):
    if path:
        data = json.loads(Path(path).read_text())
    else:
        r = run(["war", "view", "tickets", "--json"])
        if r.returncode != 0:
            sys.exit(f"war view tickets failed: {r.stderr.strip()[:300]}")
        data = json.loads(r.stdout)
    tickets = data["result"]["tickets"] if "result" in data else data["tickets"]
    return [t for t in tickets if t.get("state") != "done"]


def ticket_file(t):
    """The ticket's Markdown master: <dir>/<id>-*.md, or <dir>.md (docs/tickets/<id>.md)."""
    d = REPO_ROOT / t.get("dir", "")
    for cand in sorted(d.glob(f"{t['id']}-*.md")) + [d.with_suffix(".md")]:
        if cand.is_file():
            return cand.relative_to(REPO_ROOT).as_posix()
    return None


def summary_of(rel):
    if not rel:
        return ""
    m = re.search(r'^summary:\s*"?(.*?)"?\s*$', (REPO_ROOT / rel).read_text(), re.M)
    return m.group(1) if m else ""


def labels_for(t):
    out = ["ticket", f"priority:{t.get('priority', '?')}"]
    out += [f"role:{x}" for x in t.get("labels", [])]
    out.append(f"level:{t.get('level', 'ticket')}")
    return out


def title_for(t):
    return f"[{t['id']}] {t['title']}"


def body_for(t, repo):
    rel = ticket_file(t)
    link = f"https://github.com/{repo}/blob/main/{rel}" if rel else "(ticket file not found)"
    claims = [c for c in t.get("claims", []) if c.get("actor")]
    held = f"Currently claimed by `{claims[0]['actor']}`." if claims else "Not claimed."
    return "\n".join([
        MARKER.format(t["id"]),
        summary_of(rel) or t["title"],
        "",
        f"- **Ticket:** [`{rel}`]({link}) is the record. This issue only mirrors it.",
        f"- **Progress:** {t.get('done', 0)} of {t.get('total', 0)} items done. Priority {t.get('priority', '?')}. {held}",
        "",
        "## How to pick this up",
        "",
        "1. Read the ticket and the [getting started guide]"
        f"(https://github.com/{repo}/blob/main/docs/contributors/README.md).",
        f"2. With `war` installed: `war start {t['id']} --as your-github-handle`. Without it, comment here.",
        "3. Open a draft pull request that mentions the ticket id, with the first result or the blocker.",
        f"4. When reviewed: `war done {t['id']} --as your-github-handle --note \"Evidence in PR\"`.",
        "",
        f"See [CONTRIBUTING](https://github.com/{repo}/blob/main/CONTRIBUTING.md#claiming-work) and the "
        f"[work orders](https://github.com/{repo}/blob/main/docs/contributors/work/README.md).",
        "",
    ])


def existing_ids(repo, path):
    """Ticket ids already marked in any issue. None when it could not be read (dry run goes on)."""
    if path:
        issues = json.loads(Path(path).read_text())
    else:
        r = run(["gh", "issue", "list", "-R", repo, "--state", "all", "--limit", "1000", "--json", "number,body"])
        if r.returncode != 0:
            print(f"note: could not list existing issues ({r.stderr.strip()[:160]}); assuming none", file=sys.stderr)
            return None
        issues = json.loads(r.stdout)
    return {m.group(1) for i in issues for m in MARKER_RE.finditer(i.get("body") or "")}


def create(t, repo):
    argv = ["gh", "api", f"repos/{repo}/issues", "-f", f"title={title_for(t)}", "-f", f"body={body_for(t, repo)}"]
    for label in labels_for(t):
        argv += ["-f", f"labels[]={label}"]
    r = run(argv)
    if r.returncode != 0:
        sys.exit(f"creating {t['id']} failed: {r.stderr.strip()[:300]}")
    return json.loads(r.stdout)["html_url"]


def plan(tickets, existing):
    done = existing or set()
    return [t for t in tickets if t["id"] not in done], [t for t in tickets if t["id"] in done]


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--apply", action="store_true", help="create the issues (default: dry run)")
    ap.add_argument("--repo", default=DEFAULT_REPO)
    ap.add_argument("--tickets-json")
    ap.add_argument("--existing-json")
    ap.add_argument("--verbose", action="store_true", help="print each issue body in a dry run")
    a = ap.parse_args()
    tickets = load_tickets(a.tickets_json)
    existing = existing_ids(a.repo, a.existing_json)
    todo, skip = plan(tickets, existing)
    mode = "APPLY" if a.apply else "DRY RUN (nothing is created; pass --apply)"
    print(f"{mode}: {len(tickets)} open tickets, {len(todo)} to create, {len(skip)} already mirrored")
    for t in todo:
        print(f"- {title_for(t)}\n    labels: {', '.join(labels_for(t))}")
        if a.verbose:
            print("    " + body_for(t, a.repo).replace("\n", "\n    "))
        if a.apply:
            print(f"    created {create(t, a.repo)}")
    for t in skip:
        print(f"= {t['id']} already has an issue")
    return 0


if __name__ == "__main__":
    sys.exit(main())
