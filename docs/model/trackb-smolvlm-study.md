# Track B — SmolVLM architecture study (Burn-native admission)

## Pinned reference

- Base model: `HuggingFaceTB/SmolVLM-500M-Instruct` (revision pinned by the
  operator before this study; recorded in the run-1 manifest of KLM-WAR-0008).
- Burn crate: `0.21.0` (workspace pin, `code/Cargo.toml`).
- Backend under evaluation: `wgpu` (portable) with `cuda` as the fast path on
  the RunPod image; the choice is recorded in M1 evidence.

## Component inventory (vs. the HF reference)

| Component | HF module | Params (approx.) | Burn counterpart needed |
|---|---|---|---|
| Vision encoder | SigLIP (ViT) | ~93M | `burn::nn::transformer` blocks + patch embedding |
| Pixel-shuffle projector | `SmolVLMVisionProjector` | ~? | custom `Module` (linear + reshape) |
| Language model | SmolLM2-360M (Llama-style) | ~360M | `burn::models::llama` or hand-rolled decoder |
| Tokenizer | `AutoTokenizer` (SentencePiece/BPE) | n/a | `tokenizers` crate or `burn`'s text utilities |

## Parameter counts

Total ~500M. Encoder ~93M, projector ~?M, decoder ~360M, embeddings ~?M.
Exact counts are verified against the pinned checkpoint's `config.json` at
M1 and recorded in the evidence directory — not asserted from memory here.

## State reset points (RQ-013 reset isolation)

Between inference passes the following must be cleared or re-initialized:
- KV-cache (attention) for every decoder layer
- Any recurrent/convolutional state in the projector
- Dropout (eval mode) — no stochastic state, but mode flag must be set
- Batch-norm running stats (not used in SmolVLM, but listed for completeness)

## Parity gates (KLM-SAS-RQ-013) → named tests

| Gate | Test name | Status today |
|---|---|---|
| processor/tokenizer parity | `processor_tokenizer_parity` | RED (no impl) |
| reference logits | `reference_logits` | RED (no impl) |
| gradients | `gradients_match` | RED (no impl) |
| overfit a tiny batch | `overfit_tiny_batch` | RED (no impl) |
| reload checkpoint | `reload_checkpoint` | RED (no impl) |
| reset isolation between passes | `reset_isolation` | RED (no impl) |

## Honest-red contract

Every gate test fails today with an explicit reason ("not implemented"). A
planted test claiming parity with fabricated logits must be caught by
`fake_green_refused` (below). Green is earned, not asserted.
EOF
echo written