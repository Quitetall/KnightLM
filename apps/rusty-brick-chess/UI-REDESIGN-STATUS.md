# Redesign verification checkpoint

Cutover is **BLOCKED: 59 PASS, 1 UNKNOWN, 0 unresolved product FAIL** out of 60 checklist rows. Checked boxes in [the plan](UI-REDESIGN-PLAN.md) refer to this candidate's named Linux browser checks, not desktop release qualification.

Candidate fingerprint: `671ec47edd738438108f2fa0c342b6969c2295b7f5be32b84b7ae631ae5a9ac6` (server plus served entrypoint/JS/WASM hashes), re-verified after merging `origin/main` (3b137e6) on 2026-11-20.

Local evidence: `/mnt/4tb/artifacts/KnightLM/keep/rbc-ui-parity-verified-20261120-main-merge-ledger/` (receipts: `rbc-ui-parity-verified-20261120-main-merge`, `rbc-main-merge-live-accounts-20261120`, `rbc-main-merge-live-model-20261120`; the 2026-11-13 directory is the previous candidate). Its `candidate-build.json` identifies the individual receipts; `parity-ledger.json` maps all 60 IDs to those receipts. Earlier failed runs are retained. This checkpoint aggregates passing suites for the identical server/assets after test repairs, rather than presenting a failed batch as green.

## Merged with main (2026-11-20)

- Merge of `origin/main` 3b137e6: two conflicts. `src-tauri/src/main.rs` taken from main (per-launch token, single-instance, signal quit; main already keeps native pickers off the UI thread and quits through the predefined item, which is what this branch's earlier fix did). `crates/ai/src/provider/mod.rs` keeps both `cancel_transport` and `knightlm_serve`. `tests/knightlm_serve.rs` (main's) needed the branch's new `AppState.tournament_actions` field.
- The redesigned frontend works with the shell unchanged: it makes only same-origin relative requests, so the `/?rbc_token=` cookie covers it; it listens for `rbc-imported` (`cdp_home_profile`); the shell's `PUT /api/settings` and `POST /api/databases/switch` are handled by the merged server.
- Re-run on the merged tree: `make check`, `make check-apps`, the 56 browser suites, both live suites, the parity validator (59 PASS, 1 UNKNOWN; the full cutover command still exits 1 for LIB-09), `make check-desktop` (fmt, clippy, build, Xvfb smoke: window, token gating, second-launch PGN handoff, quit cleanup). The first `live_model_controls` run failed once with "KnightLM did not load" on a cold model; the rerun passed.

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
