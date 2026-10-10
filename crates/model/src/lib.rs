//! CPU infrastructure smoke and model admission contracts.
//! No pretrained backend is admitted by this scaffold.
pub mod smoke;

use anyhow::{Result, ensure};
use knightlm_core::{
    SCHEMA_VERSION,
    experiment::{
        Decision, ExperimentConfig, InferenceRequest, RunRecord, TokenCounter, prepare,
        score_decision,
    },
    state::{BoardObservation, GameState},
};
use serde::{Deserialize, Serialize};

pub trait ModelBackend {
    /// Must clear KV, recurrent, convolution and per-request tool state.
    fn reset(&mut self);
    fn observe(&mut self, image_id: &str, image: &[u8]) -> Result<BoardObservation>;
    fn generate(&mut self, request: &InferenceRequest) -> Result<Decision>;
}

pub fn two_pass(
    backend: &mut impl ModelBackend,
    tokenizer: &impl TokenCounter,
    state: &GameState,
    config: &ExperimentConfig,
    image_id: &str,
    image: &[u8],
) -> Result<RunRecord> {
    backend.reset();
    let observation = backend.observe(image_id, image);
    backend.reset(); // Also clear state after failed perception.
    let observation = observation?;
    ensure!(
        observation.image_id == image_id,
        "observation image identity mismatch"
    );
    state.verify_observation(&observation)?;
    let request = prepare(state, config, &[], tokenizer)?;
    let decision = backend.generate(&request);
    backend.reset();
    score_decision(state, config, &request, decision?)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelProfile {
    pub schema_version: u32,
    pub name: String,
    pub repository: String,
    pub revision: Option<String>,
    pub role: String,
    pub architecture: String,
}

#[derive(Debug, Serialize)]
pub struct AdmissionReport {
    pub schema_version: u32,
    pub repository: String,
    pub admitted: bool,
    pub missing: Vec<String>,
}

pub fn verify_model(profile: &ModelProfile) -> Result<AdmissionReport> {
    ensure!(
        profile.schema_version == SCHEMA_VERSION,
        "unsupported model profile"
    );
    ensure!(!profile.repository.is_empty(), "missing model repository");
    let mut missing = vec![
        "Rust pretrained architecture/importer is not implemented".into(),
        "processor and tokenizer reference parity".into(),
        "reference logits and complete inference-state reset parity".into(),
        "finite nonzero gradients, parameter updates and tiny-set overfit".into(),
        "checkpoint reload equivalence".into(),
    ];
    if profile
        .revision
        .as_ref()
        .is_none_or(|r| r.len() != 40 || !r.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        missing.insert(0, "immutable 40-hex model revision is not pinned".into());
    }
    Ok(AdmissionReport {
        schema_version: SCHEMA_VERSION,
        repository: profile.repository.clone(),
        admitted: false,
        missing,
    })
}
