# Contributing to RustyBrickChess and KnightLM

This repository holds the RBC chess app and the KnightLM model path. Start with
[Getting started](docs/contributors/README.md) for setup on a fresh clone, then use this page for
the flow from branch to merge.

## Flow

1. **Find or propose work.** Pick a [starter or work order](docs/contributors/work/README.md), an
   [open issue](https://github.com/Quitetall/KnightLM/issues), or send an ordinary change. Check open
   pull requests for overlapping work.
2. **Branch from current `main`.** One short-lived branch per change. Never push to `main`.
3. **Open a draft pull request as soon as you push.** Link the ticket id or issue, give the first
   result or the blocker, and list the checks you ran (the pull request template asks).
4. **Run the checks that fit the change** (below). Fix what they find on your branch.
5. **Mark it ready when fast CI is green.** A maintainer reviews. Merge with a merge commit; the
   branch is deleted on merge.

Keep credentials, datasets, weights, checkpoints and databases out of the tree ([Secrets](docs/contributors/secrets.md)).
Training runs on a GPU need the owner's go first ([why](docs/contributors/README.md#training-needs-the-owners-go)).

## Checks you can run

| Change touches | Run |
| --- | --- |
| Rust workspace, contract, Python model code | `make check` (fmt, clippy `-D warnings`, tests, Python model tests, CLI, port, pin and layering checks) |
| Torch-dependent model code | `make check-torch PYTHON=<venv python>`; setup in [Getting started](docs/contributors/README.md#python-environments) |
| RBC server or wasm frontend | `make check-apps` |
| Desktop shell | `make check-desktop` (Linux, Tauri libraries and a virtual display) |
| Docs | `python3 scripts/check_doc_links.py`; read the rendered Markdown |
| A release candidate | `make gate` (maintainers; needs Stockfish) |

## CI jobs

Every push and pull request runs the `scaffold` workflow. These jobs must pass before a merge:

| Job | What it checks |
| --- | --- |
| `cpu` | `make check` on Ubuntu with Stockfish, then `make check-apps` |
| `model-torch` | Builds the Rust binaries, installs `model/ci-requirements.txt` (CPU torch) and runs `make check-torch`; a skip for a missing dependency or an unbuilt binary fails the job, so the serve tests and the `knightlm-uci` game gate really run |
| `desktop-shell` | Lints and builds the Tauri shell, launches it under a virtual display and checks the window, token, single instance and shutdown |
| `mojo-parity` | Mojo packet parity against Rust and Python |
| `secret-scan` | gitleaks over the full history |

The `release` workflow runs on `v*` tags. The full gate and browser suites run at checkpoints, not
on every push.

## Claiming work

Tracked work is [OpenWarrant](https://github.com/Quitetall/OpenWarrant) tickets in this repository
(`docs/work/`, `docs/tickets/`). OpenWarrant is optional: an ordinary pull request needs no ticket.
When you want one, with `war` installed:

```sh
war view prime                  # what is open, who holds what, recent notes
war view ready                  # what can start now
war start t-389d --as your-github-handle     # claim a ticket and print its brief
war claim t-389d/i-6baa --as your-github-handle   # or claim one item
war done t-389d --as your-github-handle --note "Evidence in PR #NN"   # tick it and release the claim
war admin release t-389d --as your-github-handle     # give a claim back without finishing
```

A claim in one checkout does not coordinate people in different forks, so also say so on the issue
or draft pull request. Open tickets are mirrored to GitHub Issues by
`python3 scripts/sync_issues.py` (maintainers run it; dry run by default); the issue links the ticket
file. The ticket, not the issue, is the record. Details: [work orders](docs/contributors/work/README.md).
Sign-off on governed work is a person's act: see the [signing guide](docs/contributors/signing.md).

## Conventions

- Commands that can hang get a timeout. Rust builds on shared machines go through `oneoff` or
  `priority-build` if you have them; plain `cargo` is fine elsewhere.
- Preserve unrelated edits. Do not rewrite published history.
- Keep documentation current with the change: a wire change updates [the seam](docs/model/seam.md), a new
  arm updates the [training gate](docs/model/training-gate.md).
