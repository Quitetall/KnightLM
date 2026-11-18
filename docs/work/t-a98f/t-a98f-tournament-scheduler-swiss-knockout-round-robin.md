---
format: openwarrant/1.0
profile: ticket
alias: t-a98f
uuid: 01a1a54a-31a4-70ff-ba8f-d428b8df4743
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Pairings, rounds, byes, tiebreaks and draw handling."
---
# Tournament scheduler: Swiss, knockout, round robin

## Checklist

- [x] Swiss pairings/rounds/byes/Buchholz/SB (i-572d, after t-5364) — done by claude, 2026-11-04: crates/ai/src/tournament.rs plus 18 tests in crates/ai/tests/tournament.rs.
- [x] Knockout bracket and draw rematch/seed (i-52f8) — done by claude, 2026-11-04: crates/ai/src/tournament.rs plus 18 tests in crates/ai/tests/tournament.rs.
- [x] Double round robin colour swap (i-8170) — done by claude, 2026-11-04: crates/ai/src/tournament.rs plus 18 tests in crates/ai/tests/tournament.rs.
- [x] Persistent queue, pause/resume/step, concurrency limits (i-a95a) — done by claude, 2026-11-04: The persistent queue, pause/resume/step and concurrency limits are in Running (crates/ai/src/tournament.rs): next_batch returns ready pairings up to a concurrency limit (default 1 for local models, 2 when any competitor is hosted), pause stops new games while games in flight still finish, resume continues from the stored state, a hard token/cost budget pauses at the cap, and the whole thing round-trips through schema v8 so a restart resumes exactly where it stopped (asserted by running_survives_a_restart_exactly_as_it_was and running_a_pause_survives_a_restart).
- [x] Scheduler tests (i-8a2c) — done by claude, 2026-11-04: crates/ai/src/tournament.rs plus 18 tests in crates/ai/tests/tournament.rs.

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-ai --all-features tournament
```

Result: 18 tests pass.
Covers i-572d, i-52f8, i-8170, i-a95a, i-8a2c.
