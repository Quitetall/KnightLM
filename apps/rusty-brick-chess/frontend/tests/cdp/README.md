# Assistant browser checks

These drive the real app in headless Chrome against a running server, because
the behaviour they cover — a rail that filters, a session that survives a
reload, a conversation pinned into its own tab — only exists once the wasm app
runs.

Prerequisites:

```bash
cd apps/rusty-brick-chess/frontend && ./deploy.sh
# a server on 127.0.0.1:8090 serving the deployed frontend, e.g.
RBC_DB=/tmp/rbc4.db RBC_ADDR=127.0.0.1:8090 nohup ../../target/debug/rusty-brick-chess &
```

Then, from this directory:

```bash
node cdp_ai_rail.js   # AI_RAIL_PASS: search filters, selection survives a reload
node cdp_ai_rail2.js  # AI_RAIL2_PASS: rename saves, archive removes one row
node cdp_ai_pin.js    # AI_PIN_PASS: pinning opens a tab, re-pinning focuses it
```

Each script starts its own Chrome. Use the isolated runner below for a
fresh browser profile, separate ports, and test-owned cleanup.

Streaming and stop need a model behind the server:

```bash
MOCK_SLOW=0.6 python3 <mock-provider> &   # any OpenAI-compatible SSE server
MODEL_URL=http://127.0.0.1:8100/v1/chat RBC_ADDR=127.0.0.1:8090 <server> &
node cdp_ai_stream.js  # AI_STREAM_PASS: text appears mid-turn, Stop ends it
```

## Redesign baseline and slice verification

From the repository root, build the server with `cargo build --locked` in
the app directory and run `./deploy.sh` in its frontend directory. Use an
isolated worktree because deployment writes `frontend/public`.

Run the isolated browser harness with the built server, deployed assets, and
a new evidence directory under the project artifact store. Omit suite names
to exercise the complete browser suite.

```sh
python3 scripts/check_rbc_browser.py \
  --server apps/rusty-brick-chess/target/debug/rusty-brick-chess \
  --static apps/rusty-brick-chess/frontend/public \
  --evidence /mnt/4tb/artifacts/KnightLM/keep/rbc-ui-baseline-run \
  cdp_session cdp_cross_board
```

The runner assigns separate ports and a fresh database and browser profile
to each suite. It records logs and the hashes of the assets served by the
test app. An individual CDP script accepts `RBC_CDP_URL`, `RBC_CDP_PORT`,
and, for self-hosted provider fixtures, `RBC_CDP_PROVIDER_PORT`. Use the
isolated harness on shared hosts.

The browser results are inputs to the parity ledger, which is a JSON array
with one record per checklist ID. Each record includes `id`, `build`,
`platform`, `input`, `fixture`, `old_path`, `new_path`, `expected`,
`check`, `evidence`, and `status`. A browser suite covering part of a row
does not by itself establish that row. Link the complete supporting evidence.

```sh
python3 scripts/check_rbc_ui_parity.py /path/to/ledger.json --build <build-id> \
  --id RBCUI-SHELL-04 --id RBCUI-SHELL-05
```

Omit `--id` for the cutover gate. Each required record must have PASS
evidence for the identified build. Preserve failures and unknowns in the
ledger until a later run establishes the corresponding behavior.
