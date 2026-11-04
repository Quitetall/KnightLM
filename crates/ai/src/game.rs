//! Match state machine: a pure, replayable chess game with players and clocks.
//!
//! The rules engine is `shakmaty`; the context/agent layers drive this type.
//! No I/O and no model calls happen here.

use anyhow::{Context, Result, bail};
use knightlm_core::state::parse_chess;
use serde::{Deserialize, Serialize};
use shakmaty::fen::Fen;
use shakmaty::san::{San, SanPlus};
use shakmaty::uci::UciMove;
use shakmaty::{CastlingMode, Chess, Color, EnPassantMode, Position};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Player {
    Human { name: String },
    Model { provider: String, model: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchConfig {
    pub start_fen: String,
    pub white: Player,
    pub black: Player,
    pub max_plies: u32,
    #[serde(default = "default_retries")]
    pub max_retries: u8,
    #[serde(default)]
    pub rated: bool,
    #[serde(default = "default_ruleset")]
    pub ruleset: String,
}

fn default_retries() -> u8 {
    3
}

fn default_ruleset() -> String {
    "standard-v1".into()
}

impl Default for MatchConfig {
    fn default() -> Self {
        Self {
            start_fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".into(),
            white: Player::Human {
                name: "White".into(),
            },
            black: Player::Model {
                provider: "test".into(),
                model: "test".into(),
            },
            max_plies: 300,
            max_retries: 3,
            rated: false,
            ruleset: "standard-v1".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchState {
    AwaitingWhite,
    AwaitingBlack,
    Finished,
    Paused,
    Aborted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchMove {
    pub ply: u32,
    /// `white` or `black`.
    pub color: String,
    pub san: String,
    pub uci: String,
    pub fen: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchResult {
    /// `1-0`, `0-1` or `1/2-1/2`.
    pub score: String,
    pub termination: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchSnapshot {
    pub config: MatchConfig,
    pub state: MatchState,
    pub moves: Vec<MatchMove>,
    pub result: Option<MatchResult>,
}

#[derive(Debug, Clone)]
pub struct Match {
    config: MatchConfig,
    state: MatchState,
    moves: Vec<MatchMove>,
    result: Option<MatchResult>,
    position: Chess,
    mode: CastlingMode,
}

impl Match {
    pub fn new(config: MatchConfig) -> Result<Self> {
        let (position, mode) = parse_chess(&config.start_fen).context("invalid start FEN")?;
        let state = match position.turn() {
            Color::White => MatchState::AwaitingWhite,
            Color::Black => MatchState::AwaitingBlack,
        };
        Ok(Self {
            config,
            state,
            moves: Vec::new(),
            result: None,
            position,
            mode,
        })
    }

    pub fn config(&self) -> &MatchConfig {
        &self.config
    }

    pub fn state(&self) -> &MatchState {
        &self.state
    }

    pub fn moves(&self) -> &[MatchMove] {
        &self.moves
    }

    pub fn result(&self) -> Option<&MatchResult> {
        self.result.as_ref()
    }

    pub fn turn(&self) -> Color {
        self.position.turn()
    }

    pub fn fen(&self) -> String {
        Fen::from_position(&self.position, EnPassantMode::Legal).to_string()
    }

    pub fn legal_moves(&self) -> Vec<String> {
        self.position
            .legal_moves()
            .into_iter()
            .map(|mv| SanPlus::from_move(self.position.clone(), mv).to_string())
            .collect()
    }

    /// Apply one SAN or UCI move. Returns the recorded move or a typed error.
    pub fn apply(&mut self, input: &str) -> Result<MatchMove> {
        if matches!(self.state, MatchState::Finished | MatchState::Aborted) {
            bail!("match is finished");
        }
        let raw = input.trim();
        if raw.is_empty() {
            bail!("empty move");
        }
        let mv = if let Ok(uci) = UciMove::from_ascii(raw.as_bytes()) {
            uci.to_move(&self.position).ok()
        } else {
            San::from_ascii(raw.as_bytes())
                .ok()
                .and_then(|san| san.to_move(&self.position).ok())
        }
        .with_context(|| format!("{raw} is not a legal move"))?;
        if !self.position.is_legal(mv) {
            bail!("{raw} is not legal in this position");
        }
        let color = self.position.turn();
        let san = SanPlus::from_move(self.position.clone(), mv).to_string();
        let uci = UciMove::from_move(mv, self.mode).to_string();
        let next = self.position.clone().play(mv)?;
        self.position = next;
        let record = MatchMove {
            ply: self.moves.len() as u32 + 1,
            color: if color == Color::White { "white" } else { "black" }.into(),
            san,
            uci,
            fen: self.fen(),
        };
        self.moves.push(record.clone());
        self.update_state();
        Ok(record)
    }

    fn update_state(&mut self) {
        if self.position.is_checkmate() {
            let score = if self.position.turn() == Color::White {
                "0-1"
            } else {
                "1-0"
            };
            self.result = Some(MatchResult {
                score: score.into(),
                termination: "checkmate".into(),
            });
            self.state = MatchState::Finished;
        } else if self.position.is_stalemate() {
            self.result = Some(MatchResult {
                score: "1/2-1/2".into(),
                termination: "stalemate".into(),
            });
            self.state = MatchState::Finished;
        } else if self.position.is_insufficient_material() {
            self.result = Some(MatchResult {
                score: "1/2-1/2".into(),
                termination: "insufficient material".into(),
            });
            self.state = MatchState::Finished;
        } else if self.position.halfmoves() >= 100 {
            self.result = Some(MatchResult {
                score: "1/2-1/2".into(),
                termination: "fifty-move rule".into(),
            });
            self.state = MatchState::Finished;
        } else if self.moves.len() >= self.config.max_plies as usize {
            self.result = Some(MatchResult {
                score: "1/2-1/2".into(),
                termination: "max plies".into(),
            });
            self.state = MatchState::Finished;
        } else {
            self.state = match self.position.turn() {
                Color::White => MatchState::AwaitingWhite,
                Color::Black => MatchState::AwaitingBlack,
            };
        }
    }

    pub fn resign(&mut self, color: Color) {
        let score = if color == Color::White { "0-1" } else { "1-0" };
        self.result = Some(MatchResult {
            score: score.into(),
            termination: "resignation".into(),
        });
        self.state = MatchState::Finished;
    }

    pub fn snapshot(&self) -> MatchSnapshot {
        MatchSnapshot {
            config: self.config.clone(),
            state: self.state.clone(),
            moves: self.moves.clone(),
            result: self.result.clone(),
        }
    }

    pub fn from_snapshot(snapshot: MatchSnapshot) -> Result<Self> {
        let mut game = Self::new(snapshot.config)?;
        for mv in &snapshot.moves {
            game.apply(&mv.uci)?;
        }
        if matches!(snapshot.state, MatchState::Paused) {
            game.state = MatchState::Paused;
        }
        Ok(game)
    }

    pub fn pgn(&self) -> String {
        let mut out = String::new();
        let (white, black) = match (&self.config.white, &self.config.black) {
            (Player::Human { name }, Player::Model { provider, model }) => {
                (name.clone(), format!("{provider}/{model}"))
            }
            (Player::Model { provider, model }, Player::Human { name }) => {
                (format!("{provider}/{model}"), name.clone())
            }
            (Player::Human { name: w }, Player::Human { name: b }) => (w.clone(), b.clone()),
            (Player::Model { provider: p1, model: m1 }, Player::Model { provider: p2, model: m2 }) => {
                (format!("{p1}/{m1}"), format!("{p2}/{m2}"))
            }
        };
        out.push_str(&format!("[Event \"Assistant match\"]\n"));
        out.push_str(&format!("[White \"{white}\"]\n"));
        out.push_str(&format!("[Black \"{black}\"]\n"));
        out.push_str(&format!(
            "[Result \"{}\"]\n",
            self.result.as_ref().map(|r| r.score.as_str()).unwrap_or("*")
        ));
        out.push_str(&format!(
            "[Termination \"{}\"]\n\n",
            self.result
                .as_ref()
                .map(|r| r.termination.as_str())
                .unwrap_or("unterminated")
        ));
        for (index, mv) in self.moves.iter().enumerate() {
            if index % 2 == 0 {
                out.push_str(&format!("{}. ", index / 2 + 1));
            }
            out.push_str(&mv.san);
            out.push(' ');
        }
        out.push_str(
            self.result
                .as_ref()
                .map(|r| r.score.as_str())
                .unwrap_or("*"),
        );
        out.push('\n');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fools_mate_finishes_and_exports_pgn() {
        let mut game = Match::new(MatchConfig::default()).unwrap();
        for mv in ["e4", "e5", "Qh5", "Nc6", "Bc4", "Nf6", "Qxf7#"] {
            game.apply(mv).unwrap();
        }
        let result = game.result().expect("finished");
        assert_eq!(result.score, "1-0");
        assert_eq!(result.termination, "checkmate");
        assert_eq!(game.state(), &MatchState::Finished);
        assert!(game.pgn().contains("Qxf7#"));
        assert!(game.pgn().contains("[Result \"1-0\"]"));
    }

    #[test]
    fn illegal_move_is_refused() {
        let mut game = Match::new(MatchConfig::default()).unwrap();
        assert!(game.apply("e5").is_err());
        assert!(game.moves().is_empty());
    }

    #[test]
    fn snapshot_replays_to_the_same_position() {
        let mut game = Match::new(MatchConfig::default()).unwrap();
        for mv in ["e4", "e5", "Nf3", "Nc6"] {
            game.apply(mv).unwrap();
        }
        let replay = Match::from_snapshot(game.snapshot()).unwrap();
        assert_eq!(game.fen(), replay.fen());
        assert_eq!(game.moves(), replay.moves());
    }
}
