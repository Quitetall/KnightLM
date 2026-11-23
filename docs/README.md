# Documentation

RustyBrickChess (RBC) is the open-source chess workstation. KnightLM is the ML path: corpus,
tokenizer, encoder, training, evaluation and serving. They meet at [the seam](model/seam.md).

| I want to… | Start here |
| --- | --- |
| Install RBC or open the app | [Running RBC](app/running.md) |
| Set up a fresh clone, for the app or the model | [Getting started](contributors/README.md) |
| Open a pull request, see what CI checks, claim work | [CONTRIBUTING](../CONTRIBUTING.md) |
| Find or claim work | [Work orders by role](contributors/work/README.md) |
| See what comes next | [App and model roadmap](../apps/rusty-brick-chess/ROADMAP.md) |
| Learn how KnightLM fits together | [Model docs index and reading order](model/README.md), [glossary](model/training-glossary.md) |
| Train, evaluate or serve KnightLM | [Model tooling commands](../model/README.md), [training gate](model/training-gate.md), [scorecard](model/scorecard.md) |

## App reference

- [RBC developer reference](../apps/rusty-brick-chess/README.md) covers the API and desktop shell.
- [MCP](app/mcp.md) covers external tools and approvals. [Security](app/security.md) covers the trust boundaries.
- [Database migrations](app/migrations.md) covers schema upgrades and session recovery.

## Release and research evidence

- [1.0 release notes](releases/release-notes-1.0.md) describe the feature set. The [release candidate](releases/release-candidate.md) records the checks for a specific build.
- [Benchmarks](releases/benchmarks.md) record measured performance and test conditions.
- [Burn and SmolVLM study](model/trackb-smolvlm-study.md) tracks backend parity. [Scaffold verification](model/history/verification.md) records the original fixture checks.

## OpenWarrant records

| Path | Contents |
| --- | --- |
| `work/` and `tickets/` | Tickets and Orders, open and done, with their journals (`work/<id>/`; a few Orders sit in `tickets/`) |
| `warrants/` | Directory Warrants, deliverable digests, journals, and evidence |
| `sas/` and `adr/` | Architecture revisions and decision records |
| `authority/` and `gates/` | Authority records and registered checks |
| `generated/` and nested `generated/` directories | Views rebuilt with `war admin compile` |

`openwarrant.toml` also reserves `docs/roadmap/` and `docs/receipts/` for
OpenWarrant. The published app and model roadmap is linked above. See the
[signing guide](contributors/signing.md) for human sign-off on governed work.

Use this index to route readers to the source document. Keep contributor
instructions in `contributors/`, work-order discovery in `contributors/work/`,
and the Orders themselves in `tickets/` so OpenWarrant can find them.
