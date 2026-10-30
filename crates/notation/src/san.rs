//! SAN read/write, delegated to shakmaty.
//!
//! Thin wrappers over `shakmaty::san` so the rest of KnightLM has a small,
//! stable surface while the rules live in one battle-tested library.

use anyhow::{Context, Result};
use knightlm_core::state::GameState;
use shakmaty::{san::SanPlus, uci::UciMove, CastlingMode, Position};

/// Render a UCI move as SAN in `pos`.
pub fn to_san<P: Position + Clone>(pos: &P, uci: &str) -> Result<String> {
    let uci_move = UciMove::from_ascii(uci.as_bytes()).context("malformed UCI move")?;
    let mv = uci_move.to_move(pos).context("illegal UCI move")?;
    Ok(SanPlus::from_move(pos.clone(), mv).to_string())
}

/// Parse SAN into a UCI move in `pos`. Refuses illegal or ambiguous SAN.
pub fn parse_san<P: Position + Clone>(pos: &P, san: &str) -> Result<String> {
    parse_san_mode(pos, san, CastlingMode::Standard)
}

pub fn parse_san_mode<P: Position + Clone>(
    pos: &P,
    san: &str,
    mode: CastlingMode,
) -> Result<String> {
    let parsed = SanPlus::from_ascii(san.as_bytes()).context("malformed SAN")?;
    let mv = parsed.san.to_move(pos).context("illegal SAN")?;
    Ok(UciMove::from_move(mv, mode).to_string())
}

/// SAN for a UCI move played in `state`.
pub fn to_san_state(state: &GameState, uci: &str) -> Result<String> {
    to_san(state.position(), uci)
}

/// Parse SAN into a UCI move for the position in `state`.
pub fn parse_san_state(state: &GameState, san: &str) -> Result<String> {
    parse_san_mode(state.position(), san, state.castling_mode())
}

/// Render the legal moves of `state` as SAN.
pub fn legal_moves_san(state: &GameState) -> Vec<String> {
    let mut out = Vec::new();
    for uci in state.legal_moves() {
        if let Ok(s) = to_san(state.position(), &uci) {
            out.push(s);
        }
    }
    out
}
