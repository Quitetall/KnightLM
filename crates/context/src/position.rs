//! Build the authoritative [`GameState`] and the canonical move lists.
//!
//! History is SAN from the standard initial position. If a FEN is supplied
//! together with history, the FEN must match the replayed position; otherwise
//! the caller is asking for two different boards and the compiler refuses.

use anyhow::{Context, Result, ensure};
use knightlm_core::SCHEMA_VERSION;
use knightlm_core::state::{GameRecord, GameState, START_FEN};
use knightlm_notation::san::{parse_san_state, to_san_state};
use shakmaty::{Color, Position, variant::Variant};

use crate::input::{ContextInput, Side};

/// A position ready for rendering.
pub(crate) struct PreparedPosition {
    pub(crate) state: GameState,
    pub(crate) fen: String,
    pub(crate) history: Vec<String>,
    pub(crate) history_complete: bool,
    pub(crate) side_to_move: Side,
    pub(crate) variant: String,
    pub(crate) plies: usize,
}

impl PreparedPosition {
    pub(crate) fn ruleset(&self) -> String {
        if self.state.variant() == Variant::Chess {
            "standard-untimed-v1".to_owned()
        } else {
            format!("{}-untimed-v1", self.state.variant().uci())
        }
    }

    /// `(SAN, UCI)` pairs sorted by SAN, then UCI.
    pub(crate) fn legal_move_pairs(&self) -> Vec<(String, String)> {
        let mut pairs: Vec<(String, String)> = self
            .state
            .legal_moves()
            .into_iter()
            .map(|uci| {
                let san = to_san_state(&self.state, &uci).unwrap_or_else(|_| uci.clone());
                (san, uci)
            })
            .collect();
        pairs.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        pairs
    }
}

pub(crate) fn prepare(input: &ContextInput) -> Result<PreparedPosition> {
    if input.history.is_empty() {
        let fen = input.fen.clone().unwrap_or_else(|| START_FEN.to_owned());
        let variant = input.variant.as_deref().unwrap_or("standard");
        let history_complete = input.fen.is_none();
        let state = state_from_fen(&fen, variant, history_complete)?;
        return Ok(finish(state, Vec::new(), history_complete));
    }

    let mut state = state_from_fen(START_FEN, "standard", true)?;
    let mut history = Vec::with_capacity(input.history.len());
    for (index, san) in input.history.iter().enumerate() {
        let uci = parse_san_state(&state, san)
            .with_context(|| format!("history move {} is not legal SAN: `{san}`", index + 1))?;
        let canonical_san = to_san_state(&state, &uci)
            .with_context(|| format!("history move {} could not be rendered as SAN", index + 1))?;
        state
            .play(&uci)
            .with_context(|| format!("history move {} could not be played: `{san}`", index + 1))?;
        history.push(canonical_san);
    }

    if let Some(fen) = &input.fen {
        let supplied = state_from_fen(fen, input.variant.as_deref().unwrap_or("standard"), false)?;
        ensure!(
            supplied.fen() == state.fen(),
            "fen does not match history: supplied `{}`, history reaches `{}`",
            supplied.fen(),
            state.fen()
        );
    }

    Ok(finish(state, history, true))
}

fn state_from_fen(fen: &str, variant: &str, history_complete: bool) -> Result<GameState> {
    let record = GameRecord {
        schema_version: SCHEMA_VERSION,
        game_id: "context".to_owned(),
        source: "context".to_owned(),
        initial_fen: fen.to_owned(),
        history_complete,
        moves: Vec::new(),
        variant: variant.to_owned(),
    };
    GameState::replay(&record).with_context(|| format!("invalid position FEN `{fen}`"))
}

fn finish(state: GameState, history: Vec<String>, history_complete: bool) -> PreparedPosition {
    let fen = state.fen();
    let side_to_move = match state.position().turn() {
        Color::White => Side::White,
        Color::Black => Side::Black,
    };
    let variant = if state.variant() == Variant::Chess {
        "standard".to_owned()
    } else {
        state.variant().uci().to_owned()
    };
    let plies = history.len();
    PreparedPosition {
        state,
        fen,
        history,
        history_complete,
        side_to_move,
        variant,
        plies,
    }
}
