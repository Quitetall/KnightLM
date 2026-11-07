# Work orders

Use `war view ready` from the Git checkout for current availability. These
starter Orders group work by role. Anyone can claim a ready Order under their
own contributor handle.

| Role | Starter Order |
| --- | --- |
| Data Engineer | [t-3458 · Build and audit the Run 1 corpus](../../tickets/t-3458.md) |
| QA Engineer | [t-5b6e · Isolate the RBC browser release gate](../../tickets/t-5b6e.md) |
| Frontend Engineer | [t-c168 · Make Stop responsive during a provider stall](../../tickets/t-c168.md) |
| Release Engineer | [t-1ce9 · Qualify RBC on native Windows](../../tickets/t-1ce9.md) |

Install [OpenWarrant alpha.7](https://github.com/Quitetall/OpenWarrant/releases/tag/v1.0.0-alpha.7)
for tracked work. The Data Engineer pickup command is `war start t-3458 --as your-github-handle`.
`war start` prints the checklist, scope, and completion criteria.

Claims coordinate worktrees in one clone. Include the Order ID in a draft PR
so contributors in other clones can see who is working on it. When the checks
pass, run `war done <id> --as your-github-handle --note "Evidence in PR"`.
OpenWarrant prints the handoff for the person authorized to sign the Order
closed. Use `war admin release <id> --as your-github-handle` to return a claim.

[Contributor onboarding](../README.md) · [App and model roadmap](../../../apps/rusty-brick-chess/ROADMAP.md)
