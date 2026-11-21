//! DRAFT, not wired into a recipe and never run: the two stages of the board-encoder
//! arms (T4/T5), `pretrain_board_encoder` then `align_projector`.
//! Spec and the exact commands: `docs/model/board-encoder.md`.
//!
//! * `pretrain_board_encoder` runs `python -m model.encoder.pretrain`: engine-free
//!   rules-QA pretraining of the small encoder on positions from the Run 1 corpus,
//!   labels from `knightlm board-labels` (shakmaty). It writes `encoder/` (weights and
//!   `config.json`) and `axis.json` (per-head held-out accuracy, no composite) into
//!   the stage directory.
//! * `align_projector` runs `python -m model.encoder.align`: the host LLM and the
//!   encoder stay frozen and only the per-host projector trains, with soft tokens in the
//!   host's own `image_hidden_states` slot.
//!
//! Both are GPU stages; neither is registered with `KnightlmCookbook`, so nothing
//! can launch them until a recipe wires them and a person says go.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use blut::framework::artifact::{Artifact, ContentHash};
use blut::framework::compat::Compatible;
use blut::framework::error::StageError;
use blut::framework::resource::Resource;
use blut::framework::stage::{Stage, StageContext};
use blut_backends::HfTrainerBackend;
use serde::{Deserialize, Serialize};

use crate::runctx::{io_err, last_json_line, ledger_append, run_python};

/// A pretrained board encoder directory (`encoder.pt`, `config.json`, `heads.pt`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BoardEncoderDir {
    pub path: PathBuf,
    pub content_hash: ContentHash,
    /// `axis.json` of the held-out rules-QA evaluation, one record per head.
    pub axis: serde_json::Value,
}

impl Artifact for BoardEncoderDir {
    const KIND: &'static str = "knightlm.board_encoder";
    const SCHEMA: u32 = 1;
    const ALLOW_EXTERNAL_PATHS: bool = true;
    fn content_hash(&self) -> ContentHash {
        self.content_hash
    }
    fn primary_path(&self) -> &Path {
        &self.path
    }
}

/// A trained projector directory (`projector.pt`, `projector.json`) for one host.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectorDir {
    pub path: PathBuf,
    pub content_hash: ContentHash,
    pub host: String,
    pub final_loss: f32,
}

impl Artifact for ProjectorDir {
    const KIND: &'static str = "knightlm.board_projector";
    const SCHEMA: u32 = 1;
    const ALLOW_EXTERNAL_PATHS: bool = true;
    fn content_hash(&self) -> ContentHash {
        self.content_hash
    }
    fn primary_path(&self) -> &Path {
        &self.path
    }
}

fn python() -> PathBuf {
    std::env::var_os("KNIGHTLM_PYTHON").map_or_else(|| PathBuf::from("python3"), PathBuf::from)
}

pub struct PretrainBoardEncoder;
impl Compatible<HfTrainerBackend> for PretrainBoardEncoder {}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PretrainArgs {
    pub run_dir: PathBuf,
    /// Run 1 corpus directory (positions only; labels come from the compiler).
    pub corpus_dir: PathBuf,
    /// The `knightlm` binary that provides `board-labels`.
    pub knightlm_bin: PathBuf,
    pub train_rows: u64,
    pub holdout_rows: u64,
    pub epochs: u32,
    pub batch: u32,
    #[serde(default)]
    pub n_global: u32,
}

#[async_trait]
impl Stage for PretrainBoardEncoder {
    const NAME: &'static str = "pretrain_board_encoder";
    const SCHEMA: u32 = 1;
    const RESOURCES: &'static [Resource] = &[Resource::Gpu];
    const DETERMINISTIC: bool = false;
    type Input = ();
    type Output = BoardEncoderDir;
    type Args = PretrainArgs;

    async fn run(
        &self,
        ctx: &StageContext,
        _input: (),
        args: &PretrainArgs,
    ) -> Result<BoardEncoderDir, StageError> {
        let out = run_python(
            &python(),
            "model.encoder.pretrain",
            &[
                "--corpus".into(),
                args.corpus_dir.display().to_string(),
                "--out".into(),
                ctx.stage_dir.display().to_string(),
                "--knightlm".into(),
                args.knightlm_bin.display().to_string(),
                "--train-rows".into(),
                args.train_rows.to_string(),
                "--holdout-rows".into(),
                args.holdout_rows.to_string(),
                "--epochs".into(),
                args.epochs.to_string(),
                "--batch".into(),
                args.batch.to_string(),
                "--n-global".into(),
                args.n_global.to_string(),
                "--device".into(),
                "cuda".into(),
            ],
        )
        .await?;
        let axis = last_json_line(&out)?;
        let dir = ctx.stage_dir.join("encoder");
        let content_hash =
            ContentHash::hash_file(&dir.join("encoder.pt")).map_err(|e| io_err(&dir, e))?;
        ledger_append(&args.run_dir, "pretrain_board_encoder", "ok", axis.clone())?;
        Ok(BoardEncoderDir {
            path: dir,
            content_hash,
            axis,
        })
    }
}

pub struct AlignProjector;
impl Compatible<HfTrainerBackend> for AlignProjector {}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AlignArgs {
    pub run_dir: PathBuf,
    /// Host snapshot (a base key from `profile.rs`, resolved by the recipe).
    pub host_dir: PathBuf,
    pub host_key: String,
    /// Alignment rows: JSONL `{fen, repetition?, prompt, completion}` (the S0 rules-QA corpus).
    pub rows: PathBuf,
    pub steps: u32,
    pub batch: u32,
}

#[async_trait]
impl Stage for AlignProjector {
    const NAME: &'static str = "align_projector";
    const SCHEMA: u32 = 1;
    const RESOURCES: &'static [Resource] = &[Resource::Gpu];
    const DETERMINISTIC: bool = false;
    type Input = BoardEncoderDir;
    type Output = ProjectorDir;
    type Args = AlignArgs;

    async fn run(
        &self,
        ctx: &StageContext,
        input: BoardEncoderDir,
        args: &AlignArgs,
    ) -> Result<ProjectorDir, StageError> {
        let out = run_python(
            &python(),
            "model.encoder.align",
            &[
                "--host".into(),
                args.host_dir.display().to_string(),
                "--encoder".into(),
                input.path.display().to_string(),
                "--rows".into(),
                args.rows.display().to_string(),
                "--out".into(),
                ctx.stage_dir.display().to_string(),
                "--steps".into(),
                args.steps.to_string(),
                "--batch".into(),
                args.batch.to_string(),
                "--device".into(),
                "cuda".into(),
            ],
        )
        .await?;
        let summary = last_json_line(&out)?;
        let final_loss = summary["final_loss"]
            .as_f64()
            .ok_or_else(|| StageError::BadInput("align reported no final_loss".into()))?
            as f32;
        let weights = ctx.stage_dir.join("projector.pt");
        let content_hash = ContentHash::hash_file(&weights).map_err(|e| io_err(&weights, e))?;
        ledger_append(&args.run_dir, "align_projector", "ok", summary)?;
        Ok(ProjectorDir {
            path: ctx.stage_dir.clone(),
            content_hash,
            host: args.host_key.clone(),
            final_loss,
        })
    }
}
