//! BrickChess commands and events — the tool-call surface.
//!
//! An agent (or CLI, or UI) sends a [`Command`]; the widget replies with one or
//! more [`Event`]s. Every command is JSON-serializable and every event is too,
//! so the same protocol drives a language model's tool calls, a shell pipeline,
//! and the in-process typed builder.

use serde::{Deserialize, Serialize};

/// A command sent to the widget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    /// Reset to a position (default: the standard start).
    Reset {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fen: Option<String>,
    },
    /// Load a position from FEN.
    LoadFen { fen: String },
    /// Load a game from PGN (main line).
    LoadPgn { pgn: String },
    /// Play a move, by SAN or UCI (exactly one must be given).
    Play {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        san: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        uci: Option<String>,
    },
    /// Select a square, reporting its legal destinations.
    Select { square: String },
    /// Draw an annotation arrow.
    Arrow {
        from: String,
        to: String,
        #[serde(default = "default_color")]
        color: String,
    },
    /// Draw a circle on a square.
    Circle {
        square: String,
        #[serde(default = "default_color")]
        color: String,
    },
    /// Highlight a set of squares.
    Highlight {
        squares: Vec<String>,
        #[serde(default = "default_color")]
        color: String,
    },
    /// Jump to a position in the game history (0 = initial).
    Goto { index: usize },
    /// Step back one position.
    Undo,
    /// Flip board orientation.
    Flip,
    /// Set orientation: "white" or "black".
    Orient { color: String },
    /// Set the board theme.
    SetTheme { theme: String },
    /// Set the piece artwork.
    SetPieces { pieces: String },
    /// Switch the rules variant (`standard`, `chess960`, `atomic`, …).
    SetVariant { variant: String },
    /// Export the current state in one of: fen, pgn, svg, ascii.
    Export { format: String },
}

fn default_color() -> String {
    "green".to_owned()
}

/// An event emitted by the widget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// The current position changed.
    Position {
        fen: String,
        turn: String,
        index: usize,
    },
    /// A move was played.
    Move {
        uci: String,
        san: String,
        fen: String,
    },
    /// A square was selected (empty targets means a deselect was reported).
    Selected { square: String, targets: Vec<String> },
    /// The game ended.
    GameOver { result: String, reason: String },
    /// A requested export.
    Exported { format: String, data: String },
    /// A command was refused. Never silent: an illegal move is an error event.
    Error { code: String, detail: String },
}

/// A rendition-only annotation (stored on the widget, drawn by the renderer).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Shape {
    Arrow {
        from: String,
        to: String,
        color: String,
    },
    Circle {
        square: String,
        color: String,
    },
    Highlight {
        squares: Vec<String>,
        color: String,
    },
}
