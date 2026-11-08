---
format: openwarrant/1.0
profile: ticket
alias: t-a226
uuid: 01a1b88a-dc35-720a-a208-bade9b0ee3eb
priority: 2
created_at: "2026-11-08T00:24:30Z"
created_by: "codex-contributor-setup"
summary: "Report whether the PowerShell quickstart opens RBC and creates a fresh per-user database on native Windows."
labels:
  - "release-engineer"
---
# Windows Tester — Try the RBC source launcher on native Windows

**Time and setup.** About 45–90 minutes of active time after the
prerequisites are installed. You need a native Windows host, PowerShell, and
the MSVC prerequisites in the [Windows source guide](../app/running.md#windows-from-source).
A maintainer can help with missing setup. The first build may take longer
than the time estimate.

Use a disposable database and run the documented launcher. Check that the
browser opens, the board loads, and the app creates its database under Local
AppData when launched with the default settings. If setup or build fails,
capture the first relevant error and the step that produced it.

**Review.** Report Windows version and architecture, source commit,
prerequisites, launch command, and observed result. Treat a blocked launch
as a reproduction to hand back. Installer checks belong to the
[full Windows qualification order](t-1ce9.md).

## Checklist


- [ ] Run the PowerShell source quickstart on native Windows, or capture the blocker. (i-7051)
- [ ] Share the platform, commit, command, and observed result. (i-da09)
