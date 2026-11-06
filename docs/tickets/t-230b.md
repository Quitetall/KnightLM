---
format: openwarrant/1.0
profile: ticket
alias: t-230b
uuid: 01a1a54a-3154-731a-a896-3ef16a87173e
priority: 1
created_at: "2026-11-04T06:41:05Z"
created_by: "claude"
summary: "Configure OpenAI-compatible providers and keys without exposing secrets."
---
# Provider settings, keyring and local detect

## Checklist

- [x] Provider list/add/edit/test (i-85c9, after t-6584, t-2883) — done by claude, 2026-11-06 [observed]: Schema v12 providers table and the settings panel: list with key state, add, test against the endpoint, fetch models, delete. Routes GET/POST /api/providers, PUT/DELETE /api/providers/{id}.
- [x] Keyring storage with env fallback and key_ref (i-ade0) — done by claude, 2026-11-06 [observed]: A row stores key_ref, the NAME of an env var or keyring:<service>/<account>; resolve_secret resolves it at call time through secret-tool then the environment, and the route reports only whether it resolves and from where.
- [x] Model capability metadata and /v1/models fetch (i-8607) — done by claude, 2026-11-06 [observed]: capabilities_of reads context window, vision and reasoning off the model id, leaving context None for an unrecognised id rather than inventing a window; the models route fetches /v1/models and keeps the cards.
- [x] Localhost scan for Ollama/vLLM/LM Studio (i-48d7) — done by claude, 2026-11-06 [observed]: LOCAL_CANDIDATES probes Ollama 11434, LM Studio 1234, vLLM 8000, llama.cpp 8080 and Jan 1337; the route lists every candidate with whether it answered and its models, and the panel offers a one-click add.
- [x] Tests: no key in DB/log/context (i-068a) — done by claude, 2026-11-06 [observed]: tests/providers.rs asserts a secret in the environment never appears in a response, a provider row or the audit log, and that an unresolved reference is reported as unresolved rather than as a failure.

## CHK-ai-providers · check — ai-providers
run: cd /mnt/4tb/KnightLM/apps/rusty-brick-chess && cargo test --lib providers && cargo test --test providers
on: done
evaluates i-85c9, i-ade0, i-8607, i-48d7, i-068a
