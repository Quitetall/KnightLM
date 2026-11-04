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

Kill stale browsers first with `pkill -9 -x chrome`; each script starts its own
Chrome on its own debugging port.
