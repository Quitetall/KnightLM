# Security review

What follows is the review behind `t-69c9`, with the findings and the tests that
hold them. It is deliberately specific: a claim without a test is listed as
unverified rather than as done.

## Credentials

**No credential is persisted.** An API key lives in `ProviderConfig.api_key` in
memory, is used only to build the `Authorization` header, and is never written
through `set_setting`, never stored in a table, and never placed in a stored
turn. Audited by reading every `set_setting` call site and every table that
could hold one; the only `token` in `rbc-db` is an SAN token in
`update_moves`.

**A key does not leak through errors.** The provider's error type carries a
code, a message, a retryable flag and the provider's own code — never the key.
Asserted in `the_key_never_appears_in_an_error`, including the `Debug` of the
error, because that is what a log actually writes.

**The header is the only place a key appears.** `the_key_travels_in_the_authorization_header_and_not_in_the_body`
drives a real HTTP request at the in-process mock server and asserts the header
is `Bearer <key>` and the JSON body does not contain it.

Human accounts (Lichess, chess.com) take a token per request and report only
`has_token` back; the token itself is not stored.

## SQL

Every statement that interpolates does so with a **fixed** fragment: table names
come from literal lists (`["positions", "annotations", ...]`), and the WHERE
clauses in the game query are built from constant strings with values bound as
`?` parameters. No user text reaches a statement by concatenation. Verified by
reading every `format!("SELECT|INSERT|UPDATE|DELETE ...")` site in `rbc-db`.

## Policy and approvals

- **A denied tool is not called.** The decision is available before the call, so
  a refusal is a refusal and not a call whose result was discarded. The test
  keeps a host that counts calls and asserts the count stays at zero.
- **Mode gating is what keeps structural writes out of a game.** `write_note` is
  a *board* write and is legitimately allowed during Play; the tools the policy
  refuses there are the structural study writes (`create_study`,
  `add_chapter`). This corrected a wrong assumption on my part and is worth
  stating plainly, because "study writes are blocked" reads as if all writing is.
- **A board write in Chat needs approval**, and only a granted `board-write`
  scope changes that.
- **An approval is answered once.** Covered by `crates/ai/tests/approvals.rs`:
  a second answer, an expired request and an unknown id are all refused.
- **A refused write leaves the board alone**, which
  `crates/ai/tests/write_tools.rs` asserts on a stale revision.

## Model output

A model's reply is validated before it is recorded: an illegal move is retried
with the reason and then fails the turn with the board untouched, so a model
cannot talk its way onto the board. `a_model_reply_that_is_not_a_legal_move_is_never_recorded`.

## MCP

The server landed after this review began, so these two items are now
demonstrated rather than deferred.

**Authentication.** `POST /mcp` compares the bearer token before it dispatches
anything, and answers 401 without a body so a caller without the token learns
nothing else. The comparison accumulates differences across the whole string:
a near miss is not a timing signal about how much matched. A missing header, an
empty value, a scheme with no token, a short token and a long token are each
refused (`a_token_must_match_exactly`). stdio needs no token because it inherits
process trust — the parent process already runs as this user with this database.

**Resource traversal.** `rbc://` uris are matched against a fixed list of shapes
(`workspace/{id}`, `board/{id}`, `board/{id}/history`, `study/{id}`,
`study/{id}/chapter/{c}`, `match/{id}`, `match/{id}/pgn`). Anything else resolves
to nothing, so `rbc://../../etc/passwd`, `rbc://board/../workspaces`,
`file:///etc/passwd` and a bare `/etc/passwd` are all refused
(`a_resource_path_cannot_escape_into_another_resource`), while a known uri still
reads (`a_known_resource_still_reads`) — a wall that also blocked real uris would
be its own bug.

**The policy applies to MCP calls.** This was a real hole: `tools/call` called
the host directly, so an MCP client bypassed the policy entirely. It now goes
through `decide()` with `Origin::Mcp`; a write without a grant returns
`approval_required` with a changeset summary and runs nothing.

**Rate limit.** 240 requests a minute per connection, measured: 240 answered,
then 429s.

## Destructive actions

Reviewed every path that changes stored content, since the design says
destructive actions always prompt:

| Path | Guard |
|---|---|
| board write (in-app) | policy → approval, then revision check in the repository |
| board write (MCP) | policy → `approval_required`, nothing runs until a person approves |
| study content | `StudyContent` scope, never granted to MCP by default |
| board delete/overwrite | immutability: a locked or published board refuses and offers a copy |
| approval answer | one answer per request; a second is refused, expiry closes the rest |

No tool in the registry is classified `Destructive` yet, so
`a_destructive_tool_without_the_scope_is_refused` returns early. That is
stated rather than counted as a pass, and the test starts asserting the moment
such a tool is added.
- **Transport security** is out of scope: the server binds loopback and has no
  TLS story. That belongs to a deployment decision, not to this code.

## Reproducing

```bash
cargo test -p rbc-ai --all-features security
```
