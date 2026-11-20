# Chess Graph Text (`cgt/1`)

Status: specification and reference implementation (`crates/notation/src/cgt.rs`).
No prompt, training or evaluation path uses it yet; rendering it into prompts is
a follow-up (arms T2, T3, T5 in the approved plan).

## Why

Run 1 showed that a 500M text model reading FEN cannot ground board claims:
65% of its piece-on-square claims were false. In FEN, "what is on e4" is an
arithmetic puzzle over digits and slashes. CGT writes the position as a graph in
plain text: pieces are nodes named `Nb1`, relations are edges, and legal moves
are grouped under the piece that makes them. A coach claim such as "the knight
on f3 attacks the pawn on e5" is then a line the model copies, not a fact it
computes. The compiler produces the text and parses it back, so the text is
exactly as trustworthy as the rules oracle.

## Invariants it keeps

- **One state.** Everything comes from the current position and the packet's own
  facts. Nothing from earlier plies appears except the packet's repetition count.
- **Engine-zero.** CGT carries no engine evaluation, no score, no best move, no
  threat ranking, no "good/bad" label. Every edge is a rules fact computed by
  shakmaty. A parser refuses text with any line it cannot reproduce.
- **Unknown stays unknown.** An unknown repetition count is `rep ?`. A FEN with
  only four fields is refused instead of getting invented clocks.
- **Lossless.** `render -> parse -> render` is byte-identical; `parse` reproduces
  board, side, castling, en passant and both clocks exactly.

## Document shape

A document is UTF-8 text, lines separated by `\n`, no trailing newline, no blank
lines, no indentation, single spaces between items. Rendering is deterministic;
parsing accepts only the canonical form.

```
cgt/1 full
turn b
castle KQkq
ep -
clock 3 3
rep 1
W: Ke1 Qd1 Ra1 Rh1 Bb5 Bc1 Nb1 Nf3 Pa2 Pb2 Pc2 Pd2 Pe4 Pf2 Pg2 Ph2
B: ke8 qd8 ra8 rh8 bc8 bf8 nc6 ng8 pa7 pb7 pc7 pd7 pe5 pf7 pg7 ph7
moves:
ke8: e7
qd8: e7 f6 g5 h4
...
atk:
Bb5: nc6
Nf3: pe5
def:
Ke1: Qd1 Pd2 Pf2
...
pin:
hang: -
check: -
ksafe:
Ke1: open e2 f1 own d1 d2 f2
ke8: open e7 own d7 d8 f7 f8
```

(The full document for `r1bqkbnr/pppp1ppp/2n5/1B2p3/4P3/5N2/PPPP1PPP/RNBQK2R b KQkq - 3 3`
is produced by `knightlm cgt --fen ... --rep 1`.)

## Grammar

```ebnf
document  = header NL state NL boards NL "moves:" { NL piece-line }
            [ NL edges ] ;
header    = "cgt/1" SP ( "lite" | "full" ) ;
state     = "turn" SP ( "w" | "b" ) NL
            "castle" SP castling NL
            "ep" SP ( "-" | square ) NL
            "clock" SP number SP number NL
            "rep" SP ( number | "?" )
            [ NL "term" SP reason ] ;
castling  = "-" | letters ;                 (* FEN castling field: KQkq, or file letters for inner rooks / Chess960 *)
reason    = "checkmate" | "stalemate" | "proven_dead_position"
          | "fivefold_repetition" | "seventy_five_move"
          | "white_wins" | "black_wins" | "draw" ;
boards    = "W:" [ SP node { SP node } ] NL "B:" [ SP node { SP node } ] ;
node      = piece square ;                   (* UPPER case white, lower case black *)
piece     = "K" | "Q" | "R" | "B" | "N" | "P" | "k" | "q" | "r" | "b" | "n" | "p" ;
square    = file rank ;  file = "a".."h" ;  rank = "1".."8" ;
piece-line = node ":" SP move { SP move } ;
move      = "O-O" [ check ] | "O-O-O" [ check ]
          | [ "x" ] square [ "ep" ] [ "=" ( "Q"|"R"|"B"|"N" ) ] [ check ] ;
check     = "+" | "#" ;
edges     = "atk:"  { NL edge-line } NL
            "def:"  { NL edge-line } NL
            "pin:"  { NL pin-line  } NL
            "hang:" SP ( "-" | node { SP node } ) NL
            "check:" SP ( "-" | node { SP node } ) NL
            "ksafe:" { NL ksafe-line } ;
edge-line = node ":" SP node { SP node } ;
pin-line  = node ":" SP node SP node ;       (* pinned piece: pinner, king *)
ksafe-line = node ":" [ SP "open" { SP square } ] [ SP "hit" { SP square } ]
             [ SP "own" { SP square } ] ;
```

`lite` ends after the `moves:` section; `full` always contains all seven
sections in the order above (empty sections are a bare header, `hang` and
`check` use `-`).

## What each part carries

**State** (from the packet, nothing else). `turn`, `castle` (the packet's FEN
castling field), `ep` (only a legally usable en passant square, as the packet's
FEN), `clock` (halfmove then fullmove), `rep` (the packet's
`repetition_count`; `?` when the history is not complete), and `term` only when
the packet has a terminal reason or the side to move has no legal move
(checkmate or stalemate derived from the board). A terminal position has an
empty `moves:` section.

**Board.** One list per colour, white first. Nodes are `<piece><square>`.

**Moves** (side to move only). One line per piece that has a legal move, listing
destination tokens: `x` marks a capture, `ep` marks en passant (destination is
the empty square the pawn lands on), `=Q` etc. a promotion, `+` check, `#`
mate, and `O-O` / `O-O-O` castling (in the king's line). Each legal move
appears exactly once.

**atk.** An edge `A: B` means piece A attacks enemy piece B. Attacks are
geometric pseudo-attacks: a pinned piece still attacks, and attacks on empty
squares are omitted (the moves section already lists where a piece can go).
Both colours are listed, so the opponent's threats are visible.

**def.** `A: B` means piece A defends friendly piece B (B is on a square A
attacks). Same geometry rules as `atk`.

**pin.** `P: S K` means P is absolutely pinned: removing P exposes its own king K
to enemy slider S. Both colours. Relative pins and skewers are not listed.

**hang.** Non-king pieces of either colour that are attacked and have no
defender. This is a count of attackers and defenders on the node, not a
material exchange evaluation.

**check.** The pieces giving check to the side to move.

**ksafe.** For each king, its adjacent squares: `own` (occupied by its own
piece), `hit` (attacked by the enemy, with the king removed from the board so a
slider's x-ray through the king counts), otherwise `open`. This is a rules fact
about squares, not a judgement of king safety.

## Canonical ordering

- Nodes sort white before black, then K, Q, R, B, N, P, then file a to h, then
  rank 1 to 8. This order applies to the board lists, to source nodes of each
  edge section, and to targets inside a line.
- Destination tokens in a move line sort by destination file, rank, then
  promotion piece (Q, R, B, N); castling tokens go last, `O-O` before `O-O-O`.
- Squares inside `ksafe` groups sort by file then rank; groups appear in the
  order `open`, `hit`, `own`, and an empty group is omitted.
- Pin lines sort by pinned node, then pinner.
- Empty edge lines are omitted; a source with no targets has no line.

## What it deliberately omits

- Any evaluation, search, threat ranking, or strategic label (engine-zero).
- History: no move list, no last move, no earlier positions. Only the packet's
  repetition count and clocks.
- Attacks on empty squares, x-ray and discovered-attack edges, relative pins,
  skewers, forks, static exchange values, mobility counts, pawn-structure tags.
  Those are derived judgements or add size without being a rule fact the model
  must copy. They can be added as `cgt/2` sections if the S0 probes show a gap.
- Chess variants other than standard chess and Chess960 (castling letters follow
  the packet's FEN field; Chess960 round-trips).

## Variants and ablation

| variant | contents | role |
|---|---|---|
| `cgt-lite` | state, both piece lists, legal moves per piece | smallest form that grounds "what is on e4" and "where can this piece go" |
| `cgt-full` | lite plus `atk`, `def`, `pin`, `hang`, `check`, `ksafe` | grounds tactics claims: attacked, defended, pinned, hanging, check |

Arms T2/T3/T5 can be run with either, so size against grounding can be ablated
on the same S0 rules-QA probes. Lite is also a view of the full text: dropping
the edge sections is lossless for everything lite carries.

## Token cost (SmolVLM tokenizer)

Measured on the first 2,000 positions of `holdout-00000.jsonl` in
`run1-reasoning-2026-09-a`. The FEN packet column is the packet JSON of the Run 1
prompt (FEN, facts, repetition ledger, legal move list), without the fixed
instruction line. Evidence: `/mnt/4tb/artifacts/KnightLM/keep/cgt-token-cost-2026-11/`
(`summary.json`, per-position counts). Reproduce with
`python -m model.corpus.cgt_token_cost --corpus <dir> --tokenizer <dir> --out <dir>`.

| form | mean tokens | p50 | p95 | max | mean vs FEN packet |
|---|---|---|---|---|---|
| FEN packet | 474.8 | 456 | 650 | 929 | 1.00 |
| cgt-lite | 236.5 | 240 | 269 | 294 | 0.50 |
| cgt-full | 507.4 | 514 | 583 | 671 | 1.07 |

cgt-lite is half the packet's cost, because the packet repeats the FEN in the
ledger and writes every move as a four-character UCI string. cgt-full costs
about the same as the packet and has a tighter tail (p95 583 against 650), but
it adds the tactical facts that the packet does not state. These are token
counts only; whether the model grounds claims better is the T2 probe's job.

## Design alternatives considered

- **Spatial text (one token per square).** About 75 tokens and strong indexing,
  but it needs new embeddings and does not state relations; kept as arm B/T1.
- **Edges to empty squares.** `attacks_from` for every piece gives a "control
  map". It roughly doubles the text and mostly restates the moves section;
  dropped. Piece-to-piece edges are the facts a coach cites.
- **SAN-style move lists.** `Nf3` needs disambiguation rules and hides the
  origin square; per-piece grouping with destination-only tokens is shorter and
  names the mover once.
- **Colour-prefixed nodes (`wNb1`).** Clearer to a human, costs a token per node;
  case-coded letters were kept (upper white, lower black, as in FEN).
- **JSON or YAML.** Brace and quote overhead and non-canonical key orders; a
  line-oriented form is smaller and trivially canonical.
- **Material exchange or "threat" edges.** Rejected as evaluation; `hang` is the
  only derived tag and it is a pure count of attackers and defenders.
- **Per-line piece labels for the side not to move's moves.** Omitted; legal moves
  belong to the side to move only. The opponent's attacks are in `atk`.

## Versioning

`cgt/1` is the first line of every document. A change to the grammar, the
ordering, or the meaning of an edge is a new version (`cgt/2`); a parser refuses
any version it does not implement. Adding a section is a new version, since the
parser requires the exact canonical text of its own version.

## Parser guarantees

`parse_cgt` rebuilds the position from `turn`, `castle`, `ep`, `clock` and the
piece lists through the strict FEN reader (illegal positions are refused), then
recomputes every section and refuses the text when:

- a move is not legal, or a legal move is missing;
- an edge (`atk`, `def`, `pin`, `hang`, `check`, `ksafe`) does not hold on the
  board, or one is missing;
- a section does not belong to the declared variant;
- `term` contradicts the board (for example `checkmate` on a position with legal
  moves) or the clocks or repetition count;
- the text differs from the canonical rendering in order, spacing, or
  duplicates.

Implementation: `render_cgt`, `render_cgt_state`, `render_cgt_fen`, `parse_cgt` in
`knightlm_notation::cgt`; CLI `knightlm cgt --fen <FEN> [--variant lite|full] [--rep N]`.
Tests: `crates/notation/tests/cgt.rs` (round trip over 13,622 positions from
fixtures and random playouts, independent geometric recomputation of attack,
defend, pin, hanging and check edges, every legal move exactly once, planted
corruption).
