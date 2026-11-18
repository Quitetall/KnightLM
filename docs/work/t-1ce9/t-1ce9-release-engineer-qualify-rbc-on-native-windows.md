---
format: openwarrant/1.0
profile: ticket
alias: t-1ce9
uuid: 01a1b7dc-295a-7035-9ce3-9a0cf802d788
priority: 2
created_at: "2026-11-07T21:13:41Z"
created_by: "codex-contributor-setup"
summary: "Verify the Windows source launcher and desktop package on native Windows."
labels:
  - "release-engineer"
paths:
  - "packaging/run-rbc.ps1"
  - ".github/workflows/release.yml"
  - "apps/rusty-brick-chess/src-tauri/**"
  - "docs/app/running.md"
---
# Release Engineer — Qualify RBC on native Windows

Start with the Windows quickstart in `docs/app/running.md` and the Windows job in `.github/workflows/release.yml`. Keep source-build and installer results separate. Attach OS version, build identity, and artifact hashes to the PR.

## Done when

- On a native Windows host, run the PowerShell quickstart with the MSVC toolchain. Verify the browser opens and RBC creates a fresh database under Local AppData.
- Independently install and launch the candidate desktop package. Verify the bundled frontend loads, import a PGN, and run one Stockfish analysis.
- Record separate source and installer results with Windows version, architecture, source commit, and installer SHA-256. Link the results in the PR and update the run guide with the verified path.

## Checklist


- [ ] Run the MSVC and PowerShell source quickstart on Windows and confirm launch with a fresh per-user database. (i-55ff)
- [ ] Build and install the candidate desktop package on Windows, then verify bundled assets, PGN import, and a UCI analysis smoke. (i-3bb5)
- [ ] Record the native results and update the running guide with the verified install path. (i-920f)

## M-i-815f · milestone — Signed off by a person
min: signed

- [ ] Signed off by a person (i-815f)
