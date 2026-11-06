# Release candidate — 1.0.0

The evidence this candidate rests on, and what a person still has to sign.

## Build identity

| | |
|---|---|
| commit | `ec50e82` |
| built | 2026-11-06T08:12:47Z |
| binary | `~/.local/bin/rusty-brick-chess` |
| binary sha256 | `e21dcb63489cc71679cbf339da50b4eea5ac7845ae194191bc0b3c6fae654af6` |
| frontend bundle sha256 | `7d7985f1fa70b741cb9178e5b866eebb49af36aac4ac06b5c178f752c53e5182` |
| schema version | 12 |

```bash
cargo build --release --manifest-path apps/rusty-brick-chess/Cargo.toml
apps/rusty-brick-chess/frontend/deploy.sh
./packaging/install.sh --no-desktop
```

## Smoke test

Against the installed binary on a fresh database (`RBC_DB=/tmp/rc-smoke/rc.db`,
port 8095), not against the development build:

| check | result |
|---|---|
| `/api/health` | 200, `schema: 12` |
| frontend `/` | 200, 36,807 bytes |
| frontend wasm | 200, 9,960,363 bytes |
| tables present | `studies`, `ai_turns`, `ai_audit`, `providers` |
| create a study and a chapter | 200, 1 study listed |
| `/api/providers/scan` | 200 |
| `/api/compare?left=X&right=X` | 200 |
| MCP `POST /mcp` with no token | **401** |

## Test evidence

| gate | result |
|---|---|
| `cargo test --workspace` | green, 60 test binaries |
| `cargo test --workspace --all-features` | green, 60 test binaries |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean |
| frontend crate tests | 10 pass |
| browser suites | **16 of 17** pass |

The one red is `cdp_ai_approvals`, and the reason is written down in
`docs/tickets/t-4158.md`: the approval is queued and the API serves it, but the
panel does not refresh. It is a frontend defect, not a queue defect.

## Known limits at this commit

- The explorer takes 1.3 s against a million games (`docs/benchmarks.md`).
- The approvals panel does not reload after a turn queues a request.
- No Tauri shell is built; the app is a server plus a static frontend.
- The build machine's 4 TB volume was in an I/O failure state throughout, so the
  large benchmarks ran on a tmpfs and their import rates are optimistic.

## What a person signs

Nothing here is signed by an agent, and nothing here can be:

1. **The spec revision.** The Warrant corpus pins SAS 0.1.0, accepted by
   QuiteTall on 2026-10-12; the document on disk is 0.2.1, proposed. The 0.1.0
   bytes are not in any commit, so acceptance cannot be re-established — accept
   0.2.1 (`docs/authority/SAS-0.2.1.acceptance-request.toml`, an ADR required) or
   restore 0.1.0 from a backup.
2. **Each Warrant's plan** (`war sign authorize <alias>`) and its close-out
   (`war sign resolve <alias>`).
3. **The obligations**, by an independent verifier: `war evidence verify <alias>`.
4. **Binary signing** is not implemented in this project. There is no updater and
   no signature over the artifact; `packaging/install.sh` copies the binary and
   its assets into a prefix. If a signed artifact is required, that is new work,
   not a handoff.
