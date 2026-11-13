//! Stage `evaluate_checkpoint`: score the produced adapter with the external
//! evaluation harness and put the scorecard's location in the run ledger.
//!
//! It runs `python -m knightlm_blut.evaluate`, which calls KnightLM's
//! `python -m model.eval.run --predictor knightlm_blut.predictor:factory`
//! (base weights + this run's adapter, loaded as the model is served) on the
//! Run 1 holdout, and, given a baseline scorecard directory, `python -m
//! model.eval.compare BASELINE NEW`. The harness writes
//! `eval-<label>-<UTC>/scorecard.json` under the artifact store; nothing here
//! reduces it to a single score.
//!
//! The harness is an external command: when it is not present in the checkout
//! the stage records `skipped` with the reason and passes the checkpoint on, so
//! training and registration still work. A harness that is present and fails
//! fails the stage.

use std::path::PathBuf;

use async_trait::async_trait;
use blut::artifacts::HfCheckpoint;
use blut::framework::compat::Compatible;
use blut::framework::error::StageError;
use blut::framework::resource::Resource;
use blut::framework::stage::{Stage, StageContext};
use blut_backends::HfTrainerBackend;
use blut_backends::backends::hf_trainer::venv::{IMAGE_TEXT_PROFILE, profile_root, python_path};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::runctx::{ledger_append, run_python};

pub struct EvaluateCheckpoint;
impl Compatible<HfTrainerBackend> for EvaluateCheckpoint {}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Args {
    pub run_dir: PathBuf,
    /// False for a base the serving predictor cannot load: record `skipped`.
    pub enabled: bool,
    pub base_dir: PathBuf,
    pub corpus_dir: PathBuf,
    pub label: String,
    /// First N holdout rows (manifest shard order).
    pub rows: u64,
    /// Scorecard directory of the model to compare against.
    #[serde(default)]
    pub baseline: Option<PathBuf>,
    /// Checkout that provides `model.eval` (default: this KnightLM checkout).
    #[serde(default)]
    pub eval_repo: Option<PathBuf>,
    /// Where the harness writes `eval-<label>-<UTC>/` (default: its keep dir).
    #[serde(default)]
    pub out_root: Option<PathBuf>,
    /// Extra `model.eval.run` flags, e.g. `["--no-stockfish", "--no-tactics"]`.
    #[serde(default)]
    pub extra: Vec<String>,
}

pub fn eval_python() -> PathBuf {
    std::env::var_os("KNIGHTLM_EVAL_PYTHON").map_or_else(
        || python_path(&profile_root(&IMAGE_TEXT_PROFILE)),
        PathBuf::from,
    )
}

#[async_trait]
impl Stage for EvaluateCheckpoint {
    const NAME: &'static str = "evaluate_checkpoint";
    const SCHEMA: u32 = 1;
    const RESOURCES: &'static [Resource] = &[Resource::Gpu];
    const DETERMINISTIC: bool = false;
    type Input = HfCheckpoint;
    type Output = HfCheckpoint;
    type Args = Args;

    async fn run(
        &self,
        _ctx: &StageContext,
        input: HfCheckpoint,
        args: &Args,
    ) -> Result<HfCheckpoint, StageError> {
        if !args.enabled {
            ledger_append(
                &args.run_dir,
                "evaluate_checkpoint",
                "skipped",
                json!({"reason": "evaluation disabled for this base (the serving predictor cannot load it)"}),
            )?;
            return Ok(input);
        }
        let mut cmd = vec![
            "--base-dir".to_string(),
            args.base_dir.display().to_string(),
            "--adapter".into(),
            input.path.display().to_string(),
            "--corpus".into(),
            args.corpus_dir.display().to_string(),
            "--label".into(),
            args.label.clone(),
            "--rows".into(),
            args.rows.to_string(),
        ];
        if let Some(b) = &args.baseline {
            cmd.extend(["--compare".into(), b.display().to_string()]);
        }
        if let Some(r) = &args.eval_repo {
            cmd.extend(["--eval-repo".into(), r.display().to_string()]);
        }
        if let Some(r) = &args.out_root {
            cmd.extend(["--out-root".into(), r.display().to_string()]);
        }
        if !args.extra.is_empty() {
            cmd.push("--".into());
            cmd.extend(args.extra.iter().cloned());
        }
        let result = match run_python(&eval_python(), "knightlm_blut.evaluate", &cmd).await {
            Ok(out) => crate::runctx::last_json_line(&out)?,
            Err(e) => {
                ledger_append(
                    &args.run_dir,
                    "evaluate_checkpoint",
                    "failed",
                    json!({"error": e.to_string()}),
                )?;
                return Err(e);
            }
        };
        let status = result["status"].as_str().unwrap_or("ok").to_string();
        ledger_append(&args.run_dir, "evaluate_checkpoint", &status, result)?;
        Ok(input)
    }
}
