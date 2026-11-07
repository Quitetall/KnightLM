# Contributor onboarding

1. Fork if needed, clone, and branch from current `main`. Use the [running guide](../app/running.md) if your work touches RBC.
2. Pick a scoped [work order](work/README.md) or open a pull request for an ordinary contribution. Check open PRs for overlapping work before you start.
3. Run `make gate` before you ask for a merge. With Stockfish on `PATH`, or
   `STOCKFISH_PATH` set, it runs everything the repository rests on: `make check`
   (fmt, clippy, both test profiles, the Python checks), `make check-apps` (the
   server and wasm frontend crates, which the workspace excludes — neither the
   lints nor the tests above see them), and every browser suite in
   `apps/rusty-brick-chess/frontend/tests/cdp/`. `make check` alone can be green
   while the entire UI is untested; that is how the two application crates
   reached 21 and 41 warnings with a passing CI.
4. Open a PR against `main`. Include the work-order ID if you claimed one, and describe the checks you ran. Maintainers merge after fast CI passes.

Keep datasets, model weights, checkpoints, and local databases in the project
artifact store. See the [documentation index](../README.md) for architecture,
release evidence, and OpenWarrant records.
