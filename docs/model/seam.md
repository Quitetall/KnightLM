# The seam between RBC and KnightLM

RBC (RustyBrickChess) is the open-source chess app. KnightLM is the ML path: corpus,
tokenizer, training, evaluation and serving. They meet at one published wire contract and
share no code beyond it. RBC works with no KnightLM present and with any provider.

## What each side may import

| Side | May import | May not import |
|---|---|---|
| RBC (`apps/rusty-brick-chess`, `crates/ai` provider layer) | `knightlm-contract` | `model/`, `apps/blut-knightlm`, any training or serving code |
| KnightLM (`model/`, `apps/blut-knightlm`) | `knightlm-contract`, the rules core (`crates/notation`, `crates/core`) | anything from `apps/rusty-brick-chess` |

`crates/contract` (`knightlm-contract`) holds only the contract:

- `ContextPacket` and `PACKET_FORMAT_VERSION`, re-exported from `crates/core`;
- `schemas/model_tools.json` (tool schemas by mode), `model_prompts.json`,
  `play_completion_cases.json`, `chat_summary_cases.json` (goldens both languages must
  reproduce byte for byte) and `chat_completion_example.json`;
- `openai`: serde types for the chat-completions request, response and error object.

Rust generates and checks the goldens (`UPDATE_MODEL_TOOLS=1 cargo test -p rbc-ai --features
core model_` regenerates them); `model/prompt.py` reads the same files.

## Wire formats

Position packet (JSON, versioned): the canonical `ContextPacket`. `schema_version` is its
version (1 today; 2 only for the labelled effects arm). A server refuses a version it does not serve.

`POST /v1/chat/completions` (OpenAI compatible, play mode):

- Request: `model` (`knightlm`), `messages`, optional `tools`, `tool_choice`, `temperature`,
  `max_tokens`, `stream`, `stream_options`. The packet is the last user message: either the
  play prompt (`...Position packet:\n<json>`), bare packet JSON, or `{"packet": {...}}`.
  Sampling fields are accepted and ignored (decoding is fixed). `?mode=raw|constrained` or
  `"knightlm": {"mode": ...}` selects raw (never repaired, an illegal reply is a 200 record)
  or constrained (a legal move is guaranteed; an unrecoverable refusal is HTTP 502).
- Tools are fixed by the server (`submit_move`). A request naming any other tool, a coach
  tool or an app tool such as `analyze`, `evaluate` or `play_move`, gets HTTP 400 with an
  OpenAI error object (`code: tool_not_allowed`).
- Response: `chat.completion` with `choices[0].message` role `assistant`, `content` null and
  `tool_calls[0]` = `submit_move` with `arguments` a JSON string `{"reasoning", "move"}`,
  `finish_reason` `tool_calls`, `usage`. An extra `knightlm` object reports `path`
  (`model`, `constrained_move`, `constrained_forced_reasoning`, `none`), `code`, `raw_code`,
  `legal` and `manifest_hash`; a constrained fallback is never unlabelled. A raw refusal
  returns the reply as `content`, `finish_reason` `stop`, `legal: false`.
- `stream: true`: `chat.completion.chunk` SSE events (role, tool call head, argument
  fragments, final chunk with `finish_reason` and `knightlm`, optional usage chunk), then `data: [DONE]`.
- Errors: `{"error": {"message", "type", "param", "code"}}`.

`GET /v1/models` lists `knightlm`. `POST /v1/move` and `POST /v1/play` are unchanged
conveniences for `knightlm-uci`, `knightlm-match` and the evaluator.

RBC reaches this endpoint with its existing OpenAI-compatible provider
(`crates/ai/src/provider/openai.rs`, `ProviderConfig::openai_compatible`); any other
OpenAI-compatible server works the same. `provider/knightlm_serve.rs` still serves the
`/v1/move` seat adapter.

## Versioning

- `CONTRACT_VERSION` (in `knightlm-contract`) changes when a type or golden changes incompatibly.
- Additive response fields are allowed: the response types ignore unknown fields, except
  the `knightlm` extension, which is ours and refuses them. Request types are ours and refuse them.
- A change to a golden is a change to the contract: regenerate in Rust, and the Python
  parity tests (`model.tests.test_prompt`, `model.serve.test_chat`) must pass unchanged or change with it.
