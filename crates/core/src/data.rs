use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::BufRead;

use crate::{
    SCHEMA_VERSION,
    state::{GameRecord, GameState, PacketOptions},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationRow {
    pub fen: String,
    pub evals: Vec<EngineEvaluation>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineEvaluation {
    pub depth: u32,
    pub knodes: u64,
    pub pvs: Vec<PrincipalVariation>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrincipalVariation {
    pub cp: Option<i32>,
    pub mate: Option<i32>,
    /// Lichess export uses UCI_Chess960. Never use this as inference move syntax.
    pub line: String,
}

impl EvaluationRow {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.fen.split_whitespace().count() == 4,
            "evaluation export requires four-field FEN"
        );
        GameState::replay(&GameRecord {
            schema_version: SCHEMA_VERSION,
            game_id: "isolated-eval".into(),
            source: "lichess-evaluation".into(),
            initial_fen: format!("{} 0 1", self.fen),
            history_complete: false,
            moves: vec![],

            variant: "standard".into(),
        })?;
        ensure!(!self.evals.is_empty(), "empty engine evaluations");
        for eval in &self.evals {
            ensure!(
                eval.depth > 0 && !eval.pvs.is_empty(),
                "missing search depth or PV"
            );
            for pv in &eval.pvs {
                ensure!(
                    pv.cp.is_some() ^ pv.mate.is_some(),
                    "PV needs exactly one of cp or mate"
                );
                ensure!(!pv.line.trim().is_empty(), "empty principal variation");
                for mv in pv.line.split_whitespace() {
                    shakmaty::uci::UciMove::from_ascii(mv.as_bytes())?;
                }
            }
        }
        Ok(())
    }
    pub fn strongest(&self) -> Result<&EngineEvaluation> {
        self.validate()?;
        Ok(self
            .evals
            .iter()
            .max_by_key(|e| (e.depth, e.knodes))
            .unwrap())
    }
}

pub fn read_jsonl<T: serde::de::DeserializeOwned>(reader: impl BufRead) -> Result<Vec<T>> {
    let mut records = Vec::new();
    for (index, line) in reader.lines().enumerate() {
        let line = line?;
        ensure!(
            !line.trim().is_empty(),
            "blank JSONL record at line {}",
            index + 1
        );
        records.push(
            serde_json::from_str(&line)
                .map_err(|e| anyhow::anyhow!("JSONL line {}: {e}", index + 1))?,
        );
    }
    Ok(records)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusManifest {
    pub schema_version: u32,
    pub corpus_id: String,
    pub source: String,
    pub revision: String,
    pub usage_basis: String,
    pub track: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitItem {
    pub split: String,
    pub game_id: String,
    pub family_id: String,
    pub fen: String,
}

/// The FEN position key is recomputed rather than trusting caller-supplied hashes.
pub fn audit_splits(items: &[SplitItem]) -> Result<()> {
    let mut assignments: BTreeMap<String, String> = BTreeMap::new();
    for item in items {
        ensure!(
            ["train", "validation", "test"].contains(&item.split.as_str()),
            "unknown split"
        );
        ensure!(
            !item.game_id.is_empty() && !item.family_id.is_empty(),
            "missing split group"
        );
        let state = GameState::replay(&GameRecord {
            schema_version: SCHEMA_VERSION,
            game_id: item.game_id.clone(),
            source: "split-audit".into(),
            initial_fen: item.fen.clone(),
            history_complete: false,
            moves: vec![],

            variant: "standard".into(),
        })?;
        for key in [
            format!("game:{}", item.game_id),
            format!("family:{}", item.family_id),
            format!("position:{}", state.position_key()),
        ] {
            if let Some(existing) = assignments.insert(key.clone(), item.split.clone()) {
                ensure!(
                    existing == item.split,
                    "cross-split leakage for {key}: {existing} / {}",
                    item.split
                );
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticProbe {
    pub schema_version: u32,
    pub id: String,
    pub kind: String,
    pub input: ProbeState,
    pub question: String,
    /// Target is stored separately; inference_prompt never serializes this field.
    pub expected: String,
}

/// Unaided semantic probes omit all derived facts, including current check status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeState {
    pub fen: String,
    pub history_complete: bool,
    pub repetition_ledger: BTreeMap<String, u32>,
}

impl SemanticProbe {
    pub fn inference_prompt(&self) -> Result<String> {
        ensure!(
            self.schema_version == SCHEMA_VERSION,
            "unsupported probe schema"
        );
        Ok(format!(
            "{}\n{}",
            serde_json::to_string(&self.input)?,
            self.question
        ))
    }
}

pub fn generate_probes(state: &GameState) -> Result<Vec<SemanticProbe>> {
    let mut probes = Vec::new();
    let packet = state.compile(PacketOptions::default());
    let input = ProbeState {
        fen: packet.fen,
        history_complete: packet.history_complete,
        repetition_ledger: packet.repetition_ledger,
    };
    probes.push(SemanticProbe {
        schema_version: SCHEMA_VERSION,
        id: format!("{}:check", state.record().game_id),
        kind: "state_check".into(),
        input: input.clone(),
        question: "Is the side to move in check? Return true or false.".into(),
        expected: state.facts().in_check.to_string(),
    });
    if state.facts().terminal_reason.is_some() {
        return Ok(probes);
    }
    for mv in state.legal_moves() {
        let mut next = state.clone();
        next.play(&mv)?;
        probes.push(SemanticProbe {
            schema_version: SCHEMA_VERSION,
            id: format!("{}:effect:{mv}", state.record().game_id),
            kind: "move_effect_fen".into(),
            input: input.clone(),
            question: format!("After {mv}, what is the exact six-field FEN? Return only the FEN."),
            expected: next.fen(),
        });
        probes.push(SemanticProbe {
            schema_version: SCHEMA_VERSION,
            id: format!("{}:check_after:{mv}", state.record().game_id),
            kind: "counterfactual_check".into(),
            input: input.clone(),
            question: format!(
                "If {mv} is played, will the next side to move be in check? Return true or false."
            ),
            expected: next.facts().in_check.to_string(),
        });
        let side = if next.fen().split_whitespace().nth(1) == Some("w") {
            "white"
        } else {
            "black"
        };
        probes.push(SemanticProbe { schema_version: SCHEMA_VERSION,
            id: format!("{}:explain:{mv}", state.record().game_id), kind: "verified_explanation_template".into(),
            input: input.clone(), question: format!("Explain the exact state change after {mv}: report whose turn follows, whether that side is in check, and the resulting FEN."),
            expected: format!("After {mv}, {side} is to move. That side is {}in check. Resulting FEN: {}.",
                if next.facts().in_check { "" } else { "not " }, next.fen()) });
    }
    Ok(probes)
}

pub fn validate_manifests(manifests: &[CorpusManifest]) -> Result<()> {
    let mut ids = BTreeSet::new();
    for m in manifests {
        ensure!(
            m.schema_version == SCHEMA_VERSION,
            "unsupported corpus schema"
        );
        ensure!(
            !m.corpus_id.is_empty()
                && !m.source.is_empty()
                && !m.revision.is_empty()
                && !m.usage_basis.is_empty(),
            "incomplete corpus provenance"
        );
        ensure!(
            ["chess_teacher", "semantic", "vision", "general_language"].contains(&m.track.as_str()),
            "unknown corpus track"
        );
        ensure!(ids.insert(&m.corpus_id), "duplicate corpus id");
    }
    Ok(())
}
