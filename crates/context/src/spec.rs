//! Context specification: which sections are enabled, how large the packet may
//! be, and which tokenizer measures it.
//!
//! A spec is data, not behaviour. The compiler fixes section order from
//! [`SectionId::ALL`]; the order of [`ContextSpec::sections`] is irrelevant.

use serde::{Deserialize, Serialize};

/// Version of the prompt template embedded in the `system` section.
pub const PROMPT_TEMPLATE_VERSION: &str = "rbc-prompt-v1";

/// Schema identifier emitted by [`crate::compile`].
pub const SCHEMA: &str = "rbc-context/1";

/// The only tokenizer implemented in this crate.
pub const TOKENIZER_ESTIMATE_V1: &str = "estimate-v1";

/// Stable section identifiers. The declaration order is the canonical order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionId {
    System,
    Position,
    Ascii,
    LegalMoves,
    History,
    Annotations,
    Engine,
    Openings,
    Database,
    Notes,
    Siblings,
    Compare,
    Turns,
}

impl SectionId {
    /// Fixed canonical order. No other order is ever used.
    pub const ALL: [SectionId; 13] = [
        SectionId::System,
        SectionId::Position,
        SectionId::Ascii,
        SectionId::LegalMoves,
        SectionId::History,
        SectionId::Annotations,
        SectionId::Engine,
        SectionId::Openings,
        SectionId::Database,
        SectionId::Notes,
        SectionId::Siblings,
        SectionId::Compare,
        SectionId::Turns,
    ];

    /// Stable wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            SectionId::System => "system",
            SectionId::Position => "position",
            SectionId::Ascii => "ascii",
            SectionId::LegalMoves => "legal_moves",
            SectionId::History => "history",
            SectionId::Annotations => "annotations",
            SectionId::Engine => "engine",
            SectionId::Openings => "openings",
            SectionId::Database => "database",
            SectionId::Notes => "notes",
            SectionId::Siblings => "siblings",
            SectionId::Compare => "compare",
            SectionId::Turns => "turns",
        }
    }

    /// Lower priority is kept first when the budget is tight.
    pub const fn priority(self) -> u8 {
        match self {
            SectionId::System => 10,
            SectionId::Position => 30,
            SectionId::Ascii => 40,
            SectionId::LegalMoves => 50,
            SectionId::History => 60,
            SectionId::Annotations => 70,
            SectionId::Engine => 80,
            SectionId::Openings => 90,
            SectionId::Database => 100,
            SectionId::Notes => 120,
            SectionId::Siblings => 130,
            SectionId::Compare => 140,
            SectionId::Turns => 150,
        }
    }

    /// Required unless the mode or spec narrows it. `legal_moves` is made
    /// required for play mode by the compiler; the rest of the required set is
    /// fixed here.
    pub const fn is_required(self) -> bool {
        matches!(
            self,
            SectionId::System | SectionId::Position | SectionId::Ascii
        )
    }

    /// Parse a stable section name.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.as_str() == name)
    }
}

/// A context specification.
///
/// `sections` is the enabled set; `legal_moves` and the evidence toggles can
/// narrow it further. The compiler always emits sections in
/// [`SectionId::ALL`] order, never in this vector's order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContextSpec {
    pub id: String,
    pub version: u32,
    pub sections: Vec<SectionId>,
    pub token_budget: u32,
    pub tokenizer: String,
    pub turn_window: usize,
    /// Whether the `legal_moves` section is requested. In play mode an enabled
    /// `legal_moves` section is required.
    pub legal_moves: bool,
    pub engine: bool,
    pub openings: bool,
    pub database: bool,
    pub notes: bool,
    pub siblings: bool,
    pub compare: bool,
}

impl Default for ContextSpec {
    fn default() -> Self {
        Self::chat_v1()
    }
}

impl ContextSpec {
    /// Chat preset: 8k budget, legal moves available, no engine/database.
    pub fn chat_v1() -> Self {
        Self {
            id: "chat-v1".to_owned(),
            version: 1,
            sections: vec![
                SectionId::System,
                SectionId::Position,
                SectionId::Ascii,
                SectionId::LegalMoves,
                SectionId::History,
                SectionId::Annotations,
                SectionId::Siblings,
                SectionId::Turns,
            ],
            token_budget: 8192,
            tokenizer: TOKENIZER_ESTIMATE_V1.to_owned(),
            turn_window: 12,
            legal_moves: true,
            engine: false,
            openings: false,
            database: false,
            notes: false,
            siblings: true,
            compare: false,
        }
    }

    /// Play preset: 16k budget, legal moves required.
    pub fn play_v1() -> Self {
        Self {
            id: "play-v1".to_owned(),
            version: 1,
            sections: vec![
                SectionId::System,
                SectionId::Position,
                SectionId::Ascii,
                SectionId::LegalMoves,
                SectionId::History,
                SectionId::Annotations,
                SectionId::Turns,
            ],
            token_budget: 16384,
            tokenizer: TOKENIZER_ESTIMATE_V1.to_owned(),
            turn_window: 12,
            legal_moves: true,
            engine: false,
            openings: false,
            database: false,
            notes: false,
            siblings: false,
            compare: false,
        }
    }

    /// Rated play preset: byte-identical section policy to `play-v1`, with a
    /// distinct identity so evaluators can pin it.
    pub fn rated_standard_v1() -> Self {
        Self {
            id: "rated-standard-v1".to_owned(),
            ..Self::play_v1()
        }
    }

    /// Training preset: every deterministic section may be enabled.
    pub fn training_v1() -> Self {
        Self {
            id: "training-v1".to_owned(),
            version: 1,
            sections: SectionId::ALL.to_vec(),
            token_budget: 8192,
            tokenizer: TOKENIZER_ESTIMATE_V1.to_owned(),
            turn_window: 12,
            legal_moves: true,
            engine: true,
            openings: true,
            database: true,
            notes: true,
            siblings: true,
            compare: true,
        }
    }

    /// Look up a named preset.
    pub fn preset(name: &str) -> Option<Self> {
        match name {
            "chat-v1" => Some(Self::chat_v1()),
            "play-v1" => Some(Self::play_v1()),
            "rated-standard-v1" => Some(Self::rated_standard_v1()),
            "training-v1" => Some(Self::training_v1()),
            _ => None,
        }
    }

    /// Whether an optional evidence section is switched on.
    pub fn evidence_toggle(&self, id: SectionId) -> bool {
        match id {
            SectionId::Engine => self.engine,
            SectionId::Openings => self.openings,
            SectionId::Database => self.database,
            SectionId::Notes => self.notes,
            SectionId::Siblings => self.siblings,
            SectionId::Compare => self.compare,
            _ => true,
        }
    }

    /// The enabled set in canonical order.
    pub fn enabled_sections(&self) -> Vec<SectionId> {
        SectionId::ALL
            .into_iter()
            .filter(|id| self.sections.contains(id))
            .filter(|id| self.evidence_toggle(*id))
            .filter(|id| *id != SectionId::LegalMoves || self.legal_moves)
            .collect()
    }
}
