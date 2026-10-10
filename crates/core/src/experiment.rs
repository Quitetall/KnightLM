use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use crate::{
    SCHEMA_VERSION,
    state::{GameState, PacketOptions},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arm {
    History,
    HistoryState,
    FreshState,
    LegalChoices,
    LegalMasking,
    DerivedFeatures,
    RuleTools,
    Combined,
}

impl Arm {
    pub const ALL: [Self; 8] = [
        Self::History,
        Self::HistoryState,
        Self::FreshState,
        Self::LegalChoices,
        Self::LegalMasking,
        Self::DerivedFeatures,
        Self::RuleTools,
        Self::Combined,
    ];
    pub fn tools(self) -> bool {
        matches!(self, Self::RuleTools | Self::Combined)
    }
    pub fn masking(self) -> bool {
        matches!(self, Self::LegalMasking | Self::Combined)
    }
    pub fn fresh(self) -> bool {
        !matches!(self, Self::History | Self::HistoryState)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentConfig {
    pub schema_version: u32,
    pub name: String,
    pub arm: Arm,
    pub checkpoint: String,
    pub seed: u64,
    pub max_input_tokens: usize,
    pub max_output_tokens: usize,
    pub tool_budget: usize,
    pub prior_model_text: bool,
}

impl ExperimentConfig {
    pub fn fixture(arm: Arm) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            name: format!("fixture-{arm:?}"),
            arm,
            checkpoint: "fixture-only:not-a-trained-model".into(),
            seed: 17,
            max_input_tokens: 8192,
            max_output_tokens: 1024,
            tool_budget: if arm.tools() { 8 } else { 0 },
            prior_model_text: false,
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == SCHEMA_VERSION,
            "unsupported experiment schema"
        );
        ensure!(
            !self.name.is_empty() && !self.checkpoint.is_empty(),
            "missing experiment identity"
        );
        ensure!(
            self.max_input_tokens > 0 && self.max_output_tokens > 0,
            "zero token budget"
        );
        ensure!(
            self.arm.tools() == (self.tool_budget > 0),
            "tool budget contradicts arm"
        );
        ensure!(
            !self.prior_model_text || !self.arm.fresh(),
            "fresh arms cannot retain prior model text"
        );
        Ok(())
    }
}

/// Tokens must come from the selected model tokenizer. No byte/word estimates.
pub trait TokenCounter {
    fn count(&self, prompt: &str) -> Result<usize>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InferenceRequest {
    pub schema_version: u32,
    pub arm: Arm,
    pub prompt: String,
    pub input_tokens: usize,
    pub max_output_tokens: usize,
    /// This is a decoder capability, not text exposed to the model.
    pub legal_constraint: Option<Vec<String>>,
    pub tool_budget: usize,
}

pub fn prepare(
    state: &GameState,
    config: &ExperimentConfig,
    prior_text: &[String],
    tokenizer: &impl TokenCounter,
) -> Result<InferenceRequest> {
    config.validate()?;
    ensure!(
        state.record().history_complete,
        "complete-state benchmark requires trusted complete history"
    );
    let mut prompt = String::from(
        "Play standard chess. Return one UCI move, explanation, and checkable claims.\n",
    );
    if !config.arm.fresh() {
        prompt.push_str("INITIAL_STATE\n");
        prompt.push_str(&state.record().initial_fen);
        prompt.push_str("\nPLAYED_MOVES\n");
        prompt.push_str(&state.record().moves.join(" "));
        if config.prior_model_text {
            prompt.push_str("\nPRIOR_MODEL_TEXT\n");
            for text in prior_text {
                prompt.push_str(text);
                prompt.push('\n');
            }
        }
    }
    if config.arm != Arm::History {
        let packet = state.compile(PacketOptions {
            legal_choices: matches!(config.arm, Arm::LegalChoices | Arm::Combined),
            derived_features: matches!(config.arm, Arm::DerivedFeatures | Arm::Combined),
        });
        prompt.push_str("\nCURRENT_STATE\n");
        prompt.push_str(&serde_json::to_string(&packet)?);
    }
    if config.arm.tools() {
        prompt.push_str("\nRULE_TOOL: simulate(branch, uci_move) returns legality and resulting rule state. Root branch is 0.\n");
    }
    let input_tokens = tokenizer.count(&prompt)?;
    ensure!(
        input_tokens <= config.max_input_tokens,
        "context overflow: {input_tokens} > {}; no truncation",
        config.max_input_tokens
    );
    Ok(InferenceRequest {
        schema_version: SCHEMA_VERSION,
        arm: config.arm,
        prompt,
        input_tokens,
        max_output_tokens: config.max_output_tokens,
        legal_constraint: config.arm.masking().then(|| state.legal_moves()),
        tool_budget: config.tool_budget,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FactClaim {
    LegalMove { uci: String, legal: bool },
    NextFen { uci: String, fen: String },
    InCheck { value: bool },
}

impl FactClaim {
    pub fn verify(&self, state: &GameState) -> bool {
        match self {
            Self::LegalMove { uci, legal } => state.is_legal(uci) == *legal,
            Self::NextFen { uci, fen } => {
                let mut next = state.clone();
                next.play(uci).is_ok() && next.fen() == *fen
            }
            Self::InCheck { value } => state.facts().in_check == *value,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub schema_version: u32,
    pub uci_move: String,
    /// A separately executed unmasked diagnostic, not a masked prefix's argmax.
    pub unmasked_uci_move: Option<String>,
    pub explanation: String,
    pub claims: Vec<FactClaim>,
    pub output_tokens: usize,
    pub diagnostic_output_tokens: usize,
    pub tool_calls: usize,
    pub interventions: usize,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRecord {
    pub schema_version: u32,
    pub evidence: String,
    pub game_id: String,
    pub game: crate::state::GameRecord,
    pub config: ExperimentConfig,
    pub state_fen: String,
    pub raw_legal: bool,
    pub selected_legal: bool,
    pub claims_correct: usize,
    pub claims_total: usize,
    pub explanation_scored: bool,
    pub input_tokens: usize,
    pub decision: Decision,
    pub request: InferenceRequest,
}

/// Called only after the decision is frozen. No engine outputs are input fields.
pub fn score_decision(
    state: &GameState,
    config: &ExperimentConfig,
    request: &InferenceRequest,
    decision: Decision,
) -> Result<RunRecord> {
    config.validate()?;
    ensure!(
        decision.schema_version == SCHEMA_VERSION && request.schema_version == SCHEMA_VERSION,
        "unsupported decision/request schema"
    );
    ensure!(
        request.arm == config.arm
            && request.tool_budget == config.tool_budget
            && request.max_output_tokens == config.max_output_tokens,
        "request/config mismatch"
    );
    ensure!(
        request.input_tokens <= config.max_input_tokens,
        "input token budget exceeded"
    );
    ensure!(
        decision.output_tokens <= config.max_output_tokens,
        "output token budget exceeded"
    );
    ensure!(
        decision.tool_calls <= config.tool_budget,
        "rule-tool budget exceeded"
    );
    ensure!(
        config.arm.masking() || decision.interventions == 0,
        "hidden intervention in unmasked arm"
    );
    ensure!(
        config.arm.masking() || decision.unmasked_uci_move.is_none(),
        "unexpected shadow decode"
    );
    if config.arm.masking() {
        ensure!(
            decision.unmasked_uci_move.is_some(),
            "masked result requires separate unmasked diagnostic"
        );
        ensure!(
            decision.diagnostic_output_tokens > 0
                && decision.diagnostic_output_tokens <= config.max_output_tokens,
            "invalid diagnostic token accounting"
        );
        ensure!(
            state.is_legal(&decision.uci_move),
            "legal mask emitted illegal move"
        );
    } else {
        ensure!(
            decision.diagnostic_output_tokens == 0,
            "unexpected diagnostic token charge"
        );
    }
    let raw = decision
        .unmasked_uci_move
        .as_deref()
        .unwrap_or(&decision.uci_move);
    Ok(RunRecord {
        schema_version: SCHEMA_VERSION,
        evidence: if config.checkpoint.starts_with("fixture-only:") {
            "fixture_only"
        } else {
            "unverified_external_output"
        }
        .into(),
        game_id: state.record().game_id.clone(),
        game: state.record().clone(),
        config: config.clone(),
        state_fen: state.fen(),
        raw_legal: state.is_legal(raw),
        selected_legal: state.is_legal(&decision.uci_move),
        claims_correct: decision.claims.iter().filter(|c| c.verify(state)).count(),
        claims_total: decision.claims.len(),
        explanation_scored: false,
        input_tokens: request.input_tokens,
        request: request.clone(),
        decision,
    })
}

/// Character tokenizer is intentionally fixture-only; it must never stand in for
/// a pretrained model tokenizer or support a research token-efficiency claim.
pub struct FixtureTokenizer;
impl TokenCounter for FixtureTokenizer {
    fn count(&self, prompt: &str) -> Result<usize> {
        Ok(prompt.chars().count())
    }
}
