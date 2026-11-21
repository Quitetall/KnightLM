#!/usr/bin/env python3
"""Run unittest modules and print an explicit SKIPPED list (module, test, reason).

`python3 -m unittest` reports only "skipped=N", which hides that a whole torch suite
did not run. This runner prints every skip, grouped by module and reason, and can
fail when a skip reason matches a pattern (`--forbid-skip REGEX`): CI uses that in
the job that installs torch, so a missing dependency there is an error, not a skip.

    python3 scripts/run_py_tests.py model.eval.test_oracles ...
    python3 scripts/run_py_tests.py --forbid-skip 'torch|Pillow|peft' model.perceiver.test_perceiver
"""
import argparse
import re
import sys
import unittest
from pathlib import Path
from collections import defaultdict


class Result(unittest.TextTestResult):
    def __init__(self, *a, **k):
        super().__init__(*a, **k)
        self.skip_log = []

    def addSkip(self, test, reason):
        super().addSkip(test, reason)
        self.skip_log.append((test.__class__.__module__, test.id(), reason))


def skip_summary(skips):
    groups = defaultdict(list)
    for module, test_id, reason in skips:
        groups[(module, reason)].append(test_id.rsplit(".", 1)[-1])
    lines = [f"SKIPPED ({len(skips)} tests in {len(groups)} module/reason groups):"]
    for (module, reason), names in sorted(groups.items()):
        lines.append(f"  {module}: {len(names)} skipped, reason: {reason}")
    return lines


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--forbid-skip", metavar="REGEX", help="fail if any skip reason matches")
    parser.add_argument("modules", nargs="+")
    args = parser.parse_args(argv)
    sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
    suite = unittest.defaultTestLoader.loadTestsFromNames(args.modules)
    result = unittest.TextTestRunner(resultclass=Result, verbosity=1).run(suite)
    print()
    if result.skip_log:
        print("\n".join(skip_summary(result.skip_log)))
    else:
        print("SKIPPED: none")
    bad = [s for s in result.skip_log if args.forbid_skip and re.search(args.forbid_skip, s[2])]
    if bad:
        print(f"FAIL: {len(bad)} skips match --forbid-skip {args.forbid_skip!r}; the dependency must be installed here")
    return 0 if result.wasSuccessful() and not bad else 1


if __name__ == "__main__":
    sys.exit(main())
