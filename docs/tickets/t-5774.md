---
format: openwarrant/1.0
profile: ticket
alias: t-5774
uuid: 01a1a54a-3095-750e-b5b9-92ff1235377b
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Pin migration, reset, repository and frontend board behaviour with fixtures and CDP."
---
# Foundation fixtures and regression tests

## Checklist

- [x] v3→v4 fixture and reset assertions (i-fcea, after t-e7a7, t-312c, t-4200, t-6c55, t-9c43) — done by claude, 2026-11-04: crates/rbc-db/tests/foundation.rs.
- [x] Board/study repository fixtures (i-e4ba) — done by claude, 2026-11-04: crates/rbc-db/tests/foundation.rs.
- [x] Frontend multi-board CDP fixture (i-31ee) — done by claude, 2026-11-04: frontend/tests/cdp/cdp_cross_board.js is the multi-board CDP fixture: it builds three boards through the strip and asserts the target marker, the off-screen badge and the left-rail tree.
- [x] Add tests to the existing suite runner (i-af11) — done by claude, 2026-11-04: scripts/check_all.sh runs cargo test --workspace and --workspace --all-features, so every fixture above runs in the one command.

## Verification

`war done --check` cannot run checks in this layout: the runner stops with
`source-basis-unavailable: .../code: Too many levels of symbolic links (os error 40)`
because the OpenWarrant root is not a git checkout and its `code` entry is a symlink
to `/mnt/4tb/KnightLM`. The check was run by hand instead:

```bash
cd /mnt/4tb/KnightLM && cargo test -p rbc-db --test foundation
```

Result: 5 of 5 pass.
Covers i-fcea, i-e4ba, i-31ee, i-af11.
