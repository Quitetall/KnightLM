import unittest

from scripts.check_rbc_ui_parity import CHECK, FIELDS, PLAN, validate


class ParityEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.plan = PLAN.read_text()
        self.ids = CHECK.findall(self.plan)
        self.rows = [
            dict.fromkeys(FIELDS, "fixture") | {"id": item, "build": "candidate", "status": "PASS"}
            for item in self.ids
        ]

    def test_complete_and_slice(self):
        self.assertFalse(validate(self.plan, self.rows, "candidate", None)[1])
        self.assertFalse(validate(self.plan, self.rows[:1], "candidate", self.ids[:1])[1])
        self.assertTrue(validate(self.plan, self.rows[:1], "candidate", None)[1])

    def test_bad_evidence(self):
        for field, value in (
            ("evidence", ""), ("status", "FAIL"), ("status", "UNKNOWN"),
            ("build", "stale"), ("id", "RBCUI-NO-01"),
        ):
            with self.subTest(field=field, value=value):
                rows = self.rows[:-1] + [self.rows[-1] | {field: value}]
                self.assertTrue(validate(self.plan, rows, "candidate", None)[1])

    def test_duplicate_evidence(self):
        self.assertTrue(validate(self.plan, self.rows + self.rows[-1:], "candidate", None)[1])

    def test_duplicate_checklist(self):
        duplicated = self.plan + f"\n- [ ] {self.ids[0]} repeated\n"
        with self.assertRaisesRegex(ValueError, "duplicate"):
            validate(duplicated, self.rows, "candidate", None)
