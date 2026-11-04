//! Evidence the compiler receives from `rbc-ai` or the CLI.
//!
//! The compiler never fetches evidence. Each populated variant contributes a
//! fingerprint to the compiled packet, so a changed engine line, database row
//! or annotation changes the context hash even when the prose happens to render
//! identically.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::annotations::Annotations;
use crate::canonical::fingerprint;

/// A pinned engine line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineLine {
    pub rank: u32,
    /// Centipawn score from the side to move.
    pub score_cp: Option<i32>,
    /// Mate distance from the side to move, if the line is a forced mate.
    pub mate: Option<i32>,
    /// Principal variation as UCI moves.
    pub pv: Vec<String>,
}

impl Default for EngineLine {
    fn default() -> Self {
        Self {
            rank: 1,
            score_cp: None,
            mate: None,
            pv: Vec::new(),
        }
    }
}

/// Engine identity plus the lines the caller wants pinned into the context.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineEvidence {
    pub name: String,
    pub binary_sha256: String,
    pub depth: u32,
    pub threads: u32,
    pub lines: Vec<EngineLine>,
}

impl EngineEvidence {
    /// Canonical copy with lines sorted by rank, then PV, then score.
    pub fn normalized(&self) -> Self {
        let mut lines = self.lines.clone();
        lines.sort_by(|a, b| {
            a.rank
                .cmp(&b.rank)
                .then_with(|| a.pv.cmp(&b.pv))
                .then_with(|| a.score_cp.cmp(&b.score_cp))
                .then_with(|| a.mate.cmp(&b.mate))
        });
        Self {
            lines,
            ..self.clone()
        }
    }
}

/// One opening-book or master-database row.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpeningRow {
    pub move_san: String,
    pub games: u64,
    pub white_wins: u64,
    pub draws: u64,
    pub black_wins: u64,
    /// Optional caller-supplied percentage. Rendered with two decimals.
    pub win_pct: Option<f64>,
}

/// Opening statistics and the query that produced them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpeningsEvidence {
    pub source: String,
    pub query: String,
    pub rows: Vec<OpeningRow>,
}

impl OpeningsEvidence {
    /// Canonical copy with rows sorted by move, then games.
    pub fn normalized(&self) -> Self {
        let mut rows = self.rows.clone();
        rows.sort_by(|a, b| {
            a.move_san
                .cmp(&b.move_san)
                .then_with(|| b.games.cmp(&a.games))
                .then_with(|| a.white_wins.cmp(&b.white_wins))
        });
        Self {
            rows,
            ..self.clone()
        }
    }
}

/// One game in a database sample.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseGame {
    pub id: String,
    pub white: String,
    pub black: String,
    pub result: String,
    pub date: Option<String>,
}

/// Search aggregates plus a deterministic sample.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseEvidence {
    pub query: String,
    pub total_games: u64,
    pub white_wins: u64,
    pub draws: u64,
    pub black_wins: u64,
    pub sample: Vec<DatabaseGame>,
}

impl DatabaseEvidence {
    /// Canonical copy with the sample sorted by game identity.
    pub fn normalized(&self) -> Self {
        let mut sample = self.sample.clone();
        sample.sort_by(|a, b| {
            a.id.cmp(&b.id)
                .then_with(|| a.white.cmp(&b.white))
                .then_with(|| a.black.cmp(&b.black))
        });
        Self {
            sample,
            ..self.clone()
        }
    }
}

/// A compact reference to another board.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BoardSummary {
    pub id: String,
    pub title: String,
    pub fen: String,
    pub ply: Option<u32>,
    pub note: Option<String>,
}

/// All optional evidence. Absent variants omit their section; they are never
/// guessed or filled from ambient state.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Evidence {
    pub engine: Option<EngineEvidence>,
    pub openings: Option<OpeningsEvidence>,
    pub database: Option<DatabaseEvidence>,
    pub annotations: Option<Annotations>,
    pub notes: Option<String>,
    pub siblings: Vec<BoardSummary>,
    pub compare: Vec<BoardSummary>,
}

impl Evidence {
    /// Canonical copy. Arrays are sorted by their fixed key so two callers that
    /// supply the same set in different order produce the same bytes.
    pub fn normalized(&self) -> Self {
        Self {
            engine: self.engine.as_ref().map(EngineEvidence::normalized),
            openings: self.openings.as_ref().map(OpeningsEvidence::normalized),
            database: self.database.as_ref().map(DatabaseEvidence::normalized),
            annotations: self.annotations.as_ref().map(Annotations::sorted),
            notes: self.notes.clone(),
            siblings: sort_boards(&self.siblings),
            compare: sort_boards(&self.compare),
        }
    }

    /// One fingerprint per populated evidence variant. Keys are stable section
    /// ids; the map is ordered so its canonical JSON is deterministic.
    pub fn fingerprints(&self) -> Result<BTreeMap<&'static str, String>> {
        let normalized = self.normalized();
        let mut out = BTreeMap::new();
        if let Some(engine) = &normalized.engine {
            out.insert("engine", fingerprint(engine)?);
        }
        if let Some(openings) = &normalized.openings {
            out.insert("openings", fingerprint(openings)?);
        }
        if let Some(database) = &normalized.database {
            out.insert("database", fingerprint(database)?);
        }
        if let Some(annotations) = &normalized.annotations {
            out.insert("annotations", fingerprint(annotations)?);
        }
        if let Some(notes) = &normalized.notes {
            out.insert("notes", fingerprint(notes)?);
        }
        if !normalized.siblings.is_empty() {
            out.insert("siblings", fingerprint(&normalized.siblings)?);
        }
        if !normalized.compare.is_empty() {
            out.insert("compare", fingerprint(&normalized.compare)?);
        }
        Ok(out)
    }
}

fn sort_boards(boards: &[BoardSummary]) -> Vec<BoardSummary> {
    let mut sorted = boards.to_vec();
    sorted.sort_by(|a, b| {
        a.id.cmp(&b.id)
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.fen.cmp(&b.fen))
            .then_with(|| a.ply.cmp(&b.ply))
            .then_with(|| a.note.cmp(&b.note))
    });
    sorted
}
