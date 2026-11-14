# Contributor onboarding

1. Fork if needed, clone, and branch from current `main`. Use the [running guide](../app/running.md) if your work touches RBC.
2. Pick a scoped [work order](work/README.md) or open a pull request for an ordinary contribution. Check open PRs for overlapping work before you start.
3. Share early work in a draft PR. Include the work-order ID, the first result,
   and anything that blocked you. Run the checks relevant to the change and
   list them in the PR. For documentation, review the rendered Markdown and
   check the links you changed.
4. Ask for review when the scoped work is ready. The maintainers coordinate
   fast CI and release qualification. For a release candidate, `make gate`
   covers the workspace, the separately built RBC crates, and the browser
   suites. It needs Stockfish on `PATH` or `STOCKFISH_PATH` set.

Keep datasets, model weights, checkpoints, and local databases in the project
artifact store. See the [documentation index](../README.md) for architecture,
release evidence, and OpenWarrant records.

Keep credentials out of the tree: see [Secrets](secrets.md).
