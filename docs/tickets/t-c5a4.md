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

- [ ] 1M-game DB query/explorer/position search timings (i-967b, after t-2883, t-a994, t-7e84, t-3241)
- [ ] Transcript/context volume and indexes (i-075a)
- [ ] 16-model tournament concurrency (i-d970)
- [ ] SSE/latency under load (i-d8b9)
- [ ] Recorded benchmark report (i-00f6)

## CHK-perf · check — perf
run: cd /mnt/4tb/KnightLM && cargo test -p rbc-ai perf -- --ignored
on: done
evaluates i-967b, i-075a, i-d970, i-d8b9, i-00f6
