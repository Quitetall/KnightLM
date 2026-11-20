---
format: openwarrant/1.0
profile: ticket
alias: t-a004
uuid: 01a1a54a-30c5-75b4-b2e8-a8cec3d3eb39
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Prove byte/hash stability across runs, platforms and input key order."
---
# Context determinism and property tests

## Checklist

- [x] 100× compile stability (i-4753, after t-15fc, t-6abc, t-dfbc, t-bf2a, t-bc1a) — done by claude, 2026-11-04
- [x] Native/wasm agreement (i-9904) — done by claude, 2026-11-04
- [x] Input key-order stability (i-bc66) — done by claude, 2026-11-04
- [x] FEN parse/render idempotence property test (i-10af) — done by claude, 2026-11-04
- [x] Evidence fingerprint regression fixtures (i-9aad) — done by claude, 2026-11-04
