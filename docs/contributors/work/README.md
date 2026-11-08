# Contributor work orders

Pick a contribution that fits the time and equipment you have. The estimates
are active work, and setup or a first build can take longer. Each starter
ticket gives you a first step, something concrete to bring to review, and a
way to hand back a blocker. You do not need to take on the larger follow-up
to finish a starter.

| Starter | Time | What you need | What to share |
| --- | --- | --- | --- |
| [Docs · Update the RBC roadmap](../../tickets/t-389d.md) | 30–45 min | A checkout and a text editor | A small PR that distinguishes what ships, what was verified, and what's next |
| [Playtest · Try a first KnightLM game](../../tickets/t-8021.md) | 30–45 min | KnightLM in a local build or a maintainer-led session | The steps you tried, what happened, and where you got stuck if applicable |
| [Data · Inventory the Run 1 sources](../../tickets/t-a475.md) | 45–90 min | The pilot manifest or access to someone who has it | A source list, proposed cutoff, and any missing inputs |
| [Research · Design the strength baseline](../../tickets/t-e135.md) | 60–90 min | The research plan and a way to review chess positions | A small evaluation protocol with an example of how to score a result |
| [Windows · Try the source launcher](../../tickets/t-a226.md) | 45–90 min after setup | Native Windows, PowerShell, and MSVC | A launch report or the first reproducible blocker |

## This team's first picks

Start with [Docs](../../tickets/t-389d.md) and [Data inventory](../../tickets/t-a475.md).
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
| QA Engineer | [Isolate the RBC browser release gate](../../tickets/t-5b6e.md) | Take this if you can work on the browser runner and its test environment. |
| Frontend Engineer | [Make Stop responsive during a provider stall](../../tickets/t-c168.md) | Reproduce the silent-provider case from the order before changing the stream. |
| Release Engineer | [Qualify RBC on native Windows](../../tickets/t-1ce9.md) | Use the source launch report, then verify the installer separately. |

[App and model roadmap](../../../apps/rusty-brick-chess/ROADMAP.md)
