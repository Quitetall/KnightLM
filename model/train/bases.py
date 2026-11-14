"""Pinned base models the BLUT recipe (`knightlm_lora`) can train.

Mirrors apps/blut-knightlm/src/profile.rs; model/tests/test_train_run.py checks
the two agree. The recipe is the authority at run time; this table lets the
entry point refuse a bad base directory before anything is launched.
"""

from pathlib import Path

BASES = {
    "smolvlm-500m": {
        "repository": "HuggingFaceTB/SmolVLM-500M-Instruct",
        "revision": "a7da5b986cb59b408707209984f360a5f4ad7e47",
        "dir": "/mnt/4tb/artifacts/KnightLM/expire/models/smolvlm-500m-a7da5b98",
        "weights_file": "model.safetensors",
        "weights_sha256": "d05b567eeaf534e83d375551f068ed57b5f52d37c657197f644af5ef9db091a2",
        "evaluable": True,
        # Tokens/s of the BLUT trainer on the shared RTX 4090 (smokes, 2026-11-13):
        # 6.5k-10.5k at batch 2, 14k-26k at batch 4. Conservative planning value.
        "tokens_per_second": 14000,
    },
    "qwen3_5-0.8b": {
        "repository": "Qwen/Qwen3.5-0.8B-Base",
        "revision": "dc7cdfe2ee4154fa7e30f5b51ca41bfa40174e68",
        "dir": "/mnt/4tb/models/qwen3.5-0.8b-base",
        "weights_file": "model.safetensors-00001-of-00001.safetensors",
        "weights_sha256": "c2b1e5a17d9c1e27685d92ed9b382911ebb99955ecd89052d1721241adfbab6c",
        "evaluable": False,
        # flash-linear-attention / causal-conv1d are not installed, so the Gated
        # DeltaNet layers use the slow torch fallback: about 300 tokens/s.
        "tokens_per_second": 300,
    },
}

# The defaults the recipe applies (see apps/blut-knightlm/src/recipes/knightlm_lora.rs).
RECOMMENDED = {
    "epochs": 1, "batch_size": 8, "grad_accum": 4, "seq_len": 2048, "lr": 2e-4, "rank": 8,
    "alpha": 16, "seed": 42, "lr_scheduler": "cosine", "warmup_ratio": 0.03, "max_grad_norm": 1.0,
    "save_steps": 500, "keep_checkpoints": 3,
}

DEFAULT_CORPUS = Path("/mnt/4tb/artifacts/KnightLM/expire/corpus/run1-2026-09-a")
DEFAULT_RUN_ROOT = Path("/mnt/4tb/artifacts/KnightLM/expire/runs")
