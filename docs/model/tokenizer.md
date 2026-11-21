# Chess tokenizer (`ctok/1`)

Status: specification and Python reference library (`model/tokenizer/`). Nothing
trains on it and no prompt, corpus or evaluation path uses it yet; wiring it into
prompts is the follow-up for arms T1 and T3 of the approved plan. `model/prompt.py`
and its goldens are unchanged.

## Why

Run 1 showed that a 500M text model reading FEN cannot ground board claims (65% of
its piece-on-square claims were false). With the stock tokenizer a square is
"about one token mixed with digits and slashes", a UCI move is four tokens
(`e2e4` is `e`,`2`,`e`,`4`), and the packet costs about 480 tokens. `ctok/1` gives the
model a vocabulary made of chess objects: one token per board square content, one
token per move, one token per Chess Graph Text (CGT) structural item. Two arms use it:

- **T1 (board-native tokens):** the position as 64 fixed-order piece tokens plus
  state tokens and, optionally, the legal-move list as move tokens.
- **T3 (CGT as native tokens):** the `cgt/1` document of `docs/model/graph-text.md`
  tokenized with node, relation and section tokens instead of stock sub-words.

Both are views of the packet, not replacements (`docs/model/seam.md`): the packet
stays the wire format and the compiler stays the rules authority.

## Invariants it keeps

- **One state.** A sequence encodes one position record: FEN fields, the packet's
  repetition count, an optional terminal reason and an optional legal-move list.
  No history, no last move, no engine output.
- **Unknown stays unknown.** A four-field FEN is refused, never given invented
  clocks; an unknown repetition count is the token `<rep:?>`.
- **Lossless on what it carries.** `decode(encode(x)) == x` exactly, for every
  position in the tests. The packet's derived facts (`in_check`, `checkmate`, the
  ledger map) are not carried by T1; they are recomputable from the board and
  belong to the compiler.
- **No chess in the tokenizer.** The library never decides legality. Legal moves
  and CGT text come from the compiler (`knightlm playouts`, `knightlm cgt`);
  the library only maps them to ids and refuses malformed sequences. The one
  enumeration it does (the move space) is geometry, justified below.
- **One id per object.** Every legal move has exactly one token; two different
  records never encode to the same ids.

## Vocabulary

Local ids run `0..2495` (`SIZE = 2496`, a multiple of 64). A host model adds a
`base` offset. The list order below is the id order. The SHA-256 of the
newline-joined token strings is the vocabulary identity.

| range | ids | count | tokens | used by |
|---|---|---|---|---|
| structure | 0-10 | 11 | `<pos> </pos> <cgt/1> </cgt> <nl> <dash> <board> <moves> </moves> <v:lite> <v:full>` | T1, T3 |
| call | 11-15 | 5 | `<call:submit_move> </call> <reasoning> </reasoning> <arg:move>` | play call |
| side | 16-17 | 2 | `<side:w> <side:b>` | T1, T3 |
| castle | 18-22 | 5 | `<cr:K> <cr:Q> <cr:k> <cr:q> <cr:none>` | T1, T3 |
| square | 23-86 | 64 | `<sq:a1>` .. `<sq:h8>` (index = 8*rank + file) | en passant, CGT nodes, destinations |
| piece | 87-99 | 13 | `<pc:K> <pc:Q> <pc:R> <pc:B> <pc:N> <pc:P>` (white), `<pc:k>`..`<pc:p>` (black), `<pc:.>` (empty) | T1 board, CGT nodes |
| halfmove | 100-299 | 200 | `<hm:0>` .. `<hm:199>` exact halfmove clock | T1, T3 |
| digit | 300-309 | 10 | `<d:0>` .. `<d:9>`, the fullmove number in decimal | T1, T3 |
| rep | 310-320 | 11 | `<rep:0>` .. `<rep:9>`, `<rep:?>` | T1, T3 |
| term | 321-328 | 8 | `<term:checkmate>` ... `<term:draw>`, the CGT `term` reasons | T1, T3 |
| cgt key | 329-343 | 15 | `<k:turn> <k:castle> <k:ep> <k:clock> <k:rep> <k:term> <k:W> <k:B> <k:moves> <k:atk> <k:def> <k:pin> <k:hang> <k:check> <k:ksafe>` | T3 |
| cgt group | 344-346 | 3 | `<g:open> <g:hit> <g:own>` (ksafe groups) | T3 |
| cgt move mark | 347-356 | 10 | `<mv:x> <mv:ep> <mv:=Q> <mv:=R> <mv:=B> <mv:=N> <mv:+> <mv:#> <mv:O-O> <mv:O-O-O>` | T3 |
| move | 357-2324 | 1968 | `<m:a1a2>` ... `<m:h7h8n>` | T1 move list, play call |
| reserved | 2325-2495 | 171 | `<reserved:000>` ... | future (`cgt/2`, coach tools, mode markers) |

Reserved is at least 128 tokens, padded so the total is a multiple of 64 (a
GPU-friendly embedding size). Using a reserved token is a new spec version.

### The move space (derived, not guessed)

A UCI move is a from-square, a to-square and an optional promotion letter. Every
non-promotion move of every piece in standard chess is either a knight jump or
lies on a queen line (rook or bishop ray) from its from-square: king steps, pawn
pushes and captures, double pushes, en passant and the castling king move
(`e1g1` is a two-square step along a rank) are all queen-line pairs. Counting
from-to pairs on an empty board:

- queen lines: 1456 ordered pairs;
- knight jumps: 336 ordered pairs (the two sets do not overlap);
- promotions: a pawn goes from rank 7 to rank 8 (white) or rank 2 to rank 1
  (black) by a push or a capture, 22 from-to pairs per colour, each with
  `q r b n`: 2 x 22 x 4 = 176 tokens. The same from-to pair without a letter is
  already a queen-line token (for another piece), so a promotion always carries
  its letter, as UCI writes it.

Total 1456 + 336 + 176 = **1968**. The set is exact for standard chess: every
token is a move some legal position allows, and no legal move is outside it. The
tests check the second half against every legal move the compiler produces for
thousands of positions (`knightlm playouts`, the CGT fixtures and the Run 1
holdout); the first half is by construction (each pair is reachable by a queen,
knight or promoting pawn on an otherwise legal position).

Order is by from-square index, to-square index, then promotion (none, q, r, b,
n). Move tokens are written `<m:uci>`. **Castling is the king's UCI move**
(`e1g1`, `e1c1`, `e8g8`, `e8c8`) and uses the same token as a king step of that
size would; the position disambiguates. Chess960 castling written as
"king takes own rook" also falls on a queen line and has a token, but Chess960 is
**not claimed** (below).

## T1: board-native sequence

```
<pos> <side:w|b> <cr:..>+ ep <hm:N> digit+ rep [<term:..>] <board> p0..p63 [<moves> m* </moves>] </pos>
```

- `<cr:..>+` is the FEN castling rights in the order K Q k q, or `<cr:none>`.
- `ep` is `<dash>` or one `<sq:..>` (the rank must fit the side to move, 6 for
  white and 3 for black, else the encoder refuses).
- `<hm:N>` is the exact halfmove clock (0..199; the 75-move rule fires at 150).
- `digit+` is the fullmove number, 1 to 5 decimal digits, no leading zero.
- `rep` is `<rep:0..9>` or `<rep:?>`; `<term:..>` appears only if the packet has a
  terminal reason.
- The 64 board tokens run in FEN order (a8, b8, .., h8, a7, .., h1), each one of
  the 13 piece tokens. **Square `k` is the token at offset `k` after `<board>`;**
  the square is implied by position, not spelled. This is the whole point of
  arm T1 (arm B in the plan): "what is on e4" becomes "which token sits at a
  fixed distance from `<board>`" instead of an arithmetic puzzle over digits.
- The move list, when present, is the packet's `legal_moves` as move tokens in
  packet order (ascending UCI string). The decoder refuses duplicates and
  out-of-order lists. Omitting `<moves>` is distinct from an empty list.

Exact clocks, not buckets. The plan named "clock buckets"; buckets are lossy and
`ctok` promises an exact round trip, so the halfmove clock is 200 exact tokens
and the fullmove number is decimal digits. A bucketed view (for example
"moves until the 50-move claim, in fives") is a model-side input feature, not a
tokenizer fact, and is not shipped.

Square tokens `<sq:..>` exist for en passant and for T3 nodes. An interleaved
board layout (`<sq:e4> <pc:N>` for every occupied square, about 2 x pieces
tokens) is a known alternative to the 64 positional tokens; it is **not shipped
and not claimed**. It is shorter for sparse endings and spells every square, at
the cost of a variable-length board.

## T3: CGT as native tokens

The input text is exactly the compiler's `cgt/1` rendering (`knightlm cgt`,
`crates/notation/src/cgt.rs`). `encode` lexes it line by line:

- header: `<cgt/1> <v:lite|full>`; footer `</cgt>`; one `<nl>` between lines.
- state lines: a key token, then values: `turn w` is `<k:turn> <side:w>`;
  `clock 3 3` is `<k:clock> <hm:3> <d:3>`; `rep ?` is `<k:rep> <rep:?>`.
- board lines: `W:` and `B:` are `<k:W>` / `<k:B>` then nodes. **A node `Nb1` is
  two tokens `<pc:N> <sq:b1>`.**
- `moves:`, `atk:`, `def:`, `pin:` are `<k:..>` headers; each following line is
  a source node then its items. The `:` after the source node and the single
  spaces between items are implied and restored by `decode`.
- a move destination is `[<mv:x>] <sq:..> [<mv:ep>] [<mv:=Q>] [<mv:+>|<mv:#>]`;
  castling is `<mv:O-O>` or `<mv:O-O-O>` with an optional check mark.
- `hang:` and `check:` are `<k:..>` then `<dash>` or nodes; `ksafe` lines are a
  node then `<g:open|hit|own>` groups of squares.

CGT never uses the 1968 move tokens: its destinations are per-piece and short
(origin named once), and its edge items are nodes. Move tokens belong to T1's list
and to the play call. Mixed arms (T3 plus the move list) are a prompt-layout
choice, not a vocabulary one.

Strictness. `encode` accepts only canonical text and checks it by decoding its own
output; `decode` accepts only the encoding of one canonical document. Both
refuse: an unknown id or a foreign token, a truncated or extended sequence, a
missing, repeated or reordered line, a lite header over a full body or the
reverse, castling in file letters (Chess960), and node or destination lists that
break the canonical order of `graph-text.md` (white before black, K Q R B N P,
file then rank; destinations by file, rank, promotion, castling last). These are
ordering and grammar checks. **A well-formed lie is not caught**: swapping one
destination square for another gives a valid-looking document. Whether the facts
are true is `parse_cgt` in the compiler; use it on anything a model generated.

## Play call

```
<call:submit_move> <reasoning> {host token ids of the text} </reasoning> <arg:move> <m:uci> </call>
```

This is the token form of the completion `parse_play_completion` reads: reasoning
first, then one move, one call and nothing else. The reasoning stays host text
(stock sub-words) so the existing rules-derived reasoning corpus and its claim
checks apply unchanged; only the move becomes a single token. `play.build_call`
and `play.split_call` build and take apart that frame (reasoning must be non-empty
and contain no ctok id); the text form stays `model.prompt.format_play_completion`.
A single-token move means constrained decoding is a mask over at most a few dozen
ids instead of a trie over four stock tokens. The coach mode and the read tools
(`attackers_of` and the others) have no tokens yet; they take reserved ids when
those arms start.

## Composition with a pretrained text tokenizer

Two ways to host the vocabulary; the library supports the first and the spec
leaves the second to its own PR.

**A. Extend the host (default for T1 and T3 on SmolVLM).** Append the 2496 tokens
after the host's last id: ctok id `i` is host id `base + i`. The reasoning, the
instruction line and any prose stay stock tokens; the position and the move are
ctok tokens. One model reads both. Cost: new embedding and output rows that start
untrained (below), and a tokenizer whose `<pos>`-style strings must never appear in
ordinary text. They are added as non-special, non-normalized tokens so they
survive `decode(skip_special_tokens=True)`; a user message that literally contains
`<m:e2e4>` would tokenize to the move token, so a server must escape or refuse
such text (not built).

**B. Separate small model from scratch.** A tokenizer made only of ctok plus a
byte-level or small word-piece fallback for reasoning, with a 20 to 100M model
trained from scratch. Benefit: no host baggage, every token meaningful, the board
can have its own position scheme. Cost: no pretrained language (explanations must be
templated or learned from our corpus alone), no retention mix, and an arm that
differs from the control in model, tokenizer and data at once. It would not be a
matched-budget comparison with Run 1. Recommended only after the extension arm
shows the vocabulary helps.

Because T1 and T3 change the input only, A is the matched arm: same base, same
rows, same steps, same LoRA recipe, new input embedding.

## Embedding initialisation (`hf.init_new_rows`)

New rows must not start at random: a random 960-d row for `e4` carries no hint that
it relates to the stock "e4". Each new row is initialised from the host's own rows,
for `embed_tokens` and, because SmolVLM-500M's `tie_word_embeddings` is false, for
`lm_head` separately (each from its own matrix):

| token | init |
|---|---|
| square `<sq:e4>` | mean of the host rows of the sub-tokens of `"e4"` |
| piece `<pc:N>` | mean of the sub-tokens of `"white knight"`; empty is `"empty"` |
| digit, halfmove, rep | the number as text; `<rep:?>` is `"unknown"` |
| move `<m:e2e4>` | `0.6 * mean("e2") + 0.4 * mean("e4")`; a promotion is averaged with the host row of the piece word |
| keys, groups, marks, structure | the matching word (`turn`, `open`, `captures`, `end call`, ...) |
| reserved | the host mean row plus seeded noise, 0.02 of the host column std |

The move recipe is the plan's "from plus to" with an asymmetric weight so `e2e4`
and `e4e2` start different. All of this is a prior, deterministic for a given host
and seed. **It is a starting point, not meaning**: the arm only helps if training
moves these rows, and the init says nothing about whether the model will use
them. The first training check is not strength but whether the board tokens
carry the position (a claim probe on a frozen model with new rows only).

## PEFT and what must be trained

For a LoRA run on SmolVLM the new rows must be trainable and saved, or the adapter
cannot read its own input:

- `target_modules` as in the existing recipe (attention and MLP projections of the
  text model).
- **Chosen path: `trainable_token_indices`** for `embed_tokens` and the untied `lm_head`
  (`model/train/ctok.py`, `LoraConfig(trainable_token_indices={"embed_tokens": ids, "lm_head": ids})`
  with `ids = range(base, base + 2496)`). PEFT trains and stores only a delta for those rows over the
  frozen matrix: `2496 x 960` = 2.40M parameters per matrix, 4.79M for the pair, and the host rows cannot
  move. The alternative, `modules_to_save=["embed_tokens", "lm_head"]`, trains and saves full copies of
  both matrices: `(49280 + 2496) x 960` = 49.7M per matrix, 99.4M for the pair, with adapter size and
  optimizer state to match, and the host rows can drift. The CPU reload test (below) shows identical
  logits for the delta path, so the 20x smaller route is the one used. The output rows are trained too
  because the T1 and T3 completions never contain a ctok token: left at their text initialisation the new
  `lm_head` rows would sit next to the host rows they were built from and compete with them at decode
  time; training lets the model push them down. A serving layer should still mask the ctok ids when it
  samples. (`--ctok-rows embed` trains the input rows only.)
- Blocker for a GPU run: the pinned BLUT image-text trainer builds its `LoraConfig` without
  `trainable_token_indices`, rejects `spec.extra`, and the recipe pins the stock base weights. A run needs
  a cookbook change (pass `trainable_token_indices` and accept the extended base snapshot built by
  `python -m model.train.ctok build-base`), so `model.train.run --tokenizer ctok1` is preflight-only and
  refuses `--gpu`.
- Resize on load: the base checkpoint has `vocab_size = 49280`. Before loading the
  adapter, run `resize_token_embeddings(len(tokenizer))` (both matrices grow to
  `base + 2496`); the adapter's saved matrices then replace them. The reload test
  shows identical logits. `config.vocab_size` and `text_config.vocab_size` both
  change; save the extended tokenizer next to the adapter and verify its
  manifest hash on load.
- What trains: LoRA weights, the new rows of `embed_tokens` and `lm_head`
  (tokens in the reserved range and unused `halfmove` values get no useful gradient),
  nothing else. The vision tower and connector are untouched and unused for T1
  and T3.
- What does not: the host rows. With `trainable_token_indices` they are frozen by construction
  (the reload test checks them byte for byte after a perturbed "training" step).

### CPU reload test

`model/tokenizer/test_hf.py` builds a tiny random-weight `Idefics3` (same classes and
untied head as SmolVLM, hidden size 32, two layers), a small trained BPE
tokenizer, then: appends the vocabulary and checks that ctok strings tokenize to
exactly `base + local id` and decode back; resizes both matrices and checks that old
rows are untouched and new rows equal the stated means; saves and reloads model
and tokenizer and requires identical state and logits; and, with `peft` installed,
wraps in LoRA with `modules_to_save`, perturbs the trainable rows to stand in for
training, saves only the adapter, reloads it on a freshly resized base and
compares logits (`allclose`, 1e-5). When the SmolVLM snapshot is present it also
checks the real tokenizer lands at `base = 49280`. No GPU, no training run.
`peft` is not installed in the base environment; the CI job that runs the model
tests needs the `model/train/requirements.txt` pins for that test to run.

## Manifest

`python -m model.tokenizer manifest` prints, and token-cost evidence stores,

```json
{"format": "ctok-manifest/1", "spec_version": "ctok/1", "cgt_version": "cgt/1",
 "packet_schema_version": 1, "rules_revision": "standard-untimed-v1;shakmaty-0.30.2;cgt/1",
 "vocab_size": 2496, "vocab_sha256": "7f24af530c7525d79bcf0ac02ce60401cfa7f052b56a98fd5b9bb7c13178313b",
 "ranges": {"square": {"start": 23, "end": 87}, "...": "..."}}
```

`rules_revision` names the ruleset, the shakmaty release in the compiler and the
CGT version; a test fails if it disagrees with `Cargo.lock`. Any change to the
vocabulary list, the move space, the sequence grammar or one of those revisions is
a new spec version (`ctok/2`) and a new hash: a checkpoint trained on one must record
the manifest and refuse another.

## Token cost

First 2,000 positions of `holdout-00000.jsonl` in `run1-reasoning-2026-09-a`,
stock SmolVLM tokenizer (`smolvlm-500m-a7da5b98`) against ctok ids. Evidence:
`/mnt/4tb/artifacts/KnightLM/keep/ctok-token-cost-2026-11/` (`summary.json`,
`manifest.json`, `per_position.jsonl`). Reproduce with
`python -m model.tokenizer.token_cost --corpus <dir> --tokenizer <dir> --out <dir>`.
Every counted sequence round-trips.

| form | tokenizer | mean | p50 | p95 | max | vs stock FEN packet |
|---|---|---|---|---|---|---|
| FEN packet (instruction line excluded) | stock | 474.8 | 456 | 650 | 929 | 1.00 |
| bare FEN | stock | 55.6 | 56 | 62 | 69 | 0.12 |
| cgt-lite | stock | 236.5 | 240 | 269 | 294 | 0.50 |
| cgt-full | stock | 507.4 | 514 | 583 | 671 | 1.07 |
| T1 board, no move list | ctok | 74.6 | 74 | 77 | 77 | 0.16 |
| T1 board + legal moves | ctok | 114.1 | 115 | 126 | 135 | 0.24 |
| T3 cgt-lite | ctok | 158.5 | 160 | 178 | 192 | 0.33 |
| T3 cgt-full | ctok | 355.0 | 359 | 409 | 473 | 0.75 |

The stock rows reproduce the CGT report (`cgt-token-cost-2026-11`) exactly. T1 with
moves is a quarter of the packet and carries the same board, side, rights, clocks,
repetition count and legal moves, minus the fields the compiler derives and the
repetition ledger map. T3 saves 33% on lite and 30% on full against the same text
under the stock tokenizer (`stock cgt-lite` 236.5 against 158.5, `stock cgt-full`
507.4 against 355.0), because a node is two tokens and a destination is one,
instead of two to four sub-words each. These are length counts: shorter does not mean
better grounded, and the T1 and T3 probes decide that.

## Tradeoffs and what we do not claim

- **Not claimed: better play or grounding.** `ctok/1` removes a tokenization
  obstacle and shortens inputs. Whether a 500M model with fresh rows grounds claims
  better than with the stock tokenizer is exactly what arms T1 and T3 measure, against
  the Run 1 control at matched budget. A new embedding with 2.4M untrained
  parameters can also do worse.
- **Not claimed: positional board indexing works.** "Square `k` is at a fixed
  offset" is a design hypothesis about attention and rotary positions; the S0 probes
  test it. The interleaved layout is the fallback.
- **Not claimed: legality.** The tokenizer enforces shape, not rules. A well-formed
  T1 board with two white kings decodes; a well-formed CGT with a false edge decodes.
  Use the compiler (`parse_cgt`, the move gate) for truth.
- **Not claimed: Chess960, other variants, non-standard castling letters.** FEN
  castling must be `KQkq` subsets in order; other fields are refused, not
  approximated. The vocabulary has tokens that Chess960 would need only by accident.
- **Not claimed: exact legal-move coverage of every token.** The 1968 tokens are the
  full geometric space; the 3,169 playout and fixture positions exhibit 1,808 of the 1,968
  tokens as legal moves (1,821 of 4,000 positions when the Run 1 holdout is on disk); the rest are
  exercised only by the bijection check.
- **Not claimed: clock buckets or repetition ledger.** Clocks are exact; the ledger map
  and the packet's derived facts are not tokenized; repetition is one count.
- **Cost of a fixed 64-token board:** quiet endgames spend 60 tokens on empty squares
  that a node list would skip; T3 covers that trade.
- **Cost of host extension:** decode and serving must carry the extended tokenizer and
  the manifest; older checkpoints cannot read these ids; literal ctok strings in user
  text must be handled by the server.
- **Embedding cost:** the new rows add 4.79M trainable parameters (embedding and head) to the LoRA weights; the
  rejected `modules_to_save` route would add 99.4M.

## Rust

`crates/notation` already holds the rules and CGT renderer, and `knightlm-contract`
holds the goldens both languages share. A Rust implementation of the same
vocabulary would fit there: `ctok` as a small module beside `cgt.rs`, with the
vocabulary generated from the same move-space enumeration and a golden of the
manifest hash in `crates/contract/schemas/`, so the Rust server could tokenize
without calling Python and constrained decoding could mask move tokens natively.
This PR ships Python only, because the training and evaluation code is Python and
the format is not yet stable (no arm has used it). Moving it to Rust should wait
until T1 or T3 is chosen for the served model. The Python tests already pin the
manifest hash, which the Rust port would reuse as its golden.

The only Rust change here is a test-data command, `knightlm playouts` (deterministic
random playouts and listed FENs, with legal moves from shakmaty), so the Python tests
take positions and legal moves from the compiler instead of reimplementing rules.

## Library and tests

| piece | file |
|---|---|
| vocabulary, move space, hash, manifest | `model/tokenizer/vocab.py` |
| UCI move to and from a single token | `model/tokenizer/moves.py` |
| T1 position record to and from ids | `model/tokenizer/position.py` |
| T3 CGT text to and from ids | `model/tokenizer/cgt.py` |
| play-call frame | `model/tokenizer/play.py` |
| Hugging Face extension and row init | `model/tokenizer/hf.py` |
| T1/T3 prompt blocks (token strings) | `model/tokenizer/arms.py` |
| trainable new rows, extended base | `model/train/ctok.py`, `model/train/test_ctok.py` |
| token cost report | `model/tokenizer/token_cost.py` |
| tests | `model/tokenizer/test_tokenizer.py`, `model/tokenizer/test_hf.py`, `model/tokenizer/test_arms.py` |

Run `python -m unittest model.tokenizer.test_tokenizer model.tokenizer.test_hf` from the
repository root after `cargo build -p knightlm-cli` (the compiler-backed tests skip
without the binary; the HF tests skip without `transformers` and `torch`). The
tests cover: exact round trip of T1 and T3 (lite and full) over 3,169 distinct
positions from compiler playouts and fixtures (4,000 with the Run 1 holdout when present), every legal move of those
positions has one token and each token one move, distinct positions never encode
equal (including single-field changes), prefix truncation, deletion, insertion and
substitution are refused or yield the canonical encoding of a different document,
Chess960 letters and unknown repetition are handled as stated, and the manifest hash
is pinned.

## Arm prompts and train/serve parity

Arms T1 and T3 change the input only. `model/tokenizer/arms.py` writes the position block as ctok token
strings back to back, and `model/prompt.py` places it in the play prompt (`play-t1`, `play-t3`,
`play-t3-full`):

    {instruction, call format, play schema}  "Position tokens:\n"  <pos><side:w>...</pos>

After `hf.extend_tokenizer` each of those strings is one added token, so the host tokenizer produces
exactly `base + local id` for the block (`model/tokenizer/test_arms.py` pins this on a tiny tokenizer and on
the real SmolVLM one, where `base = 49280`). The completion is the unchanged host-text `submit_move` call,
so `parse_play_completion` and every eval axis apply. Train and serve both build the prompt with
`model.prompt.play_prompt` / `qa_prompt` and tokenise it with the same chat template. `model.train.run
--tokenizer ctok1` checks this on a sample of rows: `prompt_parity` (stored prompt = served prompt),
`tokenisation`, and `trainer_vs_harness_tokens` (the ctok ids in the trainer's prompt equal the id-level
encoding at `base + id`, no ctok id appears in a completion, and the serve path's chat-template encode
equals the trainer's prompt ids). Literal ctok strings typed by a user into a serving prompt would also
tokenise to ctok ids; a server must strip or escape them.
