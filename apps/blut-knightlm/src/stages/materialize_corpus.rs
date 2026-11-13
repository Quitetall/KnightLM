//! Stage `materialize_corpus`: audited knightlm-lichess-v2 corpus ->
//! `{"prompt","completion"}` JSONL.
//!
//! The work is `python3 -m knightlm_blut.materialize`: it runs the corpus
//! audit (a failure refuses the stage), then converts every train shard, in
//! manifest order, with `model.prompt.user_text` (the prompt the model is also
//! served with) and the played UCI move. The converted rows are one
//! `dataset.jsonl` in the stage directory, which the engine owns, hashes and
//! restores from its content-addressed cache; the per-shard source and
//! converted hashes go to `receipt.json` and the run ledger.
//!
//! Resume: the converter re-hashes an existing `dataset.jsonl` against its
//! receipt and the corpus/prompt/converter pins and reuses it only on a match.
//! The pins are also in the stage arguments and `code_fingerprint`, so a
//! changed corpus, prompt or converter is a cache miss rather than a stale hit.

use std::path::PathBuf;

use async_trait::async_trait;
use blut::artifacts::DatasetJsonl;
use blut::framework::artifact::ContentHash;
use blut::framework::compat::Compatible;
use blut::framework::error::StageError;
use blut::framework::resource::Resource;
use blut::framework::stage::{Stage, StageContext};
use blut_backends::HfTrainerBackend;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::runctx::{io_err, last_json_line, ledger_append, payload_dir, repo_root, run_python};

pub struct MaterializeCorpus;
impl Compatible<HfTrainerBackend> for MaterializeCorpus {}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Args {
    pub corpus_dir: PathBuf,
    /// sha256 of `<corpus_dir>/manifest.json`, read when the recipe compiled.
    /// The converter refuses a corpus whose manifest differs, and it is part
    /// of the cache key.
    pub corpus_manifest_sha256: String,
    /// Run directory whose `ledger.jsonl` receives the per-shard hashes.
    pub run_dir: PathBuf,
}

/// The bytes that decide what this stage produces besides its arguments.
pub fn fingerprint_bytes() -> Vec<u8> {
    let mut h = Sha256::new();
    for path in [
        payload_dir().join("knightlm_blut/materialize.py"),
        payload_dir().join("knightlm_blut/common.py"),
        repo_root().join("model/prompt.py"),
    ] {
        h.update(std::fs::read(&path).unwrap_or_default());
    }
    h.finalize().to_vec()
}

#[async_trait]
impl Stage for MaterializeCorpus {
    const NAME: &'static str = "materialize_corpus";
    const SCHEMA: u32 = 1;
    const RESOURCES: &'static [Resource] = &[Resource::Disk];
    type Input = ();
    type Output = DatasetJsonl;
    type Args = Args;

    fn code_fingerprint(&self) -> Option<Vec<u8>> {
        Some(fingerprint_bytes())
    }

    async fn preflight(&self, args: &Args) -> Result<(), StageError> {
        let manifest = args.corpus_dir.join("manifest.json");
        if !manifest.is_file() {
            return Err(StageError::BadInput(format!(
                "no corpus manifest at {}",
                manifest.display()
            )));
        }
        Ok(())
    }

    async fn run(
        &self,
        ctx: &StageContext,
        _input: (),
        args: &Args,
    ) -> Result<DatasetJsonl, StageError> {
        let python = std::env::var_os("KNIGHTLM_PYTHON")
            .map_or_else(|| PathBuf::from("python3"), PathBuf::from);
        let out = run_python(
            &python,
            "knightlm_blut.materialize",
            &[
                "--corpus".into(),
                args.corpus_dir.display().to_string(),
                "--out".into(),
                ctx.stage_dir.display().to_string(),
                "--expect-manifest-sha256".into(),
                args.corpus_manifest_sha256.clone(),
            ],
        )
        .await?;
        let summary = last_json_line(&out)?;
        let rows = summary["rows"]
            .as_i64()
            .ok_or_else(|| StageError::BadInput("materializer reported no row count".into()))?;
        let dataset = ctx.stage_dir.join("dataset.jsonl");
        let content_hash = ContentHash::hash_file(&dataset).map_err(|e| io_err(&dataset, e))?;
        let receipt_path = ctx.stage_dir.join("receipt.json");
        let receipt: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&receipt_path).map_err(|e| io_err(&receipt_path, e))?,
        )
        .map_err(|e| StageError::BadInput(format!("receipt.json: {e}")))?;
        ledger_append(
            &args.run_dir,
            "materialize_corpus",
            "ok",
            json!({
                "reused": summary["reused"],
                "rows": rows,
                "dataset_sha256": receipt["dataset_sha256"],
                "corpus_manifest_sha256": receipt["corpus_manifest_sha256"],
                "prompt_py_sha256": receipt["prompt_py_sha256"],
                "materializer_sha256": receipt["materializer_sha256"],
                "shards": receipt["shards"],
            }),
        )?;
        Ok(DatasetJsonl {
            path: dataset,
            content_hash,
            n_examples: rows,
        })
    }
}
