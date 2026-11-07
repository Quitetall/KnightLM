# Contributing to RustyBrickChess

Use the [RBC quickstart](README.md#start-rbc) to run the app. Branch from current
`main` and open a pull request back to `main`. Fork the repository if you need
a place to push your branch.

## Pick up a work order

OpenWarrant [alpha.7](https://github.com/Quitetall/OpenWarrant/releases/tag/v1.0.0-alpha.7)
is optional for ordinary contributions. If you want a scoped starter task, pick
a ready order with `war view ready` or start with one of these:

| Role | Starter order |
| --- | --- |
| Data Engineer | [Build and audit the Run 1 corpus](docs/tickets/t-3458.md) |
| QA Engineer | [Isolate the RBC browser release gate](docs/tickets/t-5b6e.md) |
| Frontend Engineer | [Make Stop responsive during a provider stall](docs/tickets/t-c168.md) |
| Release Engineer | [Qualify RBC on native Windows](docs/tickets/t-1ce9.md) |

Use your own contributor handle when you pick up the data order.

```sh
war view ready
war start t-3458 --as your-github-handle
```

`war start` shows the checklist, file scope, and evidence the order needs.
Claims coordinate the worktrees in your clone. Check open pull requests first
and include the order ID in a draft PR so contributors in other clones can see
who is working on it.

## Test and submit

- Run `make check` for the Rust and Python checks used by CI. Install Stockfish
  and set `STOCKFISH_PATH` if the executable is outside your `PATH`.
- For app changes, verify the affected workflow against an isolated RBC
  database and share the build, platform, and result in the PR.
- Keep datasets, model weights, checkpoints, and local databases outside Git.
- For a tracked order, run `war done <id> --as your-github-handle --note "Evidence in PR"`
  after the checks pass. OpenWarrant prints the handoff for the authorized
  person who signs the order closed.

Maintainers merge pull requests after fast CI passes. [Roadmap](apps/rusty-brick-chess/ROADMAP.md)
