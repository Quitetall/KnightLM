//! Stage `register_run`: keep the adapter, write the KnightLM run manifest and
//! register the adapter under a `model://` handle.
//!
//! The standard `register_model` stage takes a GGUF and writes the llama.cpp
//! registry; a PEFT adapter on an image-text base has no GGUF form (the
//! serving path loads base + adapter), so the adapter is registered in BLUT's
//! model registry (`blut::model_registry`) by the sha256 of its weights, and
//! `model://<name>@<alias>` points at it. The alias defaults to `candidate`;
//! governed aliases such as `prod` are refused by the registry itself.
//!
//! `run-manifest.json` carries the fields of the pilot's receipt
//! (`lichess-lora-dry-run-2026-11-07`) with the same names, plus the prompt,
//! materializer, harness and scorecard identities. Fields this path does not
//! measure (holdout loss before/after) are `null`, not omitted or invented.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use blut::artifacts::HfCheckpoint;
use blut::framework::compat::Compatible;
use blut::framework::error::StageError;
use blut::framework::resource::Resource;
use blut::framework::stage::{Stage, StageContext};
use blut_backends::HfTrainerBackend;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::runctx::{io_err, ledger_append, now_unix, repo_root, sha256_file};
use crate::stages::claim_run::mark_finished;

pub struct RegisterRun;
impl Compatible<HfTrainerBackend> for RegisterRun {}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Args {
    pub run_dir: PathBuf,
    pub run: String,
    pub model_name: String,
    pub alias: String,
    pub tenant: String,
    pub base_dir: PathBuf,
    pub base_key: String,
    pub base_repository: String,
    pub base_revision: String,
    pub weights_file: String,
    pub base_sha256: String,
    pub corpus_dir: PathBuf,
    /// The training configuration as the recipe resolved it (defaults filled).
    pub config: Value,
    #[serde(default)]
    pub notes: String,
}

fn file_sha(p: &Path) -> Value {
    sha256_file(p).map_or(Value::Null, Value::String)
}

/// The last ledger record for `stage`, if any.
pub fn last_record(run_dir: &Path, stage: &str) -> Option<Value> {
    let body = std::fs::read_to_string(run_dir.join("ledger.jsonl")).ok()?;
    body.lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .rfind(|v| v["stage"] == stage)
}

/// SmolVLM keeps its template in chat_template.json; others in the tokenizer
/// config or a .jinja file. Null when no separate file exists.
fn chat_template_sha(dir: &Path) -> Value {
    ["chat_template.json", "chat_template.jinja"]
        .iter()
        .map(|f| dir.join(f))
        .find(|p| p.is_file())
        .map_or(Value::Null, |p| file_sha(&p))
}

fn gpu_name() -> Value {
    std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.lines().next().map(|l| Value::String(l.trim().into())))
        .unwrap_or(Value::Null)
}

pub fn build_manifest(args: &Args, adapter: &Path, summary: &Value) -> Value {
    let repo = repo_root();
    let eval = last_record(&args.run_dir, "evaluate_checkpoint");
    let eval_detail = eval
        .as_ref()
        .map(|r| r["detail"].clone())
        .unwrap_or(Value::Null);
    let scorecard = eval_detail["scorecard"].as_str().map(PathBuf::from);
    let predictions = scorecard
        .as_ref()
        .and_then(|s| s.parent())
        .map(|d| d.join("predictions.jsonl"));
    let mat = last_record(&args.run_dir, "materialize_corpus")
        .map(|r| r["detail"].clone())
        .unwrap_or(Value::Null);
    let steps = summary["steps"].as_u64().unwrap_or(0);
    let max_norm = summary["max_grad_norm_seen"].as_f64().unwrap_or(0.0);
    json!({
        "evidence": "blut_knightlm_lora",
        "run": args.run,
        "registered_utc": now_unix(),
        "model": {
            "key": args.base_key,
            "repository": args.base_repository,
            "revision": args.base_revision,
            "base_sha256": args.base_sha256,
            "weights_file": args.weights_file,
            "chat_template_sha256": chat_template_sha(&args.base_dir),
            "tokenizer_sha256": file_sha(&args.base_dir.join("tokenizer.json")),
        },
        "corpus_manifest_sha256": file_sha(&args.corpus_dir.join("manifest.json")),
        "prompt_py_sha256": file_sha(&repo.join("model/prompt.py")),
        "materializer_sha256": mat["materializer_sha256"],
        "materialized_dataset_sha256": mat["dataset_sha256"],
        "materialized_shards": mat["shards"],
        // The reference trainer the BLUT path mirrors, and the evaluation harness.
        "trainer_sha256": file_sha(&repo.join("model/train/harness.py")),
        "harness_sha256": eval_detail["harness_sha256"].clone(),
        "adapter_sha256": file_sha(&adapter.join("adapter_model.safetensors")),
        "adapter_config_sha256": file_sha(&adapter.join("adapter_config.json")),
        "holdout_predictions_sha256": predictions.as_ref().map_or(Value::Null, |p| file_sha(p)),
        "scorecard": eval_detail["scorecard"],
        "scorecard_sha256": scorecard.as_ref().map_or(Value::Null, |p| file_sha(p)),
        "baseline_scorecard_dir": eval_detail["baseline"],
        "evaluation_status": eval.as_ref().map_or(Value::Null, |r| r["status"].clone()),
        "environment": {
            "gpu": gpu_name(),
            "torch": summary["torch"],
            "transformers": summary["transformers"],
            "peft": summary["peft"],
        },
        "config": args.config,
        "optimizer_steps": steps,
        "rows_consumed": summary["rows_consumed"],
        "dropped_long_rows": summary["dropped_long"],
        "trainable_parameters": summary["trainable_parameters"],
        "max_gradient_norm": max_norm,
        "adapter_updated": steps > 0 && max_norm > 0.0,
        "adapter_reload_max_logit_error": summary["adapter_reload_max_logit_error"],
        "lora_target_regex": summary["lora_target_regex"],
        "holdout_loss_before": null,
        "holdout_loss_after": null,
        "peak_gpu_allocated_bytes": summary["peak_gpu_allocated_bytes"],
        "cookbook_standard": "blut-cookbook-standard (blut-cookbooks PR #3 image_text)",
    })
}

fn copy_adapter(from: &Path, to: &Path) -> Result<(), StageError> {
    std::fs::create_dir_all(to).map_err(|e| io_err(to, e))?;
    for name in [
        "adapter_model.safetensors",
        "adapter_config.json",
        "train_summary.json",
    ] {
        let src = from.join(name);
        if src.is_file() {
            std::fs::copy(&src, to.join(name)).map_err(|e| io_err(&src, e))?;
        }
    }
    Ok(())
}

#[async_trait]
impl Stage for RegisterRun {
    const NAME: &'static str = "register_run";
    const SCHEMA: u32 = 1;
    const RESOURCES: &'static [Resource] = &[Resource::Disk];
    const DETERMINISTIC: bool = false;
    type Input = HfCheckpoint;
    type Output = HfCheckpoint;
    type Args = Args;

    async fn run(
        &self,
        ctx: &StageContext,
        input: HfCheckpoint,
        args: &Args,
    ) -> Result<HfCheckpoint, StageError> {
        let weights = input.path.join("adapter_model.safetensors");
        if !weights.is_file() {
            return Err(StageError::BadInput(format!(
                "no adapter_model.safetensors in {}",
                input.path.display()
            )));
        }
        let kept = args.run_dir.join("adapter");
        copy_adapter(&input.path, &kept)?;
        let summary: Value = std::fs::read(kept.join("train_summary.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or(Value::Null);
        let manifest = build_manifest(args, &kept, &summary);
        let manifest_path = args.run_dir.join("run-manifest.json");
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).map_err(|e| StageError::Backend(e.into()))?,
        )
        .map_err(|e| io_err(&manifest_path, e))?;

        let adapter_sha =
            sha256_file(&kept.join("adapter_model.safetensors")).map_err(|e| io_err(&kept, e))?;
        let handle = format!("model://{}@{}", args.model_name, args.alias);
        let source = format!("{}#{}", manifest_path.display(), args.run);
        let (name, alias, tenant, hash) = (
            args.model_name.clone(),
            args.alias.clone(),
            args.tenant.clone(),
            adapter_sha.clone(),
        );
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let mut conn = blut::model_registry::open().map_err(|e| e.to_string())?;
            let now = now_unix() as i64;
            blut::model_registry::register(&conn, &hash, &name, &tenant, Some(&source), now)
                .map_err(|e| e.to_string())?;
            blut::model_registry::promote(&mut conn, &hash, &tenant, &name, &alias, now)
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| StageError::Backend(e.into()))?
        .map_err(|e| StageError::BadInput(format!("model registry: {e}")))?;

        ledger_append(
            &args.run_dir,
            "register_run",
            "ok",
            json!({"handle": handle, "adapter_sha256": adapter_sha,
                   "run_manifest": manifest_path, "adapter": kept}),
        )?;
        let job_id = ctx
            .job_dir
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        mark_finished(&args.run_dir, &job_id)?;
        Ok(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_has_the_pilot_receipt_fields() {
        let td = tempfile::tempdir().unwrap();
        let args = Args {
            run_dir: td.path().into(),
            run: "r".into(),
            model_name: "m".into(),
            alias: "candidate".into(),
            tenant: "knightlm".into(),
            base_dir: td.path().into(),
            base_key: "smolvlm-500m".into(),
            base_repository: "repo".into(),
            base_revision: "rev".into(),
            weights_file: "model.safetensors".into(),
            base_sha256: "ab".into(),
            corpus_dir: td.path().into(),
            config: json!({"rank": 8}),
            notes: String::new(),
        };
        let m = build_manifest(
            &args,
            td.path(),
            &json!({"steps": 3, "max_grad_norm_seen": 1.5}),
        );
        // Names from lichess-lora-dry-run-2026-11-07/run/run-manifest.json.
        for key in [
            "evidence",
            "model",
            "corpus_manifest_sha256",
            "trainer_sha256",
            "adapter_sha256",
            "adapter_config_sha256",
            "holdout_predictions_sha256",
            "environment",
            "config",
            "optimizer_steps",
            "trainable_parameters",
            "max_gradient_norm",
            "adapter_updated",
            "adapter_reload_max_logit_error",
            "holdout_loss_before",
            "holdout_loss_after",
            "peak_gpu_allocated_bytes",
        ] {
            assert!(m.get(key).is_some(), "missing {key}");
        }
        assert_eq!(m["optimizer_steps"], 3);
        assert_eq!(m["adapter_updated"], true);
        assert!(m["holdout_loss_before"].is_null());
    }

    #[test]
    fn last_record_picks_the_newest_for_a_stage() {
        let td = tempfile::tempdir().unwrap();
        ledger_append(td.path(), "evaluate_checkpoint", "failed", json!({"n": 1})).unwrap();
        ledger_append(td.path(), "evaluate_checkpoint", "ok", json!({"n": 2})).unwrap();
        ledger_append(td.path(), "other", "ok", json!({})).unwrap();
        let r = last_record(td.path(), "evaluate_checkpoint").unwrap();
        assert_eq!(r["detail"]["n"], 2);
        assert!(last_record(td.path(), "absent").is_none());
    }
}
