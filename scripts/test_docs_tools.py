"""Tests for scripts/check_doc_links.py and scripts/sync_issues.py (stdlib only)."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import check_doc_links as links  # noqa: E402
import sync_issues as sync  # noqa: E402


class LinkTests(unittest.TestCase):
    def check(self, files):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for name, text in files.items():
                (root / name).parent.mkdir(parents=True, exist_ok=True)
                (root / name).write_text(text)
            return links.check(root, [root / n for n in files if n.endswith(".md")])

    def test_good_links_pass(self):
        self.assertEqual(self.check({"a.md": "[b](b.md#top) [s](#sec)\n## Sec\n", "b.md": "# Top\n"}), [])

    def test_missing_file_and_anchor_are_reported(self):
        bad = self.check({"a.md": "[x](gone.md) [y](b.md#nope)\n", "b.md": "# Top\n"})
        self.assertEqual(len(bad), 2)
        self.assertIn("missing file gone.md", bad[0])
        self.assertIn("missing anchor b.md#nope", bad[1])

    def test_code_and_urls_are_ignored(self):
        text = "[u](https://example.com/x) `[c](nope.md)`\n```\n[f](nope.md)\n```\n"
        self.assertEqual(self.check({"a.md": text}), [])


TICKET = {"id": "t-1234", "title": "Role — Do a thing", "state": "open", "level": "ticket",
          "done": 0, "total": 2, "priority": 2, "labels": ["docs"], "claims": [], "dir": "docs/work/t-nonexistent"}


class SyncTests(unittest.TestCase):
    def test_body_has_marker_and_claim_command(self):
        body = sync.body_for(TICKET, "o/r")
        self.assertEqual(sync.MARKER_RE.search(body).group(1), "t-1234")
        self.assertIn("war start t-1234", body)

    def test_existing_marker_makes_it_idempotent(self):
        body = sync.body_for(TICKET, "o/r")
        todo, skip = sync.plan([TICKET], {m.group(1) for m in sync.MARKER_RE.finditer(body)})
        self.assertEqual((todo, len(skip)), ([], 1))
        todo, skip = sync.plan([TICKET], None)
        self.assertEqual((len(todo), skip), (1, []))

    def test_done_tickets_are_not_mirrored(self):
        with tempfile.TemporaryDirectory() as tmp:
            f = Path(tmp) / "t.json"
            f.write_text(json.dumps({"result": {"tickets": [TICKET, dict(TICKET, id="t-9", state="done")]}}))
            self.assertEqual([t["id"] for t in sync.load_tickets(f)], ["t-1234"])


if __name__ == "__main__":
    unittest.main()
