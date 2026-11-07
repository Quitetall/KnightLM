---
format: openwarrant/1.0
profile: ticket
alias: t-5b6e
uuid: 01a1b7dc-2916-7475-aeac-b79c78a7686f
priority: 1
created_at: "2026-11-07T21:13:41Z"
created_by: "codex-contributor-setup"
summary: "Run the full RBC browser suite with an isolated database, mock providers, and test-owned processes."
labels:
  - "qa-engineer"
paths:
  - "scripts/check_all.sh"
  - "scripts/check_browser.sh"
  - "apps/rusty-brick-chess/frontend/tests/cdp/README.md"
---
# QA Engineer — Isolate the RBC browser release gate

Start from `scripts/check_all.sh` and the CDP suite README. Reproduce the approvals regression against the built frontend as the first smoke. Make process and port ownership explicit for each run so contributors can test alongside a running RBC app.

## Done when

- Document one command that runs every registered browser suite against the packaged frontend. It provisions its own database, mock provider, ports, and browser profiles.
- Demonstrate that an unrelated RBC instance and Chrome process survive the run. Verify the runner cleans up its own resources on both success and a forced failure.
- Attach one uninterrupted run with a passing result for every registered suite and the tested build identity.

## Checklist


- [ ] Launch a test server with an isolated database, mock provider, and browser profiles. (i-cf72)
- [ ] Prove the runner cleans up its own children while an unrelated browser or RBC server stays running. (i-6dfa)
- [ ] Record one uninterrupted browser run against the packaged frontend with a pass marker for every suite. (i-9e37)

## M-i-640e · milestone — Signed off by a person
min: signed

- [ ] Signed off by a person (i-640e)
