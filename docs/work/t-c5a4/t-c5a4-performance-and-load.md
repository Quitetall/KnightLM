---
format: openwarrant/1.0
profile: ticket
alias: t-c5a4
uuid: 01a1a54a-3276-7062-83f1-dfa6519e5eb5
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Measure 1M DB, long sessions, SSE and 16-model tournaments."
---
# Performance and load

## Checklist

- [x] 1M-game DB query/explorer/position search timings (i-967b, after t-2883, t-a994, t-7e84, t-3241) — done by claude, 2026-11-06 [observed]: 1,000,000 synthetic games in one 3.6 GB database on tmpfs: first page 15 ms, by result 15 ms, by player 6 ms, Elo band 170 ms, date range 42 ms, position search 3 ms (500 hits), explorer 1,312 ms. The Elo band was 301 ms at 150k before this pass because the filter was two OR groups each spanning two columns; one range per column fixed it. The explorer is now the outlier and docs/benchmarks.md says so.
- [x] Transcript/context volume and indexes (i-075a) — done by claude, 2026-11-06 [observed]: 20,000 turns in one conversation: written at 15,834/s, whole conversation read in 53 ms, newest-sequence read 34 ms. crates/rbc-db/tests/perf_plans.rs asserts the plans for reading a conversation, deleting one, replaying events from a reconnect cursor, the newest sequence, and the Elo band - a plan is deterministic and cannot flake on a busy machine.
- [x] 16-model tournament concurrency (i-d970) — done by claude, 2026-11-06 [observed]: Sixteen models, one Swiss round, concurrency 4: the scheduler starts four pairings per step and returns the head of the unfinished list, so stepping again before they finish returns the same four. Recorded because a caller counting started as new games overstates the schedule eightfold. A full tournament needs a provider that plays legal moves; cdp_tournament.js exercises that.
- [x] SSE/latency under load (i-d8b9) — done by claude, 2026-11-06 [observed]: 24 concurrent conversations against the mock provider: all 24 completed, median turn 48 ms and slowest 58 ms. The mock answers in milliseconds, so this measures the plumbing, not a model; at that speed first-event and total latency are not separable and the report says so.
- [x] Recorded benchmark report (i-00f6) — done by claude, 2026-11-06 [observed]: docs/benchmarks.md records every number with the conditions it was measured under, including that the 4 TB volume was in an I/O failure state so the large runs use a tmpfs and the import rate is marked optimistic while the query latencies are not. It also lists what it does not measure: memory growth, provider latency, and concurrency beyond the counts given.

## CHK-perf · check — perf
run: cd /mnt/4tb/KnightLM && cargo test -p rbc-db --test perf_plans && cargo build --release --example bench_db --manifest-path apps/rusty-brick-chess/Cargo.toml
on: done
evaluates i-967b, i-075a, i-d970, i-d8b9, i-00f6
