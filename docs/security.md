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

## Not covered here

- **MCP authentication, resource traversal and the destructive-action review**
  (`i-23d6`, `i-5896`) have nothing to test yet: there is no MCP server
  (`t-745d`). They are listed as unverified rather than as passing.
- **A destructive tool does not exist in the builtin registry**, so the
  destructive-scope test returns early instead of asserting. When one lands, the
  test starts checking it.
- **Transport security** is out of scope: the server binds loopback and has no
  TLS story. That belongs to a deployment decision, not to this code.

## Reproducing

```bash
cargo test -p rbc-ai --all-features security
```
