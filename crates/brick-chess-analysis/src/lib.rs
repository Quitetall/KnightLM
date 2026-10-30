//! BrickChess analysis: turn engine evaluations into move labels and accuracy.
//!
//! Two classification systems are produced side by side:
//!
//! 1. **Lichess-style** — win probability from centipawns, with the published
//!    Lichess thresholds (10% win-chance drop: inaccuracy, 20%: mistake,
//!    30%: blunder) and the Lichess accuracy formula.
//! 2. **Expected Points (clean-room)** — a logistic expected-points model with
//!    the public chess.com V2 threshold bands (0.02 / 0.05 / 0.10 / 0.20).
//!    This is an independent implementation of the *published model shape*, not
//!    a copy of chess.com's proprietary code or constants.
//!
//! The crate is pure: callers supply evaluations (from Stockfish or any engine)
//! and get a report. It does not spawn an engine.

use serde::{Deserialize, Serialize};

/// A score for the side to move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Eval {
    /// Centipawns, positive = good for the side to move.
    Cp(i32),
    /// Mate in N moves, positive = the side to move delivers mate.
    Mate(i32),
}

impl Eval {
    /// Centipawns, with mates mapped to a large magnitude.
    pub fn cp(self) -> i32 {
        match self {
            Eval::Cp(c) => c,
            Eval::Mate(n) => {
                if n > 0 {
                    100_000 - n * 100
                } else {
                    -100_000 - n * 100
                }
            }
        }
    }

    /// Win probability in percent (Lichess mapping).
    pub fn win_percent(self) -> f64 {
        match self {
            Eval::Mate(n) if n > 0 => 100.0,
            Eval::Mate(_) => 0.0,
            Eval::Cp(c) => win_percent_cp(c as f64),
        }
    }

    /// Clean-room expected points in `0.0..=1.0`.
    ///
    /// `scale_cp` is the centipawn value at which expected points reach ~0.9
    /// (400 is a reasonable default: +4 is a near-won game).
    pub fn expected_points(self, scale_cp: f64) -> f64 {
        match self {
            Eval::Mate(n) if n > 0 => 1.0,
            Eval::Mate(_) => 0.0,
            Eval::Cp(c) => expected_points_cp(c as f64, scale_cp),
        }
    }
}

/// Lichess win-probability mapping: `50 + 50 * (2/(1+exp(-k*cp)) - 1)`.
pub fn win_percent_cp(cp: f64) -> f64 {
    const K: f64 = 0.00368208;
    (50.0 + 50.0 * (2.0 / (1.0 + (-K * cp).exp()) - 1.0)).clamp(0.0, 100.0)
}

/// Clean-room expected points: logistic in centipawns, 0.5 at equality.
pub fn expected_points_cp(cp: f64, scale_cp: f64) -> f64 {
    let scale = if scale_cp.is_finite() && scale_cp > 0.0 {
        scale_cp
    } else {
        400.0
    };
    let x = cp * std::f64::consts::LN_10 / scale;
    (1.0 / (1.0 + (-x).exp())).clamp(0.0, 1.0)
}

/// Lichess accuracy percentage for a win-chance drop (in percentage points).
pub fn accuracy_percent(win_drop: f64) -> f64 {
    let drop = win_drop.max(0.0);
    (103.1668 * (-0.04354 * drop).exp() - 3.1669).clamp(0.0, 100.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoveLabel {
    Best,
    Excellent,
    Good,
    Inaccuracy,
    Mistake,
    Blunder,
}

impl MoveLabel {
    fn index(self) -> usize {
        match self {
            MoveLabel::Best => 0,
            MoveLabel::Excellent => 1,
            MoveLabel::Good => 2,
            MoveLabel::Inaccuracy => 3,
            MoveLabel::Mistake => 4,
            MoveLabel::Blunder => 5,
        }
    }
}

/// Lichess-style label from a win-chance drop (percentage points).
pub fn classify_lichess(win_drop: f64, is_best: bool) -> MoveLabel {
    if is_best {
        return MoveLabel::Best;
    }
    let d = win_drop.max(0.0);
    if d >= 30.0 {
        MoveLabel::Blunder
    } else if d >= 20.0 {
        MoveLabel::Mistake
    } else if d >= 10.0 {
        MoveLabel::Inaccuracy
    } else {
        MoveLabel::Good
    }
}

/// Clean-room Expected Points V2 label from an expected-points loss.
pub fn classify_expected_points(ep_loss: f64, is_best: bool) -> MoveLabel {
    if is_best {
        return MoveLabel::Best;
    }
    let d = ep_loss.max(0.0);
    if d >= 0.20 {
        MoveLabel::Blunder
    } else if d >= 0.10 {
        MoveLabel::Mistake
    } else if d >= 0.05 {
        MoveLabel::Inaccuracy
    } else if d >= 0.02 {
        MoveLabel::Good
    } else {
        MoveLabel::Excellent
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AnalysisConfig {
    /// Expected-points scale in centipawns (default 400).
    pub ep_scale_cp: f64,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self { ep_scale_cp: 400.0 }
    }
}

/// One played move with the evaluations needed to label it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveInput {
    pub san: String,
    pub uci: String,
    /// Evaluation before the move, from the mover's point of view.
    pub eval_before: Eval,
    /// Evaluation after the move, from the mover's point of view.
    pub eval_after: Eval,
    /// The engine's best move in the position before (UCI).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_move: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LabelCounts {
    pub best: usize,
    pub excellent: usize,
    pub good: usize,
    pub inaccuracy: usize,
    pub mistake: usize,
    pub blunder: usize,
}

impl LabelCounts {
    fn bump(&mut self, label: MoveLabel) {
        match label {
            MoveLabel::Best => self.best += 1,
            MoveLabel::Excellent => self.excellent += 1,
            MoveLabel::Good => self.good += 1,
            MoveLabel::Inaccuracy => self.inaccuracy += 1,
            MoveLabel::Mistake => self.mistake += 1,
            MoveLabel::Blunder => self.blunder += 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzedMove {
    pub ply: usize,
    pub san: String,
    pub uci: String,
    pub lichess: MoveLabel,
    pub expected_points: MoveLabel,
    pub win_before: f64,
    pub win_after: f64,
    pub ep_before: f64,
    pub ep_after: f64,
    pub accuracy: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_move: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameReport {
    pub moves: Vec<AnalyzedMove>,
    pub white_accuracy: Option<f64>,
    pub black_accuracy: Option<f64>,
    pub lichess_counts: LabelCounts,
    pub expected_points_counts: LabelCounts,
}

/// Label a sequence of moves. `ply` 0 is White's first move.
pub fn analyze(moves: &[MoveInput], config: &AnalysisConfig) -> GameReport {
    let mut out = Vec::with_capacity(moves.len());
    let mut lichess_counts = LabelCounts::default();
    let mut ep_counts = LabelCounts::default();
    let mut white_sum = 0.0;
    let mut white_n = 0usize;
    let mut black_sum = 0.0;
    let mut black_n = 0usize;

    for (ply, m) in moves.iter().enumerate() {
        let win_before = m.eval_before.win_percent();
        let win_after = m.eval_after.win_percent();
        let win_drop = (win_before - win_after).max(0.0);
        let ep_before = m.eval_before.expected_points(config.ep_scale_cp);
        let ep_after = m.eval_after.expected_points(config.ep_scale_cp);
        let ep_loss = (ep_before - ep_after).max(0.0);
        let is_best = m.best_move.as_deref() == Some(m.uci.as_str());
        let lichess = classify_lichess(win_drop, is_best);
        let expected_points = classify_expected_points(ep_loss, is_best);
        let acc = accuracy_percent(win_drop);

        lichess_counts.bump(lichess);
        ep_counts.bump(expected_points);
        if ply % 2 == 0 {
            white_sum += acc;
            white_n += 1;
        } else {
            black_sum += acc;
            black_n += 1;
        }

        out.push(AnalyzedMove {
            ply,
            san: m.san.clone(),
            uci: m.uci.clone(),
            lichess,
            expected_points,
            win_before,
            win_after,
            ep_before,
            ep_after,
            accuracy: acc,
            best_move: m.best_move.clone(),
        });
    }

    GameReport {
        moves: out,
        white_accuracy: (white_n > 0).then(|| white_sum / white_n as f64),
        black_accuracy: (black_n > 0).then(|| black_sum / black_n as f64),
        lichess_counts,
        expected_points_counts: ep_counts,
    }
}

impl GameReport {
    /// Total number of labelled moves.
    pub fn len(&self) -> usize {
        self.moves.len()
    }
    pub fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }
    /// Counts for a label across both systems, for quick summaries.
    pub fn count(&self, label: MoveLabel) -> (usize, usize) {
        let idx = label.index();
        let pick = |c: &LabelCounts| match idx {
            0 => c.best,
            1 => c.excellent,
            2 => c.good,
            3 => c.inaccuracy,
            4 => c.mistake,
            _ => c.blunder,
        };
        (pick(&self.lichess_counts), pick(&self.expected_points_counts))
    }
}
