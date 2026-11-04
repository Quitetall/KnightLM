//! Input to the compiler: the position, its history, annotations, recent turns
//! and evidence. Nothing here is fetched or inferred from the environment.

use serde::{Deserialize, Serialize};

use crate::annotations::Annotations;
use crate::evidence::Evidence;

/// The mode decides the output contract and whether legal moves are required.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Chat,
    Play,
    Watch,
}

impl Mode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Mode::Chat => "chat",
            Mode::Play => "play",
            Mode::Watch => "watch",
        }
    }
}

/// The seat the assistant or human occupies, when the caller knows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    White,
    Black,
}

impl Side {
    pub const fn as_str(self) -> &'static str {
        match self {
            Side::White => "white",
            Side::Black => "black",
        }
    }
}

/// Who produced a conversation turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnRole {
    User,
    Assistant,
    Tool,
    System,
}

impl TurnRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            TurnRole::User => "user",
            TurnRole::Assistant => "assistant",
            TurnRole::Tool => "tool",
            TurnRole::System => "system",
        }
    }
}

/// One complete conversation turn. Truncation never splits a turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    pub role: TurnRole,
    pub content: String,
}

/// Everything the compiler needs.
///
/// `history` is SAN from the standard initial position. When `fen` is present
/// with a non-empty history, the FEN must match the replayed history; the
/// position is authoritative only when no history is supplied.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContextInput {
    /// Current position. `None` starts from the standard initial position and
    /// replays `history`.
    pub fen: Option<String>,
    /// SAN moves from the standard initial position.
    pub history: Vec<String>,
    /// Annotations attached to this input. Evidence annotations are merged in.
    pub annotations: Annotations,
    /// Conversation turns, oldest first.
    pub turns: Vec<Turn>,
    pub mode: Mode,
    /// Seat the assistant is playing or watching, when known.
    pub side: Option<Side>,
    /// Variant name for a FEN-only input (`standard` by default).
    pub variant: Option<String>,
    pub evidence: Evidence,
}

impl ContextInput {
    /// The standard initial position with no history. `fen` stays `None` so a
    /// caller can append SAN history without also pinning a contradictory FEN.
    pub fn start() -> Self {
        Self::default()
    }

    /// A FEN-only input. History is unknown, so repetition facts are absent.
    pub fn from_fen(fen: impl Into<String>) -> Self {
        Self {
            fen: Some(fen.into()),
            ..Self::default()
        }
    }
}
