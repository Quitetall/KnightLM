use anyhow::{Context, Result};
use knightlm_core::state::{GameRecord, GameState, START_FEN};
use knightlm_core::SCHEMA_VERSION;
use serde::Serialize;
use std::fmt::Write as _;

use crate::uci::Player;

#[derive(Debug, Clone, Serialize)]
pub struct MatchResult {
    pub moves: Vec<String>,
    pub sans: Vec<String>,
    pub result: String,
    pub termination: String,
    pub white: String,
    pub black: String,
}

fn start() -> GameState {
    GameState::replay(&GameRecord {
        schema_version: SCHEMA_VERSION,
        game_id: "match".into(),
        source: "match".into(),
        initial_fen: START_FEN.into(),
        history_complete: true,
        moves: vec![],

        variant: "standard".into(),
    })
    .expect("start position")
}

/// Play one game between two players. White and black take turns. Returns the
/// result with per-ply UCI moves and the SANs (for PGN export).
pub fn play_game(
    white: &dyn Player,
    black: &dyn Player,
    max_plies: usize,
) -> Result<MatchResult> {
    let mut state = start();
    let mut sans = Vec::new();
    let mut moves = Vec::new();
    let mut plies = 0usize;

    loop {
        let facts = state.facts();
        if facts.checkmate {
            let result = if state.record().moves.len() % 2 == 0 { "1-0" } else { "0-1" };
            return Ok(finish(moves, sans, result, "checkmate".into(), white.name(), black.name()));
        }
        if facts.stalemate {
            return Ok(finish(moves, sans, "1/2-1/2", "stalemate".into(), white.name(), black.name()));
        }
        if facts.terminal_reason.is_some() {
            return Ok(finish(moves, sans, "1/2-1/2", facts.terminal_reason.unwrap_or_else(|| "terminal".to_string()), white.name(), black.name()));
        }
        if plies >= max_plies {
            return Ok(finish(moves, sans, "1/2-1/2", "move limit".into(), white.name(), black.name()));
        }
        let white_to_move = state.record().moves.len() % 2 == 0;
        let mv = if white_to_move {
            white.select(&state).context("white move")?
        } else {
            black.select(&state).context("black move")?
        };
        if !state.is_legal(&mv) {
            let result = if white_to_move { "0-1" } else { "1-0" };
            return Ok(finish(
                moves,
                sans,
                result,
                format!("illegal move by {}", if white_to_move { "white" } else { "black" }),
                white.name(),
                black.name(),
            ));
        }
        let san = knightlm_notation::to_san(state.position(), &mv).unwrap_or_else(|_| mv.clone());
        state.play(&mv).context("play")?;
        moves.push(mv);
        sans.push(san);
        plies += 1;
    }
}

fn finish(
    moves: Vec<String>,
    sans: Vec<String>,
    result: &str,
    termination: String,
    white: &str,
    black: &str,
) -> MatchResult {
    MatchResult { moves, sans, result: result.to_owned(), termination, white: white.to_owned(), black: black.to_owned() }
}

/// Render a MatchResult as a PGN game string.
pub fn to_pgn(result: &MatchResult, event: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "[Event \"{}\"]", event);
    let _ = writeln!(out, "[Site \"KnightLM\"]");
    let _ = writeln!(out, "[White \"{}\"]", result.white);
    let _ = writeln!(out, "[Black \"{}\"]", result.black);
    let _ = writeln!(out, "[Result \"{}\"]", result.result);
    let _ = writeln!(out, "[Termination \"{}\"]", result.termination);
    let mut line = String::new();
    for (i, san) in result.sans.iter().enumerate() {
        if i % 2 == 0 {
            let _ = write!(line, "{}. ", i / 2 + 1);
        }
        let _ = write!(line, "{} ", san);
    }
    let _ = writeln!(line, "{}", result.result);
    out.push_str(&line);
    out
}
