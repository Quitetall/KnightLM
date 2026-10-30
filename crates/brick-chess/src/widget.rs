use anyhow::{Context, Result};
use knightlm_core::state::{
    castling_mode_for_fen, parse_variant_fen, variant_for_name, GameRecord, GameState, START_FEN,
};
use knightlm_core::SCHEMA_VERSION;
use knightlm_notation::{pgn, reader, san};
use shakmaty::{uci::UciMove, variant::Variant, CastlingMode, Position};

use crate::command::{Command, Event, Shape};
use crate::pieces::PieceSet;
use crate::scene::Scene;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    White,
    Black,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Brown,
    Blue,
    Green,
}

fn normalize_variant_name(name: &str) -> String {
    let lower = name.trim().to_ascii_lowercase();
    match lower.as_str() {
        "" | "standard" | "chess" => "standard".into(),
        "chess960" | "chess 960" | "fischer random" => "chess960".into(),
        "king of the hill" | "kingofthehill" => "kingofthehill".into(),
        "three-check" | "three check" | "threecheck" => "3check".into(),
        other => other.to_owned(),
    }
}

fn start_state() -> GameState {
    GameState::replay(&GameRecord {
        schema_version: SCHEMA_VERSION,
        game_id: "brick-chess".into(),
        source: "brick-chess".into(),
        initial_fen: START_FEN.into(),
        history_complete: true,
        moves: vec![],

        variant: "standard".into(),
    })
    .expect("valid start position")
}

fn state_from_fen(fen: &str, variant_name: &str) -> Result<GameState> {
    let fields: Vec<&str> = fen.split_whitespace().collect();
    anyhow::ensure!(fields.len() == 4 || fields.len() == 6, "FEN needs 4 or 6 fields");
    let full = if fields.len() == 4 {
        format!("{} 0 1", fen.trim())
    } else {
        fen.trim().to_owned()
    };
    GameState::replay(&GameRecord {
        schema_version: SCHEMA_VERSION,
        game_id: "brick-chess".into(),
        source: "brick-chess".into(),
        initial_fen: full,
        history_complete: false,
        moves: vec![],
        variant: variant_name.to_owned(),
    })
    .context("invalid FEN")
}

/// The widget: canonical chess state (via `knightlm-core`) plus view state
/// (selection, history position, annotations, orientation, theme).
#[derive(Clone)]
pub struct Widget {
    history: Vec<GameState>,
    index: usize,
    selection: Option<String>,
    shapes: Vec<Shape>,
    orientation: Orientation,
    theme: Theme,
    pieces: PieceSet,
    variant: String,
}

impl Default for Widget {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget {
    pub fn new() -> Self {
        Self {
            history: vec![start_state()],
            index: 0,
            selection: None,
            shapes: Vec::new(),
            orientation: Orientation::White,
            theme: Theme::Green,
            pieces: PieceSet::Cburnett,
            variant: "standard".into(),
        }
    }

    pub fn from_fen(fen: &str) -> Result<Self> {
        let st = state_from_fen(fen, "standard")?;
        let mut w = Self::new();
        w.history = vec![st];
        w.index = 0;
        Ok(w)
    }

    pub fn current(&self) -> &GameState {
        &self.history[self.index]
    }

    pub fn initial_fen(&self) -> String {
        self.current().record().initial_fen.clone()
    }

    pub fn fen(&self) -> String {
        self.current().fen()
    }

    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    pub fn history_index(&self) -> usize {
        self.index
    }

    pub fn orientation(&self) -> Orientation {
        self.orientation
    }

    pub fn theme(&self) -> Theme {
        self.theme
    }

    pub fn pieces(&self) -> PieceSet {
        self.pieces
    }

    pub fn variant(&self) -> &str {
        &self.variant
    }

    pub fn shapes(&self) -> &[Shape] {
        &self.shapes
    }

    pub fn selection(&self) -> Option<&str> {
        self.selection.as_deref()
    }

    /// The move that led to the current history position (works at any ply).
    pub fn last_move_pair(&self) -> Option<(String, String)> {
        let uci = self.current().record().moves.last()?;
        if uci.len() < 4 || uci.contains('@') {
            return None;
        }
        Some((uci[0..2].to_owned(), uci[2..4].to_owned()))
    }

    pub fn move_uci_at(&self, index: usize) -> Option<String> {
        self.history
            .get(index)?
            .record()
            .moves
            .last()
            .cloned()
    }

    /// The move list (SAN) of the game so far.
    pub fn san_moves(&self) -> Vec<String> {
        let final_state = self.history.last().expect("non-empty");
        let rec = final_state.record();
        let variant = variant_for_name(&rec.variant);
        let mode = if variant == Variant::Chess {
            if rec.variant.eq_ignore_ascii_case("chess960") {
                CastlingMode::Chess960
            } else {
                castling_mode_for_fen(&rec.initial_fen)
            }
        } else {
            CastlingMode::Standard
        };
        let (mut pos, _) =
            parse_variant_fen(&rec.initial_fen, variant, mode).expect("valid initial FEN");
        let mut out = Vec::new();
        for uci in &rec.moves {
            out.push(san::to_san(&pos, uci).unwrap_or_else(|_| uci.clone()));
            if let Ok(u) = UciMove::from_ascii(uci.as_bytes()) {
                if let Ok(mv) = u.to_move(&pos) {
                    if let Ok(next) = pos.clone().play(mv) {
                        pos = next;
                    }
                }
            }
        }
        out
    }

    fn turn(&self) -> String {
        self.fen()
            .split_whitespace()
            .nth(1)
            .unwrap_or("w")
            .to_owned()
    }

    fn position_event(&self) -> Event {
        Event::Position {
            fen: self.fen(),
            turn: self.turn(),
            index: self.index,
        }
    }

    fn push_state(&mut self, state: GameState) {
        self.history.truncate(self.index + 1);
        self.history.push(state);
        self.index = self.history.len() - 1;
    }

    /// Apply a command, returning the events it produced. Never panics and
    /// never silently repairs: refusals become `Event::Error`.
    pub fn apply(&mut self, cmd: Command) -> Vec<Event> {
        match cmd {
            Command::Reset { fen } => {
                let st = match fen {
                    Some(f) => match state_from_fen(&f, &self.variant) {
                        Ok(s) => s,
                        Err(e) => return err("bad_fen", e.to_string()),
                    },
                    None => start_state(),
                };
                self.history = vec![st];
                self.index = 0;
                self.selection = None;
                self.shapes.clear();
                vec![self.position_event()]
            }
            Command::LoadFen { fen } => match state_from_fen(&fen, &self.variant) {
                Ok(st) => {
                    self.history = vec![st];
                    self.index = 0;
                    self.selection = None;
                        vec![self.position_event()]
                }
                Err(e) => err("bad_fen", e.to_string()),
            },
            Command::LoadPgn { pgn } => match load_pgn_states(&pgn) {
                Ok(states) => {
                    self.variant = states
                        .last()
                        .map(|st| st.record().variant.clone())
                        .unwrap_or_else(|| "standard".into());
                    self.history = states;
                    self.index = self.history.len().saturating_sub(1);
                    self.selection = None;
                    self.shapes.clear();
                        vec![self.position_event()]
                }
                Err(e) => err("bad_pgn", e.to_string()),
            },
            Command::Play { san, uci } => {
                let mv = match (san, uci) {
                    (Some(s), None) => match san::parse_san(self.current().position(), &s) {
                        Ok(u) => u,
                        Err(e) => return err("bad_san", e.to_string()),
                    },
                    (None, Some(u)) => u,
                    _ => return err("bad_command", "play needs exactly one of san or uci"),
                };
                self.do_move(&mv)
            }
            Command::Select { square } => {
                let targets = self
                    .current()
                    .legal_moves()
                    .into_iter()
                    .filter(|m| m.len() >= 4 && &m[0..2] == square)
                    .map(|m| m[2..4].to_owned())
                    .collect::<Vec<_>>();
                if targets.is_empty() {
                    self.selection = None;
                } else {
                    self.selection = Some(square.clone());
                }
                vec![Event::Selected { square, targets }]
            }
            Command::Arrow { from, to, color } => {
                self.shapes.push(Shape::Arrow { from, to, color });
                vec![]
            }
            Command::Circle { square, color } => {
                self.shapes.push(Shape::Circle { square, color });
                vec![]
            }
            Command::Highlight { squares, color } => {
                self.shapes.push(Shape::Highlight { squares, color });
                vec![]
            }
            Command::Goto { index } => {
                if self.history.is_empty() {
                    return err("no_history", "history is empty");
                }
                let idx = index.min(self.history.len() - 1);
                self.index = idx;
                self.selection = None;
                vec![self.position_event()]
            }
            Command::Undo => {
                self.index = self.index.saturating_sub(1);
                self.selection = None;
                vec![self.position_event()]
            }
            Command::Orient { color } => {
                self.orientation = match color.to_ascii_lowercase().as_str() {
                    "black" | "b" => Orientation::Black,
                    _ => Orientation::White,
                };
                vec![self.position_event()]
            }
            Command::Flip => {
                self.orientation = match self.orientation {
                    Orientation::White => Orientation::Black,
                    Orientation::Black => Orientation::White,
                };
                vec![]
            }
            Command::SetVariant { variant } => {
                let name = normalize_variant_name(&variant);
                match state_from_fen(&self.fen(), &name) {
                    Ok(state) => {
                        self.variant = name;
                        self.history = vec![state];
                        self.index = 0;
                        self.selection = None;
                                vec![self.position_event()]
                    }
                    Err(e) => err("bad_variant", e.to_string()),
                }
            }
            Command::SetTheme { theme } => {
                self.theme = match theme.to_ascii_lowercase().as_str() {
                    "blue" => Theme::Blue,
                    "brown" => Theme::Brown,
                    "green" => Theme::Green,
                    other => return err("bad_theme", format!("unknown theme: {other}")),
                };
                vec![]
            }
            Command::SetPieces { pieces } => match PieceSet::parse(&pieces) {
                Some(p) => {
                    self.pieces = p;
                    vec![]
                }
                None => err("bad_pieces", format!("unknown piece set: {pieces}")),
            },
            Command::Export { format } => match self.export(&format) {
                Ok(data) => vec![Event::Exported { format, data }],
                Err(e) => err("bad_export", e.to_string()),
            },
        }
    }

    fn do_move(&mut self, uci: &str) -> Vec<Event> {
        if uci.len() < 4 {
            return err("bad_move", format!("not a UCI move: {uci}"));
        }
        let mut st = self.current().clone();
        if !st.is_legal(uci) {
            return err("illegal_move", uci.to_owned());
        }
        let san_str = san::to_san(st.position(), uci).unwrap_or_else(|_| uci.to_owned());
        if let Err(e) = st.play(uci) {
            return err("illegal_move", e.to_string());
        }
        self.push_state(st);
        self.selection = None;

        let mut events = vec![
            Event::Move {
                uci: uci.to_owned(),
                san: san_str,
                fen: self.fen(),
            },
            self.position_event(),
        ];
        if let Some(over) = self.game_over() {
            events.push(over);
        }
        events
    }

    fn game_over(&self) -> Option<Event> {
        let facts = self.current().facts();
        if facts.checkmate {
            let result = if self.turn() == "w" { "0-1" } else { "1-0" };
            return Some(Event::GameOver {
                result: result.to_owned(),
                reason: "checkmate".to_owned(),
            });
        }
        if facts.stalemate {
            return Some(Event::GameOver {
                result: "1/2-1/2".to_owned(),
                reason: "stalemate".to_owned(),
            });
        }
        if let Some(reason) = facts.terminal_reason {
            return Some(Event::GameOver {
                result: "1/2-1/2".to_owned(),
                reason,
            });
        }
        None
    }

    pub fn export(&self, format: &str) -> Result<String> {
        match format.to_ascii_lowercase().as_str() {
            "fen" => Ok(self.fen()),
            "pgn" => Ok(self.to_pgn()),
            "svg" => Ok(crate::svg::render(&self.scene())),
            "ascii" => Ok(crate::ascii::render(&self.scene())),
            other => anyhow::bail!("unknown export format: {other}"),
        }
    }

    pub fn to_pgn(&self) -> String {
        let sans = self.san_moves();
        let mut out = String::new();
        out.push_str("[Event \"BrickChess\"]\n[Site \"?\"]\n[Date \"????.??.??\"]\n");
        out.push_str("[White \"?\"]\n[Black \"?\"]\n");
        let result = self
            .game_over()
            .map(|e| match e {
                Event::GameOver { result, .. } => result,
                _ => "*".to_owned(),
            })
            .unwrap_or_else(|| "*".to_owned());
        out.push_str(&format!("[Result \"{}\"]\n\n", result));
        for (i, san) in sans.iter().enumerate() {
            if i % 2 == 0 {
                out.push_str(&format!("{}. ", i / 2 + 1));
            }
            out.push_str(san);
            out.push(' ');
        }
        out.push_str(&result);
        out.push('\n');
        out
    }

    pub fn scene(&self) -> Scene {
        Scene::from_widget(self)
    }
}

fn err(code: &str, detail: impl Into<String>) -> Vec<Event> {
    vec![Event::Error {
        code: code.to_owned(),
        detail: detail.into(),
    }]
}

fn load_pgn_states(text: &str) -> Result<Vec<GameState>> {
    let game = pgn::parse(text).context("parse PGN")?;
    let record = reader::to_game_record(&game, "brick-chess", "pgn")?;
    let mut initial = record.clone();
    initial.moves.clear();
    let mut states = vec![GameState::replay(&initial)?];
    let mut current = GameState::replay(&initial)?;
    for mv in &record.moves {
        current.play(mv)?;
        states.push(current.clone());
    }
    Ok(states)
}
