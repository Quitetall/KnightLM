# Redesign verification checkpoint

Cutover is **BLOCKED: 59 PASS, 1 UNKNOWN, 0 unresolved product FAIL** out of 60 checklist rows. Checked boxes in [the plan](UI-REDESIGN-PLAN.md) refer to this candidate's named Linux browser checks, not desktop release qualification.

Candidate fingerprint: `793653912b2c58b01c6cad9d2dd5dcea58887966620978e806505a78eaf23558` (server plus served entrypoint/JS/WASM hashes).

Local evidence: `/mnt/4tb/artifacts/KnightLM/keep/rbc-ui-parity-verified-20261112-final/`. Its `candidate-build.json` identifies the individual receipts; `parity-ledger.json` maps all 60 IDs to those receipts. Earlier failed runs are retained. This checkpoint aggregates passing suites for the identical server/assets after test repairs, rather than presenting a failed batch as green.

## Verified

- All 55 normal browser suites have passing receipts. Tournament Step creates and advances real stored matches: double round robin, Swiss, knockout, pause/resume, standings and PGN movetext.
- Two explicit live suites pass: real private KnightLM load/direct/thinking/voting/unload, and public Lichess/Chess.com account linking/import/reload/unlink. The private LAMU runtime was verified as test-owned and unloaded; existing Ollama discovery was read-only.
- App fmt, server/frontend clippy and tests pass. AI host-feature tests (including silent-stream cancellation and body deadline preservation), DB tests, AI host clippy, native build/clippy and the parity validator's negative tests pass.
- Fixed stale Analysis dock replies; saved Sent context, old transcript fallback and replay against the current board; stalled provider cancellation; Home refresh after native imports; New board readiness during workspace creation; explorer stale output; keyboard focus/promotion trap. Replaced sleep-based rail/cross-board checks with state-based waits, and waited for stream completion before the scope-revocation follow-up.
- Linux native runtime checks found and fixed UI-thread picker deadlock, database extension/stem mismatch and disabled Quit. A duplicate native database import refuses overwrite. These are partial unbundled Linux observations, not a native release PASS.

## Fixed and verified in this checkpoint

- RBCUI-CFG-04: custom sound upload and playback now share one name (the server accepts a name with or without `.ogg`; already-stored stems keep working). 16 bundled files that were symlink or 404 placeholders were replaced with the real upstream Lichess files (see `frontend/assets/sounds/COPYING.txt`), with an integrity test. `cdp_sound` passes on a PulseAudio null sink.
- RBCUI-TRAIN-02: Lichess now requires auth on the explorer. The linked account token is sent as `Authorization: Bearer`; without it a 401 is reported as an actionable message instead of a silent local fallback.
- RBCUI-LIB-04: the app no longer persists a raw Lichess token. Settings hold `lichess_token_ref`, the NAME of an environment variable or a `keyring:<service>/<account>` entry, resolved at call time through `providers::resolve_secret` (the AI provider `key_ref` pattern). The link form refuses a pasted token (without echoing it) and stores nothing unless the reference resolves. A database from an older build with a plaintext `lichess_token` is migrated at startup: the value is written with `secret-tool store`, read back, and only then is the plaintext row deleted; if that fails the row is kept, a warning (never the value) is printed, and the old value keeps working until re-linked. The live suite types `LICHESS_TOKEN` (the name), not the value; evidence was scanned for the token value and holds none.

## Remaining required coverage

| ID | Why it remains UNKNOWN | What settles it |
| --- | --- | --- |
| RBCUI-LIB-09 | Linux unbundled native menus/pickers were exercised; installed resource identity/full handoffs and macOS/Windows were not. | Run the packaged-host scenarios on each released platform, against these candidate assets. |

No missing qualification is waived. The full cutover command must fail until LIB-09 has real passing evidence.
