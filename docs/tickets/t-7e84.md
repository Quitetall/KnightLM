---
format: openwarrant/1.0
profile: ticket
alias: t-7e84
uuid: 01a1a54a-323f-775b-b19e-242fe4af6586
priority: 2
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Pin alignment, coverage, export and no-mutation."
---
# Compare tests and read-only guard

## Checklist

- [x] Alignment fixtures (i-e3ef, after t-0ba4, t-92ef, t-c76d) — done by claude, 2026-11-06 [observed]: crates/ai/tests/diff_fixtures.rs: chapters align by id (a copy), by ordinal (a rename keeps its place), one-sided chapters are removed; boards pair by FEN with counters stripped.
- [x] Coverage fixtures (i-b736) — done by claude, 2026-11-06 [observed]: One pair differing in moves, shapes, annotations, board notes, chapter notes, metadata, progress and tags - each asserted in the report AND the Markdown export - plus the negative case that identical content produces no chapter noise.
- [x] Read-only assertions (i-7322) — done by claude, 2026-11-06 [observed]: comparing_touches_no_table_it_does_not_read asserts two full comparisons write no version and no audit row and move no revision or updated stamp; a_report_is_a_snapshot_not_a_live_handle asserts a report does not change after the study does.
- [x] One suite command (i-9755) — done by claude, 2026-11-06 [observed]: scripts/check_compare.sh runs the engine, the matrix, both read-only tests and the browser; check_all.sh calls it. It fails if a filtered run selects no test.

## CHK-compare-suite · check — compare-suite
run: cd /mnt/4tb/KnightLM && ./scripts/check_compare.sh
on: done
evaluates i-e3ef, i-b736, i-7322, i-9755
