use anyhow::{Context, Result, bail, ensure};
use shakmaty::{
    fen::Fen, uci::UciMove, variant::{Variant, VariantPosition}, CastlingMode, Chess, Color,
    EnPassantMode, Position, Role, Square,
};
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
    /// `standard`, `chess960`, or a shakmaty variant name (`atomic`, …).
    #[serde(default = "default_variant")]
    pub variant: String,
}

fn default_variant() -> String {
    "standard".into()
}

#[derive(Debug, Clone)]
pub struct GameState {
    board: VariantPosition,
    variant: Variant,
    mode: CastlingMode,
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

/// Castling notation implied by a FEN: file-letter rights (`HAha`) are
/// Chess960/Shredder-FEN, `KQkq` could be either and is resolved by parsing.
pub fn castling_mode_for_fen(fen: &str) -> CastlingMode {
    let field = fen.split_whitespace().nth(2).unwrap_or("-");
    if field != "-"
        && field
            .chars()
            .any(|c| !matches!(c, 'K' | 'Q' | 'k' | 'q' | '-'))
    {
        CastlingMode::Chess960
    } else {
        CastlingMode::Standard
    }
}

/// Parse a FEN as standard chess, falling back to Chess960 when the position
/// is only valid with 960 castling rules (king not on the e-file).
pub fn parse_chess(fen: &str) -> Result<(Chess, CastlingMode)> {
    let parsed = Fen::from_ascii(fen.as_bytes()).context("invalid FEN")?;
    if castling_mode_for_fen(fen).is_standard() {
        if let Ok(pos) = parsed.clone().into_position(CastlingMode::Standard) {
            return Ok((pos, CastlingMode::Standard));
        }
        // `KQkq` is also valid for a Chess960 setup whose king is not on e1.
        let pos = parsed
            .into_position(CastlingMode::Chess960)
            .context("invalid standard or Chess960 position")?;
        return Ok((pos, CastlingMode::Chess960));
    }
    let pos = parsed
        .into_position(CastlingMode::Chess960)
        .context("invalid Chess960 position")?;
    Ok((pos, CastlingMode::Chess960))
}

pub fn variant_for_name(name: &str) -> Variant {
    if name.eq_ignore_ascii_case("chess960") {
        Variant::Chess
    } else {
        Variant::from_ascii(name.as_bytes()).unwrap_or(Variant::Chess)
    }
}

pub fn parse_variant_fen(
    fen: &str,
    variant: Variant,
    mode: CastlingMode,
) -> Result<(VariantPosition, CastlingMode)> {
    let parsed = Fen::from_ascii(fen.as_bytes()).context("invalid FEN")?;
    let setup = parsed.as_setup().clone();
    let pos = VariantPosition::from_setup(variant, setup, mode)
        .map_err(|e| anyhow::anyhow!("invalid {:?} position: {e:?}", variant))?;
    Ok((pos, mode))
}

fn castling_field(pos: &VariantPosition) -> String {
    Fen::from_position(pos, EnPassantMode::Legal)
        .to_string()
        .split_whitespace()
        .nth(2)
        .unwrap_or("-")
        .to_owned()
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
        let variant = variant_for_name(&record.variant);
        let chess960 = record.variant.eq_ignore_ascii_case("chess960");
        let mode = if variant == Variant::Chess {
            if chess960 {
                CastlingMode::Chess960
            } else {
                castling_mode_for_fen(&record.initial_fen)
            }
        } else {
            CastlingMode::Standard
        };
        let (board, mode) = if variant == Variant::Chess && record.variant.eq_ignore_ascii_case("standard") {
            // Auto-detect a Chess960 setup when a standard-tagged FEN needs it.
            let (pos, mode) = parse_chess(&record.initial_fen)?;
            (VariantPosition::from(pos), mode)
        } else {
            parse_variant_fen(&record.initial_fen, variant, mode)?
        };
        let mut initial = record.clone();
        initial.moves.clear();
        let mut state = Self {
            board,
            variant,
            mode,
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

    /// The canonical position (any shakmaty variant).
    pub fn position(&self) -> &VariantPosition {
        &self.board
    }

    pub fn variant(&self) -> Variant {
        self.variant
    }

    pub fn castling_mode(&self) -> CastlingMode {
        self.mode
    }

    pub fn board(&self) -> &shakmaty::Board {
        self.board.board()
    }

    pub fn fen(&self) -> String {
        Fen::from_position(&self.board, EnPassantMode::Legal).to_string()
    }

    pub fn record(&self) -> &GameRecord {
        &self.record
    }

    pub fn position_key(&self) -> String {
        let key = Fen::from_position(&self.board, EnPassantMode::Legal)
            .to_string()
            .split_whitespace()
            .take(4)
            .collect::<Vec<_>>()
            .join(" ");
        if self.variant == Variant::Chess {
            key
        } else {
            format!("{}|{key}", self.variant.uci())
        }
    }

    pub fn legal_moves(&self) -> Vec<String> {
        if self.facts().terminal_reason.is_some() {
            return vec![];
        }
        let mut moves: Vec<String> = self
            .board
            .legal_moves()
            .into_iter()
            .map(|m| UciMove::from_move(m, self.mode).to_string())
            .collect();
        moves.sort();
        moves
    }

    pub fn is_legal(&self, uci: &str) -> bool {
        self.facts().terminal_reason.is_none()
            && UciMove::from_ascii(uci.as_bytes())
                .ok()
                .and_then(|u| u.to_move(&self.board).ok())
                .is_some()
    }

    pub fn play(&mut self, uci: &str) -> Result<()> {
        ensure!(
            self.facts().terminal_reason.is_none(),
            "game is already terminal"
        );
        let mv = UciMove::from_ascii(uci.as_bytes())
            .context("malformed UCI move")?
            .to_move(&self.board)
            .context("illegal move")?;
        let zeroing = mv.is_zeroing();
        let rights_before = castling_field(&self.board);
        let next = self.board.clone().play(mv).context("illegal move")?;
        self.board = next;
        let rights_after = castling_field(&self.board);
        if zeroing || rights_before != rights_after {
            self.repetitions.clear();
        }
        *self.repetitions.entry(self.position_key()).or_default() += 1;
        self.record.moves.push(uci.to_owned());
        Ok(())
    }

    pub fn facts(&self) -> RuleFacts {
        let in_check = self.board.checkers().any();
        let no_moves = self.board.legal_moves().is_empty();
        let checkmate = no_moves && in_check;
        let stalemate = no_moves && !in_check;
        let count = self
            .record
            .history_complete
            .then(|| *self.repetitions.get(&self.position_key()).unwrap_or(&0));
        let dead = self.elementary_dead_position().then_some(true);
        let halfmove = self.board.halfmoves();
        let terminal_reason = if checkmate {
            Some("checkmate")
        } else if stalemate {
            Some("stalemate")
        } else if dead == Some(true) {
            Some("proven_dead_position")
        } else if count.is_some_and(|n| n >= 5) {
            Some("fivefold_repetition")
        } else if halfmove >= 150 {
            Some("seventy_five_move")
        } else {
            match self.board.outcome().known() {
                Some(shakmaty::KnownOutcome::Decisive { winner }) => {
                    Some(if winner == Color::White { "white_wins" } else { "black_wins" })
                }
                Some(shakmaty::KnownOutcome::Draw) => Some("draw"),
                None => None,
            }
        }
        .map(str::to_owned);
        let active = terminal_reason.is_none();
        RuleFacts {
            in_check,
            checkmate,
            stalemate,
            repetition_count: count,
            threefold_claim_now: count.map(|n| active && n >= 3),
            fifty_move_claim_now: active && halfmove >= 100,
            fivefold_automatic: count.map(|n| !checkmate && n >= 5),
            seventy_five_move_automatic: !checkmate && halfmove >= 150,
            dead_position: dead,
            terminal_reason,
        }
    }

    fn elementary_dead_position(&self) -> bool {
        if self.variant != Variant::Chess {
            return false;
        }
        let mut minors = 0;
        for square in Square::ALL {
            match self.board.board().piece_at(square) {
                Some(p) => match p.role {
                    Role::Pawn | Role::Rook | Role::Queen => return false,
                    Role::Bishop | Role::Knight => minors += 1,
                    Role::King => {}
                },
                None => {}
            }
        }
        minors <= 1 // K vs K, K+B vs K, K+N vs K only; no universal dead-position claim.
    }

    pub fn compile(&self, options: PacketOptions) -> ContextPacket {
        ContextPacket {
            schema_version: SCHEMA_VERSION,
            ruleset: if self.variant == Variant::Chess {
                "standard-untimed-v1".to_owned()
            } else {
                format!("{}-untimed-v1", self.variant.uci())
            },
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
        let board = self.board.board();
        for color in Color::ALL {
            let mut attacked = std::collections::BTreeSet::new();
            for square in board.by_color(color) {
                let role = board.role_at(square).expect("piece has a role");
                *material_counts
                    .entry(format!("{color:?}_{role:?}").to_lowercase())
                    .or_default() += 1;
                for target in board.attacks_from(square) {
                    attacked.insert(target.to_string());
                }
            }
            attacks.insert(
                format!("{color:?}").to_lowercase(),
                attacked.into_iter().collect(),
            );
        }
        let mut pinned_to_move: Vec<_> = self
            .pinned_to_move()
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

    /// Absolute pins of the side to move: a piece whose removal exposes its king
    /// to an enemy slider. (shakmaty has no direct `pinned()` in 0.30.)
    fn pinned_to_move(&self) -> Vec<Square> {
        let board = self.board.board();
        let us = self.board.turn();
        let them = us.other();
        let Some(king) = board.king_of(us) else {
            return Vec::new();
        };
        let occupied = board.occupied();
        let mut pinned = Vec::new();
        for square in board.by_color(us) {
            if square == king {
                continue;
            }
            if board.attacks_to(king, them, occupied.without(square)).any() {
                pinned.push(square);
            }
        }
        pinned
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
