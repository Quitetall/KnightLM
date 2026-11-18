---
format: openwarrant/1.0
profile: ticket
alias: t-7e92
uuid: 01a1a54a-30fb-71f2-83a8-87f50d0f5179
priority: 0
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Drive a turn from context compilation through tool calls to a stored turn."
---
# Agent loop: tools, retries, cancellation and context hash

## Checklist

- [x] Compile context per turn and store hash (i-954f, after t-e833, t-4836, t-b1ab, t-4eb3, t-6584) — done by claude, 2026-11-04
- [x] Execute tool calls, feed results, max rounds (i-c35f) — done by claude, 2026-11-04
- [x] Illegal move/tool-error retries (i-b811) — done by claude, 2026-11-04
- [x] Cancellation stops further tool writes (i-f467) — done by claude, 2026-11-04
- [x] Turn/usage/error persistence tests (i-6b2e) — done by claude, 2026-11-04

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-ai --all-features agent_loop
```

Result: 15 of 15 integration tests pass (agent_loop binary); the full crate suite is 66 unit + 15 agent_loop + 16 provider. The --all-features flag is required because agent and provider are cargo features.
Covers i-954f, i-c35f, i-b811, i-f467, i-6b2e.
