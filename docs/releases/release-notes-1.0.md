# RustyBrickChess 1.0.0 — release notes

The first release with the Assistant: a chess workstation where a model can read
the board you are looking at, work through positions with you, and write studies
it has to ask permission for.

## What you can do

**Talk about the position you are on.** The Assistant reads the board, the engine
line and your database, and the context it compiles is shown to you with its
hash before anything is sent. Attach a position or a pasted game as a chip, or
mention a study with `@`; `/` offers the prompts the app ships.

**Watch two models play, or play one yourself.** Matches have a board, a move
list, a clock, PGN export and a transcript of reasoning. A game can be paused,
stepped ply by ply, resumed and reset. Model-versus-model games under the standard
ruleset are rated with Glicko-2; a human game is not, and neither is a variant.

**Run a tournament.** Swiss, knockout or double round-robin, up to sixteen
competitors, with a budget that pauses a run rather than killing it.

**Author studies.** Chapters, boards, a move tree, annotations, notes in Markdown,
tags, progress you can tick per user, versions you can branch, and a lock that
keeps a study still. A published study refuses further writes and offers a copy.

**Compare two studies.** A structural diff across chapters, boards, moves, shapes,
annotations, notes, metadata and progress, with a Markdown export, a first
divergence, and two boards that follow one ply together.

**Drive it from outside.** An MCP server over stdio and Streamable HTTP, with a
bearer token, 240 requests a minute, and every write going through the same
approval queue a person uses.

## Things worth knowing

**A key is a name, not a value.** The provider settings store the *name* of an
environment variable or a keyring entry. Nothing writes a secret to the database,
the audit log or a stored turn, and `tests/providers.rs` fails if that changes.

**A refusal is recorded.** When a write is refused — a locked board, a published
study, a denied approval — the reason is kept, and the approval stays approved.
Silence is never the answer.

**The gate.** `./scripts/check_all.sh` runs formatting, both test profiles, clippy
on the workspace, and every browser suite. It stops at the first failure and names
it.

## Numbers

`docs/releases/benchmarks.md` has the measurements and the conditions they were taken
under, including a million-game database: position search 3 ms, an Elo-banded
search 170 ms, the explorer 1.3 s. The explorer is the known slow spot.

## Known limits

- The explorer takes 1.3 s against a million games. Named, measured, next.
- `cdp_match_ui` was red for a fortnight because the tournament panel added a
  second button labelled "Step"; both panels keep the label because a person
  reading the screen wants it, and the check now goes by class.
- No Tauri shell is built here: the app is a server plus a static frontend.
- Variants are parsed and rejected honestly rather than half-supported.
