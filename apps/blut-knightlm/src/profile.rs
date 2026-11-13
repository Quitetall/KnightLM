//! Base-model profiles: the pinned identity of each model the recipe can train.
//!
//! The recipe takes `base = "<key>"`; everything model-specific lives here so
//! adding a model is one entry. `lora_regex = None` leaves the choice to the
//! trainer's per-`model_type` default (which already covers both models below);
//! the recipe arg `lora_target_regex` overrides either.

use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct BaseProfile {
    pub key: &'static str,
    pub repository: &'static str,
    pub revision: &'static str,
    /// Default local snapshot directory (artifact store; override with `base_dir`).
    pub dir: &'static str,
    /// Weights file name inside `dir`.
    pub weights_file: &'static str,
    /// sha256 of the weights file.
    pub weights_sha256: &'static str,
    /// Whether the trainer's own `base_sha256` check can run: it reads
    /// `<dir>/model.safetensors`, so a sharded or differently named file cannot
    /// use it. The recipe still records the digest in the run manifest.
    pub trainer_checks_sha: bool,
    pub lora_regex: Option<&'static str>,
    /// Whether `model.serve.inference.Predictor` (and so the evaluation
    /// harness) can load this base. It pins the SmolVLM digest.
    pub evaluable: bool,
}

pub const PROFILES: &[BaseProfile] = &[
    BaseProfile {
        key: "smolvlm-500m",
        repository: "HuggingFaceTB/SmolVLM-500M-Instruct",
        revision: "a7da5b986cb59b408707209984f360a5f4ad7e47",
        dir: "/mnt/4tb/artifacts/KnightLM/expire/models/smolvlm-500m-a7da5b98",
        weights_file: "model.safetensors",
        weights_sha256: "d05b567eeaf534e83d375551f068ed57b5f52d37c657197f644af5ef9db091a2",
        trainer_checks_sha: true,
        lora_regex: None,
        evaluable: true,
    },
    BaseProfile {
        key: "qwen3_5-0.8b",
        repository: "Qwen/Qwen3.5-0.8B-Base",
        revision: "dc7cdfe2ee4154fa7e30f5b51ca41bfa40174e68",
        dir: "/mnt/4tb/models/qwen3.5-0.8b-base",
        weights_file: "model.safetensors-00001-of-00001.safetensors",
        // The Hugging Face LFS etag of that file, which is its sha256.
        weights_sha256: "c2b1e5a17d9c1e27685d92ed9b382911ebb99955ecd89052d1721241adfbab6c",
        trainer_checks_sha: false,
        lora_regex: None,
        evaluable: false,
    },
];

pub fn find(key: &str) -> Option<&'static BaseProfile> {
    PROFILES.iter().find(|p| p.key == key)
}

pub fn keys() -> Vec<&'static str> {
    PROFILES.iter().map(|p| p.key).collect()
}

impl BaseProfile {
    pub fn dir_or(&self, over: Option<&Path>) -> PathBuf {
        over.map_or_else(|| PathBuf::from(self.dir), Path::to_path_buf)
    }
}
