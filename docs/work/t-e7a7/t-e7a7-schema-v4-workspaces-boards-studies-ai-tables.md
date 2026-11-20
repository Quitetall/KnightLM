---
format: openwarrant/1.0
profile: ticket
alias: t-e7a7
uuid: 01a1a54a-306b-74c5-a3cb-7803628a2258
priority: 0
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Add the v4 tables, migration, legacy session backup and reset; bump SCHEMA_VERSION to 4."
---
# Schema v4: workspaces, boards, studies, AI tables and reset

## Checklist

- [x] Add workspaces, boards, board_revisions, studies, study_chapters, study_versions, study_progress, ai_* and legacy_sessions tables (i-35be) — done by claude, 2026-11-04
- [x] Bump SCHEMA_VERSION to 4 and implement migrate_v4; report schema in /api/health (i-29e9) — done by claude, 2026-11-04
- [x] Copy old session payloads into legacy_sessions and clear active session rows (i-ffd0) — done by claude, 2026-11-04
- [x] Add indexes for parent, revision, session/seq, approval status and progress (i-3c9e) — done by claude, 2026-11-04
- [x] Add migration/reset tests and a v3 fixture (i-876b) — done by claude, 2026-11-04
