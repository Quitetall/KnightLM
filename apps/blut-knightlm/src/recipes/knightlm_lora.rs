//! Recipe `knightlm_lora`: corpus -> image-text LoRA -> evaluation -> registry.
//!
//!   materialize_corpus -> register_corpus -> claim_run -> hf_sft_train
//!     -> evaluate_checkpoint -> register_run
//!
//! * `materialize_corpus` audits the knightlm-lichess-v2 corpus and converts it
//!   with `model.prompt` (the prompt the model is served with).
//! * `register_corpus` wraps the standard `register_dataset` so a rerun does not
//!   fail on the existing row; the name carries the corpus and prompt hashes.
//! * `claim_run` is the crash gate (see its docs); `hf_sft_train` is the
//!   standard trainer with `arch = "image_text"`, `resume = true` and its
//!   checkpoints in the run directory in the artifact store.
//! * `evaluate_checkpoint` calls the external harness; `register_run` writes
//!   the run manifest and the `model://` handle.
//!
//! The base model and LoRA regex are parameters (`base`, `lora_target_regex`),
//! e.g. `"base": "qwen3_5-0.8b"`.

use std::path::{Path, PathBuf};

use blut::framework::error::RecipeError;
use blut::framework::plan::Plan;
use blut::recipes::recipe::Recipe;
use blut_backends::backends::hf_trainer::stages::hf_sft_train::{Args as TrainArgs, HfSftTrain};
use blut_backends::backends::hf_trainer::{ARCH_IMAGE_TEXT, ImageTextOpts};
use blut_backends::stages::register_dataset::Args as RegDsArgs;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::profile;
use crate::runctx::sha256_file;
use crate::stages::{
    ClaimRun, EvaluateCheckpoint, MaterializeCorpus, RegisterCorpus, RegisterRun, claim_run,
    evaluate_checkpoint, materialize_corpus, register_run,
};

pub const DEFAULT_CORPUS: &str = "/mnt/4tb/artifacts/KnightLM/expire/corpus/run1-2026-09-a";
pub const DEFAULT_RUN_ROOT: &str = "/mnt/4tb/artifacts/KnightLM/expire/runs";

#[derive(Default)]
pub struct KnightlmLora;

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Args {
    /// Run name; its directory under `run_root` holds checkpoints, the ledger,
    /// the adapter and the run manifest. A finished run is never extended: use
    /// a new name to retry. Default: `<base>-<corpus manifest sha[..12]>`.
    #[serde(default)]
    pub run: Option<String>,
    #[serde(default = "default_run_root")]
    pub run_root: PathBuf,
    /// knightlm-lichess-v2 corpus directory (Run 1 by default).
    #[serde(default = "default_corpus")]
    pub corpus_dir: PathBuf,
    /// Base-model profile: `smolvlm-500m` (default) or `qwen3_5-0.8b`.
    #[serde(default = "default_base")]
    pub base: String,
    /// Override the profile's snapshot directory.
    #[serde(default)]
    pub base_dir: Option<PathBuf>,
    /// Override the LoRA target regex (full match on module names).
    #[serde(default)]
    pub lora_target_regex: Option<String>,
    #[serde(default = "default_rank")]
    pub rank: u32,
    #[serde(default = "default_alpha")]
    pub alpha: u32,
    #[serde(default = "default_lr")]
    pub lr: f32,
    #[serde(default = "default_epochs")]
    pub epochs: u32,
    #[serde(default = "default_batch")]
    pub batch_size: u32,
    #[serde(default = "default_accum")]
    pub grad_accum: u32,
    #[serde(default = "default_seq_len")]
    pub seq_len: u32,
    #[serde(default = "default_seed")]
    pub seed: u64,
    #[serde(default = "default_scheduler")]
    pub lr_scheduler: String,
    #[serde(default = "default_warmup")]
    pub warmup_ratio: f64,
    #[serde(default = "default_clip")]
    pub max_grad_norm: f64,
    #[serde(default = "default_save_steps")]
    pub save_steps: u32,
    #[serde(default = "default_keep")]
    pub keep_checkpoints: u32,
    #[serde(default)]
    pub shuffle_buffer: Option<u32>,
    /// Stop after this many optimizer steps (smokes). Default: the full epochs.
    #[serde(default)]
    pub max_steps: Option<u64>,
    #[serde(default)]
    pub logging_steps: Option<u32>,
    #[serde(default = "default_true")]
    pub verify_reload: bool,
    /// Holdout rows the evaluation scores (first N, manifest order).
    #[serde(default = "default_eval_rows")]
    pub eval_rows: u64,
    /// Set false to skip evaluation. It is skipped anyway for a base the
    /// serving predictor cannot load.
    #[serde(default = "default_true")]
    pub evaluate: bool,
    /// Baseline scorecard directory for `model.eval.compare` (per-axis, no composite).
    #[serde(default)]
    pub baseline: Option<PathBuf>,
    /// Checkout providing `model.eval` (default: this KnightLM checkout).
    #[serde(default)]
    pub eval_repo: Option<PathBuf>,
    #[serde(default)]
    pub eval_out_root: Option<PathBuf>,
    /// Extra `model.eval.run` flags (e.g. `--no-stockfish`).
    #[serde(default)]
    pub eval_args: Vec<String>,
    /// `model://<model_name>@<alias>`; default name `knightlm-lora-<base>`.
    #[serde(default)]
    pub model_name: Option<String>,
    #[serde(default = "default_alias")]
    pub alias: String,
    /// Seconds without a heartbeat before another run is considered crashed.
    #[serde(default)]
    pub stale_after_secs: Option<u64>,
    #[serde(default)]
    pub heartbeat_secs: Option<u64>,
    #[serde(default)]
    pub notes: String,
}

fn default_run_root() -> PathBuf {
    DEFAULT_RUN_ROOT.into()
}
fn default_corpus() -> PathBuf {
    DEFAULT_CORPUS.into()
}
fn default_base() -> String {
    "smolvlm-500m".into()
}
fn default_rank() -> u32 {
    8
}
fn default_alpha() -> u32 {
    16
}
fn default_lr() -> f32 {
    2e-4
}
fn default_epochs() -> u32 {
    1
}
fn default_batch() -> u32 {
    8
}
fn default_accum() -> u32 {
    4
}
fn default_seq_len() -> u32 {
    2048
}
fn default_seed() -> u64 {
    42
}
fn default_scheduler() -> String {
    "cosine".into()
}
fn default_warmup() -> f64 {
    0.03
}
fn default_clip() -> f64 {
    1.0
}
fn default_save_steps() -> u32 {
    500
}
fn default_keep() -> u32 {
    3
}
fn default_true() -> bool {
    true
}
fn default_eval_rows() -> u64 {
    1000
}
fn default_alias() -> String {
    "candidate".into()
}

fn bad(field: &'static str, message: impl Into<String>) -> RecipeError {
    RecipeError::InvalidArgs {
        field: Some(field),
        message: message.into(),
    }
}

fn traversal_free(field: &'static str, p: &Path) -> Result<(), RecipeError> {
    if p.as_os_str().is_empty() {
        return Err(bad(field, "must be non-empty"));
    }
    if p.components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(bad(field, format!("'{}' contains '..'", p.display())));
    }
    Ok(())
}

fn safe_ident(field: &'static str, s: &str) -> Result<(), RecipeError> {
    if s.is_empty()
        || !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        || s.starts_with(['.', '-'])
        || s.contains("..")
    {
        return Err(bad(
            field,
            format!("'{s}' must be [A-Za-z0-9_.-], not starting with . or -"),
        ));
    }
    Ok(())
}

impl Recipe for KnightlmLora {
    type Backend = blut_backends::HfTrainerBackend;
    const NAME: &'static str = "knightlm_lora";
    const DESCRIPTION: &'static str = "KnightLM move-prediction LoRA: audit and materialize the v2 corpus with the serving prompt, \
         train an image-text LoRA (SmolVLM / Qwen3.5) with crash-gated resume, evaluate the \
         adapter with the external harness, and register it with a run manifest.";
    type Args = Args;
    const CATEGORY: blut::recipes::recipe::RecipeCategory =
        blut::recipes::recipe::RecipeCategory::Train;
    const INPUT_KINDS: &'static [&'static str] = &[];
    const OUTPUT_KIND: &'static str = "checkpoint.hf";

    fn compile(&self, args: Self::Args) -> Result<Plan<(), Self::Backend>, RecipeError> {
        let prof = profile::find(&args.base).ok_or_else(|| {
            bad(
                "base",
                format!(
                    "unknown base '{}'; known: {}",
                    args.base,
                    profile::keys().join(", ")
                ),
            )
        })?;
        traversal_free("corpus_dir", &args.corpus_dir)?;
        traversal_free("run_root", &args.run_root)?;
        if args.epochs == 0 || args.batch_size == 0 || args.grad_accum == 0 || args.seq_len == 0 {
            return Err(bad(
                "epochs",
                "epochs, batch_size, grad_accum, seq_len must all be > 0",
            ));
        }
        if args.rank == 0 || args.alpha == 0 {
            return Err(bad("rank", "rank and alpha must be > 0"));
        }
        if !(args.lr > 0.0 && args.lr.is_finite()) {
            return Err(bad(
                "lr",
                format!("must be positive finite; got {}", args.lr),
            ));
        }
        if args.eval_rows == 0 {
            return Err(bad("eval_rows", "must be > 0"));
        }
        // The backend validates the rest (scheduler, warmup, clip, ...) when the stage runs.
        let manifest = args.corpus_dir.join("manifest.json");
        let manifest_sha = sha256_file(&manifest).map_err(|e| {
            bad(
                "corpus_dir",
                format!("cannot read {}: {e}", manifest.display()),
            )
        })?;
        let run = args
            .run
            .clone()
            .unwrap_or_else(|| format!("{}-{}", prof.key.replace('.', "_"), &manifest_sha[..12]));
        safe_ident("run", &run)?;
        let model_name = args
            .model_name
            .clone()
            .unwrap_or_else(|| format!("knightlm-lora-{}", prof.key.replace('.', "_")));
        safe_ident("model_name", &model_name)?;
        safe_ident("alias", &args.alias)?;

        let run_dir = args.run_root.join(&run);
        let base_dir = prof.dir_or(args.base_dir.as_deref());
        let prompt_sha = sha256_file(&crate::runctx::repo_root().join("model/prompt.py"))
            .map_err(|e| bad("corpus_dir", format!("cannot read model/prompt.py: {e}")))?;
        let dataset_name = format!("knightlm-{}-{}", &manifest_sha[..12], &prompt_sha[..8]);

        let config = json!({
            "base": prof.key, "batch_size": args.batch_size, "grad_accum": args.grad_accum,
            "effective_batch": args.batch_size * args.grad_accum,
            "learning_rate": args.lr.to_string().parse::<f64>().unwrap_or(f64::from(args.lr)), "seed": args.seed, "epochs": args.epochs,
            "rank": args.rank, "alpha": args.alpha, "seq_len": args.seq_len,
            "lr_scheduler": args.lr_scheduler, "warmup_ratio": args.warmup_ratio,
            "max_grad_norm": args.max_grad_norm, "save_steps": args.save_steps,
            "keep_checkpoints": args.keep_checkpoints, "max_steps": args.max_steps,
            "shuffle_buffer": args.shuffle_buffer, "lora_target_regex_override": args.lora_target_regex,
            "arch": ARCH_IMAGE_TEXT,
        });
        let recipe_args = serde_json::to_value(&args)
            .map_err(|e| RecipeError::CompileFailed(format!("serialize args: {e}")))?;

        let train = TrainArgs {
            base_model: base_dir.display().to_string(),
            method: "lora".into(),
            rank: args.rank,
            alpha: args.alpha,
            lr: args.lr,
            epochs: args.epochs,
            batch_size: args.batch_size,
            grad_accum: args.grad_accum,
            seq_len: args.seq_len,
            seed: args.seed,
            eval_dataset_path: String::new(),
            extra: serde_json::Map::new(),
            nproc_per_node: 1,
            nnodes: 1,
            opts: ImageTextOpts {
                arch: ARCH_IMAGE_TEXT.into(),
                lora_target_regex: args
                    .lora_target_regex
                    .clone()
                    .or(prof.lora_regex.map(Into::into)),
                lr_scheduler: Some(args.lr_scheduler.clone()),
                warmup_ratio: Some(args.warmup_ratio),
                max_grad_norm: Some(args.max_grad_norm),
                save_steps: Some(args.save_steps),
                keep_checkpoints: Some(args.keep_checkpoints),
                // Always resume: a restart continues the run's own checkpoints,
                // and `claim_run` has already decided that is allowed.
                resume: true,
                dataset_shards: Vec::new(),
                shuffle_buffer: args.shuffle_buffer,
                max_steps: args.max_steps,
                logging_steps: args.logging_steps,
                base_sha256: prof
                    .trainer_checks_sha
                    .then(|| prof.weights_sha256.to_string()),
                checkpoint_root: Some(run_dir.join("checkpoints")),
                verify_reload: Some(args.verify_reload),
                adapter_path: None,
            },
        };

        let plan = Plan::<(), Self::Backend>::new(Self::NAME, recipe_args)
            .start(
                MaterializeCorpus,
                materialize_corpus::Args {
                    corpus_dir: args.corpus_dir.clone(),
                    corpus_manifest_sha256: manifest_sha.clone(),
                    run_dir: run_dir.clone(),
                },
            )
            .then(
                RegisterCorpus,
                RegDsArgs {
                    name: dataset_name,
                    kind: "sft".into(),
                    metadata: Some(
                        json!({"corpus_format": "knightlm-lichess-v2",
                               "corpus_manifest_sha256": manifest_sha,
                               "prompt_py_sha256": prompt_sha})
                        .to_string(),
                    ),
                },
            )
            .then(
                ClaimRun,
                claim_run::Args {
                    run_dir: run_dir.clone(),
                    run: run.clone(),
                    base_weights: base_dir.join(prof.weights_file),
                    stale_after_secs: args.stale_after_secs,
                    heartbeat_secs: args.heartbeat_secs,
                },
            )
            .then(HfSftTrain, train)
            .then(
                EvaluateCheckpoint,
                evaluate_checkpoint::Args {
                    run_dir: run_dir.clone(),
                    enabled: args.evaluate && prof.evaluable,
                    base_dir: base_dir.clone(),
                    corpus_dir: args.corpus_dir.clone(),
                    label: run.clone(),
                    rows: args.eval_rows,
                    baseline: args.baseline.clone(),
                    eval_repo: args.eval_repo.clone(),
                    out_root: args.eval_out_root.clone(),
                    extra: args.eval_args.clone(),
                },
            )
            .then(
                RegisterRun,
                register_run::Args {
                    run_dir,
                    run,
                    model_name,
                    alias: args.alias.clone(),
                    tenant: "knightlm".into(),
                    base_dir,
                    base_key: prof.key.into(),
                    base_repository: prof.repository.into(),
                    base_revision: prof.revision.into(),
                    weights_file: prof.weights_file.into(),
                    base_sha256: prof.weights_sha256.into(),
                    corpus_dir: args.corpus_dir.clone(),
                    config,
                    notes: args.notes.clone(),
                },
            )
            .finish();
        Ok(plan)
    }
}

blut::register_recipe!(KnightlmLora);

#[cfg(test)]
mod tests {
    use super::*;

    fn corpus() -> tempfile::TempDir {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("manifest.json"), b"{}").unwrap();
        td
    }

    fn args(td: &tempfile::TempDir) -> Args {
        serde_json::from_value(json!({"corpus_dir": td.path(), "run_root": td.path().join("runs")}))
            .unwrap()
    }

    #[test]
    fn compiles_to_a_six_node_plan() {
        let td = corpus();
        let plan = KnightlmLora.compile(args(&td)).unwrap().into_compiled();
        assert_eq!(plan.n_nodes(), 6);
        assert_eq!(plan.n_edges(), 5);
    }

    #[test]
    fn defaults_are_the_recommended_ones() {
        let td = corpus();
        let a = args(&td);
        assert_eq!((a.rank, a.alpha, a.batch_size, a.grad_accum), (8, 16, 8, 4));
        assert_eq!((a.save_steps, a.keep_checkpoints), (500, 3));
        assert_eq!(
            (a.lr_scheduler.as_str(), a.warmup_ratio, a.max_grad_norm),
            ("cosine", 0.03, 1.0)
        );
        assert_eq!(a.base, "smolvlm-500m");
    }

    #[test]
    fn qwen_is_a_parameter_not_a_fork() {
        let td = corpus();
        let mut a = args(&td);
        a.base = "qwen3_5-0.8b".into();
        assert!(KnightlmLora.compile(a).is_ok());
    }

    #[test]
    fn unknown_base_is_refused_with_the_known_list() {
        let td = corpus();
        let mut a = args(&td);
        a.base = "gpt".into();
        let e = KnightlmLora.compile(a).unwrap_err().to_string();
        assert!(
            e.contains("smolvlm-500m") && e.contains("qwen3_5-0.8b"),
            "{e}"
        );
    }

    #[test]
    fn missing_corpus_manifest_is_refused() {
        let td = tempfile::tempdir().unwrap();
        assert!(matches!(
            KnightlmLora.compile(args(&td)),
            Err(RecipeError::InvalidArgs {
                field: Some("corpus_dir"),
                ..
            })
        ));
    }

    #[test]
    fn bad_scalars_and_names_are_refused() {
        let td = corpus();
        for mutate in [
            (|a: &mut Args| a.epochs = 0) as fn(&mut Args),
            |a| a.lr = 0.0,
            |a| a.rank = 0,
            |a| a.eval_rows = 0,
            |a| a.run = Some("../escape".into()),
            |a| a.alias = "a/b".into(),
            |a| a.run_root = "/x/../y".into(),
        ] {
            let mut a = args(&td);
            mutate(&mut a);
            assert!(matches!(
                KnightlmLora.compile(a),
                Err(RecipeError::InvalidArgs { .. })
            ));
        }
    }

    #[test]
    fn recipe_is_registered_under_the_hf_backend() {
        assert_eq!(KnightlmLora::NAME, "knightlm_lora");
        assert_eq!(DEF.name, "knightlm_lora");
    }
}
