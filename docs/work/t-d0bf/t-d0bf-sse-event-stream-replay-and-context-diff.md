---
format: openwarrant/1.0
profile: ticket
alias: t-d0bf
uuid: 01a1a54a-3105-77f5-8869-4527b594c6fe
priority: 0
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Expose normalized events with event ids, reconnect and replay/diff."
---
# SSE event stream, replay and context diff

## Checklist

- [x] AiEvent enum and event log (i-c07c, after t-7e92) — done by claude, 2026-11-04
- [x] SSE endpoint with after/event id (i-23bd) — done by claude, 2026-11-04
- [x] Replay endpoint rebuilds context (i-14be) — done by claude, 2026-11-04
- [x] Match/diff result for a stored turn (i-402d) — done by claude, 2026-11-04
- [x] Stream/reconnect tests (i-63c5) — done by claude, 2026-11-04

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-ai --all-features stream
```

Result: 9 of 9 tests pass (stream binary) covering the event log and the replay diff. The --all-features flag is required because the log lives behind the host cargo feature. Additionally verified end to end against a mock provider: a two-round turn recorded 13 ordered events, a reconnect at after=10 returned exactly 11-13, and replay/2 reported an identical context.
Covers i-c07c, i-23bd, i-14be, i-402d, i-63c5.
