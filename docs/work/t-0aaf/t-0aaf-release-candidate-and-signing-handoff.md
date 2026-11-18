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

- [x] Release build and install (i-fcca, after t-1b87) — done by claude, 2026-11-06 [observed]: cargo build --release produced a 60.9 MB binary at commit 2bb5d1d; packaging/install.sh --no-desktop installed it with its assets to ~/.local/bin/rusty-brick-chess and ~/.local/share/rustybrickchess/frontend. The install is current (03:12 today), not the 31 October build that was there before.
- [x] RC smoke test (i-c106) — done by claude, 2026-11-06 [observed]: Smoke test against the INSTALLED binary on a fresh database, port 8095: health 200 schema 12, frontend index 200 (36,807 bytes) and wasm 200 (9,960,363 bytes), tables studies/ai_turns/ai_audit/providers present, study+chapter created, providers scan 200, compare 200, and MCP POST /mcp without a token 401.
- [x] Evidence bundle (i-4f9e) — done by claude, 2026-11-06 [observed]: docs/release-candidate.md carries the build identity (commit, binary and bundle sha256, schema 12) and the evidence tables: workspace tests green on both feature profiles, fmt and clippy clean, frontend 10 tests, browser suites 16 of 17 with the red one diagnosed in t-4158.
- [x] Signed/updater handoff (i-7638) — done by claude, 2026-11-06 [observed]: The handoff is written and honest: nothing in it is signed by an agent. Binary signing is not implemented in this project - no updater, no signature over the artifact - so that is new work rather than a handoff. What a person signs is listed: the SAS revision, each Warrant plan and close-out, and the obligations via an independent verifier.
- [x] Release notes (i-772d) — done by claude, 2026-11-06 [observed]: docs/release-notes-1.0.md: what the release does (assistant, play, tournaments, studies, compare, MCP), the decisions worth knowing (a key is a name not a value; a refusal is recorded; the gate), the measured numbers, and the known limits including the 1.3 s explorer.

## CHK-rc · check — rc
run: cd /mnt/4tb/KnightLM && cargo build --release
on: done
evaluates i-fcca, i-c106, i-4f9e, i-7638, i-772d
