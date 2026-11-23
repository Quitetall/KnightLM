# KnightLM model docs

KnightLM is the ML path of this repository: corpus, tokenizer, board encoder, training,
evaluation and serving. RBC, the chess app, is a separate product that meets it at one
contract. If you only want the app, you do not need this folder.

**Status (read this first).** Only Run 1 short (625 steps, 20,000 rows, SmolVLM-500M + LoRA)
has trained. Every other arm is specified, has CPU-tested reference code and a CPU preflight,
and is waiting for the owner's go. Nothing here is a strength claim. Each document opens with
its own `Status:` line; trust that line over the title.

## Reading order for a newcomer

1. [Training vocabulary](training-glossary.md): one page on tokens, SFT, LoRA and data terms,
   and which part of a training pipeline KnightLM does. Read it if the rest uses words you do
   not know.
2. [Implementation contract (`plan.md`)](plan.md): the invariants. State comes from a
   deterministic compiler, never from model memory; no engine at inference; perception is
   not reasoning; results are scored on separate axes.
3. [Version 1 contracts](contracts.md): the packet and the data and evidence records, with
   their rejection rules.
4. [The seam](seam.md): the OpenAI-compatible `/v1/chat/completions` contract, what RBC and
   KnightLM may import, and the pin rule.
5. Representation arms, in the order the model sees a board:
   [Chess Graph Text](graph-text.md) (`cgt/1`, text), [tokenizer](tokenizer.md) (`ctok/1`, board
   tokens), [board encoder](board-encoder.md) (arms T4 and T5, features into the model),
   [move head](move-head.md) (a policy head without language decoding) and
   [perceiver](perceiver.md) (image to piece placement, a separate model).
6. Corpus: [corpus v3](corpus-v3.md) (one pipeline for every source), then
   [reasoning v2](reasoning-v2.md) (short compiler-grounded reasoning) and
   [modes and tools](modes-and-tools.md) (play and coach turns, tool schemas).
7. [Training gate](training-gate.md): every arm in run order, with measured and unmeasured numbers
   and the rule that each GPU command needs the owner's explicit go.
8. [Scorecard](scorecard.md): the axes, when two scorecards may be compared, and what they
   cannot say; [strength baseline](strength-baseline.md) fixes the evaluation settings.

## Reference and history

- [Track B: SmolVLM and Burn study](trackb-smolvlm-study.md) and [scaffold verification](history/verification.md).
- [`model/README.md`](../../model/README.md): the commands for the pilot, serving, BLUT training
  and the preflight. This folder explains why; that file says how.
- The architecture spec is [`docs/sas/KnightLM_SAS.md`](../sas/KnightLM_SAS.md).

## Where the code is

| Area | Path |
| --- | --- |
| Corpus builders and audits | `model/corpus/` |
| Prompt and contract parity with Rust | `model/prompt.py`, `crates/contract/` |
| Tokenizer, encoder, perceiver, move head | `model/tokenizer/`, `model/encoder/`, `model/perceiver/`, `model/policy_head/` |
| Training and the BLUT recipe | `model/train/`, `apps/blut-knightlm/` |
| Evaluation | `model/eval/` |
| Serving | `model/serve/` (`fake_server.py` runs it with no model) |

To set up and run any of it, follow the [getting started guide](../contributors/README.md).
