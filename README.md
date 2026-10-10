# KnightLM

A Rust/Burn research scaffold for chess reasoning with compiled state. The
hypothesis is that replacing accumulated model context with a fresh, complete
state packet can improve reliability and context efficiency. This is an
experiment: it does not presume that fresh state or any helper configuration wins.

**Status:** runnable compiler, fixtures, experiment contracts, CPU training smoke,
and Python/Mojo protocol slices. No pretrained inference or fine-tuning backend
is implemented yet. There is no chess strength, vision accuracy, or semantic
understanding result.

## Intended system

```text
                         authoritative game ledger
                                    |
image -> VLM observation -> validated state -> compiler -> fresh VLM pass
             |                                               |
       perception score                              move + explanation
```

Both VLM passes use the same weights, with independent inference state. Hidden
game metadata comes from the ledger, never pixels. An observation mismatch is
reported, not silently corrected. KV caches, recurrent state, and convolution
state must all reset. The current adapter tests use a deliberately stateful
fixture; they do not prove any real checkpoint's reset behavior.

SmolVLM-500M-Instruct is the first planned integration target. Intended research
models are Qwen 3.5-class 4B/9B models, preserving general language and reasoning
alongside chess and digital-board vision. All profiles are deliberately
unadmitted until processor, tokenizer, logits, gradients, persistence and reset
parity have been demonstrated for that architecture.

## Run locally

Rust 2024 edition and Python 3.10+ are required. Burn is pinned to 0.21.0;
`Cargo.lock` pins dependencies. The default build uses only the CPU ndarray
backend. Initial verification used Rust 1.97.1 and Python 3.14.7.

```sh
cargo build --locked -j 2
cargo test --workspace --locked -j 2
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings
python3 -m unittest discover -s ports/python
python3 scripts/check_cli.py
python3 scripts/check_ports.py
```

The CLI checks create temporary artifacts and exercise the following commands:

```sh
cargo run --locked -- compile fixtures/repetition.json
cargo run --locked -- compile fixtures/start.json --legal-choices --features
cargo run --locked -- probes fixtures/start.json
cargo run --locked -- render fixtures/start.json --black-bottom --blue-theme
cargo run --locked -- evaluate-fixtures fixtures/start.json
cargo run --locked -- simulate fixtures/start.json e2e4 e7e5 --budget 8
cargo run --locked -- validate-evals fixtures/evaluations.jsonl
cargo run --locked -- audit-splits fixtures/splits.json
cargo run --locked -- validate-corpora fixtures/corpora.json
cargo run --locked -- verify-observation fixtures/start.json fixtures/observation.json
```

`render` writes a deterministic SVG using letter pieces; it is an image fixture,
not a recognizer or a realistic vision benchmark. Rasterization and held-out real
board themes belong to the pretrained vision stage.

`evaluate-fixtures` selects scripted moves and deliberately injects an illegal
unmasked diagnostic in masking arms. Its character counter is **not a model
tokenizer**. Reports include exact requests, source game records and experiment
configuration, and are labeled `fixture_only`. They do not measure a model.

Run the CPU gradient/checkpoint check with a **new** output path:

```sh
mkdir -p runs
cargo run --locked -- train-smoke runs/cpu-smoke.json
```

The smoke learns a two-weight regression, verifies nonzero finite gradients,
parameter updates and reload equivalence, and refuses to overwrite checkpoints.
It proves Burn infrastructure only, not LLM training compatibility.

This command intentionally prints an admission report and **exits nonzero**:

```sh
cargo run --locked -- verify-model configs/models/smolvlm-500m.json
```

For executable Mojo checks, see [ports/README.md](ports/README.md). Mojo 1.0.0 can
be installed into `ports/mojo/.venv`; no system environment changes are needed.
`python3 scripts/check_ports.py --require-mojo` requires all three languages.

## Experiments

| Arm | Context and assistance |
|---|---|
| `history` | Verified initial FEN and actual played moves |
| `history_state` | Same history plus current complete packet |
| `fresh_state` | Current complete packet only |
| `legal_choices` | Fresh state plus legal-move list |
| `legal_masking` | Fresh state plus decoder legality constraint |
| `derived_features` | Fresh state plus attacks, pins and material counts |
| `rule_tools` | Fresh state plus bounded hypothetical transitions |
| `combined` | Fresh state plus all listed helpers |

History-state isolates the effect of forgetting history from the effect of
supplying state. Prior model prose is a separately configured contamination
control. Fresh arms reject retention of that prose. Compare a shared fine-tuned
checkpoint first, then specialized fine-tunes from the same base under matched
budgets. Real token counts must come from the admitted model's tokenizer.

Raw/unmasked legality, assisted legality, causal probes, explanation claims,
whole-board perception, play strength, retention and cost remain separate.
There is no composite winner score. Free prose is explicitly unscored; currently
only supported structured claims can be mechanically checked. Model-free probes
cover current check, hypothetical check, next-position FEN and verified factual
explanation templates. They omit answer-bearing rule facts from inputs.

## Repository map

- `crates/core`: replay, rules, compiler, bounded simulation, dataset contracts,
  probes, SVG fixtures, experiment requests and score receipts.
- `crates/model`: backend/reset boundary, pretrained admission report and Burn
  CPU gradient/checkpoint smoke.
- `crates/cli`: local workflow and fixture walkthrough.
- `fixtures`, `configs`: small authored fixtures, provenance, split and model
  profiles. Bulk data, checkpoints, and generated runs stay ignored.
- `ports`: Python and Mojo protocol parity slices, not full model ports.
- [docs/plan.md](docs/plan.md): accepted scope and research intent.
- [docs/contracts.md](docs/contracts.md): state, failure and evidence semantics.

## Data and proof limits

The evaluation JSONL fixture contains **fabricated example scores**, not actual
Stockfish results. The real [Lichess export](https://database.lichess.org/) has
four-field FENs, heterogeneous search depths and UCI_Chess960 PV notation. Those
records cannot establish repetition history or counters. Normalized game
replays carry explicit source assertions about history completeness. PGN import,
streaming bulk ingestion and training mixture scheduling are later work.

Only K-vs-K, K+B-vs-K and K+N-vs-K are recognized as proven dead positions.
Other dead-position cases are unknown; this scaffold is not a complete FIDE
adjudicator. Ordinary legality, mate/stalemate, claimable 50-move/threefold rules
and automatic 75-move/fivefold rules are tested. Future draw claims can be checked
against simulated successor states; no move recommendation is supplied.

Split audits reject shared scored positions, game IDs and related-family IDs.
They cannot establish absence of contamination in a base model's pretraining.
Expert annotations and books require recorded provenance and a usage basis.
External engines may supply training labels or post-decision scoring, never
inference input. No engine process is started by this scaffold.

## Next admission gate

Implement the pinned SmolVLM architecture/processor in Burn and compare it with
the Python reference: preprocessing, tokenizer, logits, gradients, tiny-set
overfit, reload and reset isolation. Qwen needs its own architecture gate,
including Gated DeltaNet state; passing SmolVLM cannot admit Qwen. Burn-LM was
[archived](https://github.com/tracel-ai/burn-lm) and is a reference, not a turnkey
dependency. No model weights or bulk datasets are downloaded by current commands.
