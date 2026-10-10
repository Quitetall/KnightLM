use anyhow::{Context, Result, bail, ensure};
use cozy_chess::{Board, Color, Move, Piece, Square};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::SCHEMA_VERSION;

pub const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameRecord {
    pub schema_version: u32,
    pub game_id: String,
    pub source: String,
    pub initial_fen: String,
    /// An assertion by the trusted source, never inferred from a FEN or image.
    pub history_complete: bool,
    pub moves: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GameState {
    board: Board,
    halfmove: u32,
    fullmove: u32,
    repetitions: BTreeMap<String, u32>,
    record: GameRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleFacts {
    pub in_check: bool,
    pub checkmate: bool,
    pub stalemate: bool,
    pub repetition_count: Option<u32>,
    pub threefold_claim_now: Option<bool>,
    pub fifty_move_claim_now: bool,
    pub fivefold_automatic: Option<bool>,
    pub seventy_five_move_automatic: bool,
    /// Only proven elementary dead positions are recognized in v1.
    pub dead_position: Option<bool>,
    pub terminal_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedFeatures {
    pub material_counts: BTreeMap<String, u32>,
    /// Absolute pins of the side to move. Not tactical evaluations.
    pub pinned_to_move: Vec<String>,
    /// Pseudo-attacks: a pinned piece still attacks squares under chess rules.
    pub attacks: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PacketOptions {
    pub legal_choices: bool,
    pub derived_features: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextPacket {
    pub schema_version: u32,
    pub ruleset: String,
    pub fen: String,
    pub history_complete: bool,
    /// Canonical four-field positions, with only legally usable en passant.
    /// Readable keys rather than hashes the model cannot interpret.
    pub repetition_ledger: BTreeMap<String, u32>,
    pub facts: RuleFacts,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legal_moves: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub features: Option<DerivedFeatures>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardObservation {
    pub schema_version: u32,
    pub image_id: String,
    /// FEN piece-placement field only. Hidden rights are never read from pixels.
    pub placement: String,
}

impl GameState {
    pub fn replay(record: &GameRecord) -> Result<Self> {
        ensure!(
            record.schema_version == SCHEMA_VERSION,
            "unsupported game schema"
        );
        ensure!(
            !record.game_id.is_empty() && !record.source.is_empty(),
            "missing game provenance"
        );
        let fields: Vec<_> = record.initial_fen.split_whitespace().collect();
        ensure!(fields.len() == 6, "complete six-field FEN required");
        let halfmove = fields[4]
            .parse::<u32>()
            .context("invalid halfmove counter")?;
        let fullmove = fields[5]
            .parse::<u32>()
            .context("invalid fullmove counter")?;
        ensure!(fullmove > 0, "fullmove counter must be positive");
        // cozy-chess caps its counters; retain authoritative counters independently.
        let board = Board::from_fen(&format!("{} 0 1", fields[..4].join(" ")), false)
            .context("invalid standard chess position")?;
        for (color, king_square) in [(Color::White, Square::E1), (Color::Black, Square::E8)] {
            let rights = board.castle_rights(color);
            ensure!(
                (rights.short.is_none() && rights.long.is_none())
                    || board.king(color) == king_square,
                "nonstandard castling king origin"
            );
        }
        let mut initial = record.clone();
        initial.moves.clear();
        let mut state = Self {
            board,
            halfmove,
            fullmove,
            repetitions: BTreeMap::new(),
            record: initial,
        };
        state.repetitions.insert(state.position_key(), 1);
        for mv in &record.moves {
            state.play(mv).with_context(|| {
                format!(
                    "game {} move {}",
                    record.game_id,
                    state.record.moves.len() + 1
                )
            })?;
        }
        Ok(state)
    }

    pub fn fen(&self) -> String {
        let fen = self.board.to_string();
        let fields: Vec<_> = fen.split_whitespace().collect();
        format!(
            "{} {} {}",
            fields[..4].join(" "),
            self.halfmove,
            self.fullmove
        )
    }

    pub fn record(&self) -> &GameRecord {
        &self.record
    }

    pub fn position_key(&self) -> String {
        let fen = self.board.to_string();
        let fields: Vec<_> = fen.split_whitespace().collect();
        let ep_usable = self.board.en_passant().is_some()
            && self.internal_moves().iter().any(|m| {
                self.board.piece_on(m.from) == Some(Piece::Pawn)
                    && m.from.file() != m.to.file()
                    && self.board.piece_on(m.to).is_none()
            });
        format!(
            "{} {} {} {}",
            fields[0],
            fields[1],
            fields[2],
            if ep_usable { fields[3] } else { "-" }
        )
    }

    fn internal_moves(&self) -> Vec<Move> {
        let mut moves = Vec::new();
        self.board.generate_moves(|set| {
            moves.extend(set);
            false
        });
        moves
    }

    pub fn legal_moves(&self) -> Vec<String> {
        if self.facts().terminal_reason.is_some() {
            return vec![];
        }
        let mut moves: Vec<_> = self
            .internal_moves()
            .into_iter()
            .map(|m| cozy_chess::util::display_uci_move(&self.board, m).to_string())
            .collect();
        moves.sort();
        moves
    }

    pub fn is_legal(&self, uci: &str) -> bool {
        self.facts().terminal_reason.is_none()
            && cozy_chess::util::parse_uci_move(&self.board, uci)
                .is_ok_and(|m| self.board.is_legal(m))
    }

    pub fn play(&mut self, uci: &str) -> Result<()> {
        ensure!(
            self.facts().terminal_reason.is_none(),
            "game is already terminal"
        );
        let mv =
            cozy_chess::util::parse_uci_move(&self.board, uci).context("malformed UCI move")?;
        ensure!(self.board.is_legal(mv), "illegal move: {uci}");
        let pawn = self.board.piece_on(mv.from) == Some(Piece::Pawn);
        let capture = self.board.color_on(mv.to) == Some(!self.board.side_to_move())
            || (pawn && mv.from.file() != mv.to.file());
        let rights_before = self
            .board
            .to_string()
            .split_whitespace()
            .nth(2)
            .unwrap()
            .to_owned();
        let next_half = if pawn || capture {
            0
        } else {
            self.halfmove.checked_add(1).context("halfmove overflow")?
        };
        let next_full = self
            .fullmove
            .checked_add(u32::from(self.board.side_to_move() == Color::Black))
            .context("fullmove overflow")?;
        self.board.try_play(mv).context("illegal move")?;
        self.halfmove = next_half;
        self.fullmove = next_full;
        let rights_after = self
            .board
            .to_string()
            .split_whitespace()
            .nth(2)
            .unwrap()
            .to_owned();
        if pawn || capture || rights_before != rights_after {
            self.repetitions.clear();
        }
        *self.repetitions.entry(self.position_key()).or_default() += 1;
        self.record.moves.push(uci.to_owned());
        Ok(())
    }

    pub fn facts(&self) -> RuleFacts {
        let in_check = !self.board.checkers().is_empty();
        let no_moves = self.internal_moves().is_empty();
        let checkmate = no_moves && in_check;
        let stalemate = no_moves && !in_check;
        let count = self
            .record
            .history_complete
            .then(|| *self.repetitions.get(&self.position_key()).unwrap_or(&0));
        let dead = self.elementary_dead_position().then_some(true);
        let terminal_reason = if checkmate {
            Some("checkmate")
        } else if stalemate {
            Some("stalemate")
        } else if dead == Some(true) {
            Some("proven_dead_position")
        } else if count.is_some_and(|n| n >= 5) {
            Some("fivefold_repetition")
        } else if self.halfmove >= 150 {
            Some("seventy_five_move")
        } else {
            None
        }
        .map(str::to_owned);
        let active = terminal_reason.is_none();
        RuleFacts {
            in_check,
            checkmate,
            stalemate,
            repetition_count: count,
            threefold_claim_now: count.map(|n| active && n >= 3),
            fifty_move_claim_now: active && self.halfmove >= 100,
            fivefold_automatic: count.map(|n| !checkmate && n >= 5),
            seventy_five_move_automatic: !checkmate && self.halfmove >= 150,
            dead_position: dead,
            terminal_reason,
        }
    }

    fn elementary_dead_position(&self) -> bool {
        let mut minors = 0;
        for square in Square::ALL {
            match self.board.piece_on(square) {
                Some(Piece::Pawn | Piece::Rook | Piece::Queen) => return false,
                Some(Piece::Bishop | Piece::Knight) => minors += 1,
                _ => {}
            }
        }
        minors <= 1 // K vs K, K+B vs K, K+N vs K only; no universal dead-position claim.
    }

    pub fn compile(&self, options: PacketOptions) -> ContextPacket {
        ContextPacket {
            schema_version: SCHEMA_VERSION,
            ruleset: "standard-untimed-v1".into(),
            fen: self.fen(),
            history_complete: self.record.history_complete,
            repetition_ledger: self.repetitions.clone(),
            facts: self.facts(),
            legal_moves: options.legal_choices.then(|| self.legal_moves()),
            features: options.derived_features.then(|| self.features()),
        }
    }

    pub fn verify_observation(&self, observation: &BoardObservation) -> Result<()> {
        ensure!(
            observation.schema_version == SCHEMA_VERSION,
            "unsupported observation schema"
        );
        ensure!(!observation.image_id.is_empty(), "missing image identity");
        let actual = self.fen();
        ensure!(
            actual.split_whitespace().next() == Some(observation.placement.as_str()),
            "perception failure: observed placement disagrees with authoritative ledger"
        );
        Ok(())
    }

    pub fn features(&self) -> DerivedFeatures {
        let mut material_counts = BTreeMap::new();
        let mut attacks = BTreeMap::new();
        for color in Color::ALL {
            let mut attacked = std::collections::BTreeSet::new();
            for square in Square::ALL {
                if self.board.color_on(square) != Some(color) {
                    continue;
                }
                let piece = self.board.piece_on(square).unwrap();
                *material_counts
                    .entry(format!("{color:?}_{piece:?}").to_lowercase())
                    .or_default() += 1;
                let squares = match piece {
                    Piece::Pawn => cozy_chess::get_pawn_attacks(square, color),
                    Piece::Knight => cozy_chess::get_knight_moves(square),
                    Piece::King => cozy_chess::get_king_moves(square),
                    Piece::Bishop => cozy_chess::get_bishop_moves(square, self.board.occupied()),
                    Piece::Rook => cozy_chess::get_rook_moves(square, self.board.occupied()),
                    Piece::Queen => {
                        cozy_chess::get_bishop_moves(square, self.board.occupied())
                            | cozy_chess::get_rook_moves(square, self.board.occupied())
                    }
                };
                attacked.extend(squares.into_iter().map(|s| s.to_string()));
            }
            attacks.insert(
                format!("{color:?}").to_lowercase(),
                attacked.into_iter().collect(),
            );
        }
        let mut pinned_to_move: Vec<_> = self
            .board
            .pinned()
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        pinned_to_move.sort();
        DerivedFeatures {
            material_counts,
            pinned_to_move,
            attacks,
        }
    }
}

/// Each request simulates one ply from an immutable numbered branch.
pub struct RuleSession {
    states: Vec<GameState>,
    budget: usize,
    calls: usize,
}

#[derive(Debug, Serialize)]
pub struct ToolReply {
    pub call: usize,
    pub branch: Option<usize>,
    pub legal: bool,
    pub state: Option<ContextPacket>,
    pub error: Option<String>,
}

impl RuleSession {
    pub fn new(state: GameState, budget: usize) -> Self {
        Self {
            states: vec![state],
            budget,
            calls: 0,
        }
    }
    pub fn calls(&self) -> usize {
        self.calls
    }
    pub fn simulate(&mut self, branch: usize, mv: &str) -> Result<ToolReply> {
        ensure!(self.calls < self.budget, "rule-tool budget exhausted");
        self.calls += 1; // Bad requests also consume budget.
        let Some(parent) = self.states.get(branch) else {
            bail!("unknown rule-tool branch")
        };
        let mut state = parent.clone();
        match state.play(mv) {
            Ok(()) => {
                let packet = state.compile(PacketOptions::default());
                let branch = self.states.len();
                self.states.push(state);
                Ok(ToolReply {
                    call: self.calls,
                    branch: Some(branch),
                    legal: true,
                    state: Some(packet),
                    error: None,
                })
            }
            Err(e) => Ok(ToolReply {
                call: self.calls,
                branch: None,
                legal: false,
                state: None,
                error: Some(e.to_string()),
            }),
        }
    }
}
