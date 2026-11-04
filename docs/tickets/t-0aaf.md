---
format: openwarrant/1.0
profile: ticket
alias: t-0aaf
uuid: 01a1a54a-32a6-7504-9576-c574c50a71a1
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Tag, run all gates, record evidence and hand off signing."
---
# Release candidate and signing handoff

## Checklist

- [ ] Release build and install (i-fcca, after t-1b87)
- [ ] RC smoke test (i-c106)
- [ ] Evidence bundle (i-4f9e)
- [ ] Signed/updater handoff (i-7638)
- [ ] Release notes (i-772d)

## CHK-rc · check — rc
run: cd /mnt/4tb/KnightLM && cargo build --release
on: done
evaluates i-fcca, i-c106, i-4f9e, i-7638, i-772d
