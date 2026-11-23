# Contributor work orders

This list is the current set of open starter tickets (the [strength protocol](../../work/t-e135/t-e135-research-contributor-define-a-knightlm-strength.md)
starter is done). The same open tickets are mirrored to
[GitHub Issues](https://github.com/Quitetall/KnightLM/issues) by `scripts/sync_issues.py`, which maintainers run;
the ticket file is the record and `war view prime` is the live list. This page is maintained by hand.
Pick a contribution that fits the time and equipment you have. The estimates
are active work, and setup or a first build can take longer. Each starter
ticket gives you a first step, something concrete to bring to review, and a
way to hand back a blocker. You do not need to take on the larger follow-up
to finish a starter.

| Starter | Time | What you need | What to share |
| --- | --- | --- | --- |
| [Docs · Update the RBC roadmap](../../work/t-389d/t-389d-docs-contributor-bring-the-rbc-roadmap-up-to.md) | 30–45 min | A checkout and a text editor | A small PR that distinguishes what ships, what was verified, and what's next |
| [Playtest · Try a first KnightLM game](../../work/t-8021/t-8021-playtester-try-the-first-knightlm-game-in-rbc.md) | 30–45 min | KnightLM in a local build or a maintainer-led session | The steps you tried, what happened, and where you got stuck if applicable |
| [Data · Inventory the Run 1 sources](../../work/t-a475/t-a475-data-engineer-inventory-sources-for-run-1.md) | 45–90 min | The pilot manifest or access to someone who has it | A source list, proposed cutoff, and any missing inputs |
| [Evaluation · Agree the corpus-to-trainer contract](../../work/t-5cd8/t-5cd8-representation-evaluation-engineer-agree-on-the.md) | 60–90 min | The corpus manifest and `model/train/` | A small manifest plus a fixture both sides accept |
| [Windows · Try the source launcher](../../work/t-a226/t-a226-windows-tester-try-the-rbc-source-launcher-on.md) | 45–90 min after setup | Native Windows, PowerShell, and MSVC | A launch report or the first reproducible blocker |

## This team's first picks

Start with [Docs](../../work/t-389d/t-389d-docs-contributor-bring-the-rbc-roadmap-up-to.md) and [Data inventory](../../work/t-a475/t-a475-data-engineer-inventory-sources-for-run-1.md).
The [four-person training plan](training-loop.md) lists the follow-up work for
Data, Representation/Evaluation, and MLOps/CV and how it comes together.

## Pick up and hand back work

1. Check open PRs for the ticket ID. With [OpenWarrant alpha.7](https://github.com/Quitetall/OpenWarrant/releases/tag/v1.0.0-alpha.7),
   run `war view ready` to see available work in your checkout and
   `war start <id> --as your-github-handle` to claim a ticket. For example, start
   with `war start t-389d --as your-github-handle`.
2. Claims in one checkout don't coordinate people in different forks. Open
   a draft PR with the ticket ID when you have a change to share. For
   report-only work, ask a maintainer where to post the report so it can be
   linked from the ticket. A draft is welcome with one result or a specific
   blocker.
3. When the work is ready for review, share what you tried, what happened,
   and what remains. After a maintainer reviews it, record the evidence link with
   `war done <id> --as your-github-handle --note "Evidence in PR or report"`.
   Starter tickets use ordinary maintainer review. The larger signed orders
   below also need their recorded sign-off.
4. If you need to stop, leave the last successful step, the blocker, and what
   someone should try next. Release your claim with
   `war admin release <id> --as your-github-handle`.

You can also contribute through an ordinary PR. See
[contributor onboarding](../README.md) for setup and checks.

## Larger follow-ups

| Role | Existing order | Start with |
| --- | --- | --- |
| Data Engineer | [Build and audit the Run 1 corpus](../../tickets/t-3458.md) | Review the source inventory before generating full shards. |
| QA Engineer | [Isolate the RBC browser release gate](../../work/t-5b6e/t-5b6e-qa-engineer-isolate-the-rbc-browser-release-gate.md) | Take this if you can work on the browser runner and its test environment. |
| Frontend Engineer | [Make Stop responsive during a provider stall](../../work/t-c168/t-c168-frontend-engineer-make-stop-responsive-during-a.md) | Reproduce the silent-provider case from the order before changing the stream. |
| Release Engineer | [Qualify RBC on native Windows](../../work/t-1ce9/t-1ce9-release-engineer-qualify-rbc-on-native-windows.md) | Use the source launch report, then verify the installer separately. |

[App and model roadmap](../../../apps/rusty-brick-chess/ROADMAP.md)
