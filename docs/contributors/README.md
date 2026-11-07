# Contributor onboarding

1. Fork if needed, clone, and branch from current `main`. Use the [running guide](../app/running.md) if your work touches RBC.
2. Pick a scoped [work order](work/README.md) or open a pull request for an ordinary contribution. Check open PRs for overlapping work before you start.
3. Run the checks relevant to your change. For Rust and Python, use `make check` with Stockfish on `PATH` or set `STOCKFISH_PATH`. For app changes, exercise the affected workflow against an isolated database and record the platform, build, and result.
4. Open a PR against `main`. Include the work-order ID if you claimed one, and describe the checks you ran. Maintainers merge after fast CI passes.

Keep datasets, model weights, checkpoints, and local databases in the project
artifact store. See the [documentation index](../README.md) for architecture,
release evidence, and OpenWarrant records.
