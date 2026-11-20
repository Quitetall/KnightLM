# Training vocabulary and where KnightLM sits

A short orientation for contributors. It explains the terms the model-side
docs assume, and says which part of a frontier training pipeline KnightLM does
and does not do. It makes no strength claim.

## What a model reads

A transformer reads integers. Text is split by a **tokenizer** into pieces from
a fixed vocabulary (SmolVLM: 49,280), each piece has an ID, and the IDs become
vectors.

```
text -> tokenizer -> token IDs -> embeddings -> transformer -> next-token probabilities
```

- JSON is ordinary text to the model. It sees characters cut into tokens and
  has no built-in JSON understanding.
- Raw binary is not read. Images are cut into patches by a vision encoder and
  the patch vectors sit beside the text tokens. Audio is turned into frames.
- The **context window** is how many tokens fit at once (SmolVLM: 8,192).
- A **chat template** adds marker tokens that separate system, user and
  assistant turns. Training data must use the same template as inference.
- BPE tokenizers cut FENs and UCI strings into uneven pieces, so `e2e4` is not
  one clean unit. KnightLM keeps the stock tokenizer and constrains decoding to
  legal moves at inference.

## How data is stored

| Format | Use |
| --- | --- |
| JSONL | One example per line; readable. The KnightLM corpus: `{"prompt","completion"}`. |
| Parquet | Columnar, compressed; the Hugging Face Hub default. |
| Token shards (`.bin`, memmap) | Pre-tokenized integers for very large runs. |
| WebDataset (tar) | Streaming shards for multimodal data. |

**Streaming** reads shards as it trains instead of loading the set, which is how
the trainer handles 210,000 rows at flat memory.

## Training stages

| Stage | What it is | KnightLM |
| --- | --- | --- |
| Pretraining | Learn language from trillions of tokens | Not done; we start from SmolVLM |
| Continued pretraining / mid-training | Keep training on a new domain | Planned: chess text mix |
| SFT | Train on prompt to answer examples, loss masked to the answer | Run 1 |
| LoRA / PEFT | Train small adapter matrices, base untouched | r=8 on q/v |
| Preference tuning (DPO and relatives) | Train on better-versus-worse answers | Possible later, for coach style |
| RL with verifiable rewards | Reward checkable outcomes | Not planned; see engine-zero below |
| Distillation / synthetic data | A stronger model writes the data | Limited by engine-zero |

## Data-quality terms

- **Deduplication:** remove exact and near-duplicate records (MinHash).
- **Decontamination:** remove anything that overlaps the evaluation set. The
  corpus split audit is this.
- **Quality filtering:** heuristics or a classifier that drops junk.
- **Data mixture:** the share of each source in training. Record it in the
  manifest; do not leave it implicit.
- **Packing:** joining short examples into one sequence to avoid padding.
- **Held-out set:** data never trained on, used only to measure.

## Tool calls

A model does not call functions. It writes text in an agreed format (usually
JSON), the harness parses and runs it, and the result goes back as text. So the
tool schema must appear in training data exactly as it appears at inference,
and a call outside the visible schema is a failure.

## Tooling to recognise

- **Frameworks:** PyTorch (Burn is the Rust counterpart, Track B); Hugging Face
  `transformers`, `tokenizers`, `datasets`, `peft`, `trl`.
- **Training stacks:** Axolotl, Unsloth, torchtune, LLaMA-Factory. KnightLM
  uses BLUT's `hf_trainer`.
- **Scale-out:** DeepSpeed, FSDP, Megatron. Not needed on one GPU.
- **Inference:** vLLM, SGLang, llama.cpp/GGUF. KnightLM serves with
  `model/serve`.
- **Data tooling:** DataTrove and Dolma for filtering and dedup; Docling,
  Marker, Nougat and olmOCR for PDFs and scans.
- **Evaluation:** `lm-evaluation-harness` for standard benchmarks;
  `model/eval` for chess.

## What the frontier write-ups say that applies here

From Alex Wa's survey of open-weight training reports,
[frontier model training methodologies](https://djdumpling.github.io/2026/01/31/frontier_training.html),
which leans on Hugging Face's SmolLM3 report. It concerns 3B-and-up models and
mostly pretraining, so read it for method, not for our scale.

- Save the best data for late in training; stage the mixture.
- Deduplicate and decontaminate before anything else.
- A good evaluation is monotonic, low-noise, above random and rank-consistent.
  `model/eval` keeps axes separate for the same reason.
- Small models improve with modest SFT; preference-tuning learning rates sit
  well below SFT rates.
- Match the tokenizer to the target data; check fertility (tokens per
  character) on chess strings before changing a base model.

The survey does not cover PDFs, scans or chess-specific data.

## KnightLM constraints to keep in mind

- **Engine-zero (WAR-0005):** Stockfish never reaches the model or its
  serving path. It is used only by the evaluator after the model commits.
- **One prompt source:** `model/prompt.py`; training rows and serving must
  render identically.
- **Splits:** by time and opening family; the audit refuses any
  cross-split game, family or position.
- **Reuse before inventing:** BLUT stages, the Rust Stockfish driver as the
  oracle, and existing extraction tools come first.
