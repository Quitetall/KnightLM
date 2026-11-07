# Documentation

RustyBrickChess (RBC) is the app. KnightLM is the chess model being trained for it.

| I want to… | Start here |
| --- | --- |
| Install RBC or open the app | [Running RBC](running.md) |
| Make my first contribution | [Contributor onboarding](contributors/README.md) |
| Find or claim work | [Work orders by role](contributors/work/README.md) |
| See what comes next | [App and model roadmap](../apps/rusty-brick-chess/ROADMAP.md) |
| Train or evaluate KnightLM | [Training quickstart](../ports/README.md), [experiment plan](plan.md), [data and evidence contracts](contracts.md) |

## App reference

- [RBC developer reference](../apps/rusty-brick-chess/README.md) covers the API and desktop shell.
- [MCP](mcp.md) covers external tools and approvals. [Security](security.md) covers the trust boundaries.
- [Database migrations](migrations.md) covers schema upgrades and session recovery.

## Release and research evidence

- [1.0 release notes](release-notes-1.0.md) describe the feature set. The [release candidate](release-candidate.md) records the checks for a specific build.
- [Benchmarks](benchmarks.md) record measured performance and test conditions.
- [Burn and SmolVLM study](trackb-smolvlm-study.md) tracks backend parity. [Scaffold verification](verification.md) records the original fixture checks.

## OpenWarrant records

| Path | Contents |
| --- | --- |
| `tickets/` | Current and completed work, including contributor Orders and their journals |
| `warrants/` | Directory Warrants, deliverable digests, journals, and evidence |
| `sas/` and `adr/` | Architecture revisions and decision records |
| `authority/` and `gates/` | Authority records and registered checks |
| `generated/` and nested `generated/` directories | Views rebuilt with `war admin compile` |

`openwarrant.toml` also reserves `docs/roadmap/` and `docs/receipts/` for
OpenWarrant. The published app and model roadmap is linked above. See the
[signing guide](signing.md) for human sign-off on governed work.

Use this index to route readers to the source document. Keep contributor
instructions in `contributors/`, work-order discovery in `contributors/work/`,
and the Orders themselves in `tickets/` so OpenWarrant can find them.
