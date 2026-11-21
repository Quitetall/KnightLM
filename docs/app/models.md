# Using a model from RustyBrickChess

RustyBrickChess (RBC) reaches a model through one door: an OpenAI-compatible
`/v1/chat/completions` endpoint. It does not import, bundle or run KnightLM code. A
KnightLM checkpoint is one provider among others, such as Ollama, LM Studio, vLLM,
llama.cpp or a hosted API. The contract with a KnightLM server is in
[docs/model/seam.md](../model/seam.md); RBC needs none of it to work.

## Providers

A provider is a row in Settings > Providers (`/api/providers`): a label, a base URL
(`http://127.0.0.1:11434/v1`) and an optional key **reference**.

- A key reference is a name, never a value: an environment variable name
  (`OPENAI_API_KEY`) or `keyring:<service>/<account>`. A value that looks like a key
  is refused with 400, and the refusal does not repeat it. Responses report whether
  the reference resolves, not what it resolves to; no secret reaches the database,
  the audit log or a response.
- **KnightLM server** preset: Settings has an "Add KnightLM server" button, and
  `POST /api/providers {"preset": "knightlm"}` does the same. It fills the label
  `KnightLM server` and the local base URL (`http://127.0.0.1:8000/v1`, or what
  `RBC_KNIGHTLM_SERVE_URL` points at), so chat goes to
  `http://127.0.0.1:8000/v1/chat/completions`, with model name `knightlm`. It is an
  ordinary row; edit or delete it like any other. `GET /api/providers/presets` lists presets.
- **Connection test**: `POST /api/providers/{id}/test` (optional body `{"model": "..."}`)
  or, before saving, `POST /api/providers/probe {"base_url", "key_ref", "model"}`. The
  answer is `{ok, error, models, model_ids, latency_ms}`; on failure `error` is
  `{code, message}` with one of `invalid_url`, `key_unusable`, `key_unresolved`,
  `timeout`, `unreachable`, `unauthorized`, `not_found` (usually a base URL missing
  `/v1`), `server_error`, `bad_response`, `model_missing`.

## Seats and the chat toggle

A match has a white and a black seat; a person or a model (`{"kind": "model",
"provider": "provider:<id>", "model": "<name>"}`) holds each. `provider:<id>` names a
registered provider; the existing `local:<name>` seats keep working. A third seat, the
**coach**, answers chat and coach turns and defaults to the model that is playing.

`GET` and `PUT /api/model-seats` store `{white, black, coach, chat}` as defaults
(a seat naming a provider that is not registered is a 400). A model move or chat
request may carry its own `coach` and `chat`, which win over the stored values.

The chat toggle sits with the play controls on the Play tab.

| Chat | What the model gets | What comes back |
|---|---|---|
| on | a **coach turn**: the position, a rolling summary of the chat and the recent messages, the coach tools | the move (the last legal `Provisional:` line), a reply for the chat, and marks for the board |
| off | the **competition (play) harness**: the position and the play tools, `submit_move` | the move only |

A rated game never chats, whatever the toggle says (`chat_on` in the answer says what
happened). The client keeps the chat and sends `summary` and `history` with
`POST /api/ai/matches/{id}/model-move` and `/chat`; the server rolls them to the
bounds in `rbc_ai::model_turn` (the last messages verbatim, older ones folded into a
summary) and returns the rolled chat in `chat`, to send back next time.

## What a coach turn draws

`model-move` returns `tools.ui`, the structured effects of the coach's calls in order,
and `tools.marks`, the marks on the board when the turn ended. The Play tab draws
arrows and marked squares from `tools.marks`, highlights the ply the coach pointed at
(`show_ply`) in the move list, and shows that position on a small read-only board.
The UI interprets data and never parses model text.

## Tests

- `apps/rusty-brick-chess/tests/model_wiring.rs`: a scripted fake OpenAI server on a
  loopback port, driven through the real router: provider settings, typed errors,
  seats, coach turn versus play harness, marks, rolling chat.
- `apps/rusty-brick-chess/frontend/tests/cdp/cdp_coach_board.js`: the same fake behind
  the real app in headless Chrome; arrows, pointer, chat line and the toggle.
- `board::coach_tests` (frontend crate): mark and pointer decoding.
