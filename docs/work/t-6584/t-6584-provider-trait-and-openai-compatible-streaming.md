---
format: openwarrant/1.0
profile: ticket
alias: t-6584
uuid: 01a1a54a-30f3-7103-b183-691179a47b53
priority: 0
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Implement the first provider adapter with tools, SSE, usage and normalized errors."
---
# Provider trait and OpenAI-compatible streaming

## Checklist

- [x] Provider trait and request/response types (i-0ddd, after t-e833) — done by claude, 2026-11-04
- [x] OpenAI-compatible chat completions + tools (i-6af4) — done by claude, 2026-11-04
- [x] SSE parsing for content/reasoning/tool deltas (i-2cc3) — done by claude, 2026-11-04
- [x] Usage, timeout, cancellation, 429/5xx errors (i-e7ed) — done by claude, 2026-11-04
- [x] Mock-server conformance tests (i-cdd6) — done by claude, 2026-11-04

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: …/code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-ai --all-features provider
```

Result 2026-11-04: 31 unit and 16 mock-server integration tests pass
(`cargo test -p rbc-ai --all-features` → 66 lib + 16 integration). The
`--all-features` flag is required because `provider` is a cargo feature that
`host` enables.

