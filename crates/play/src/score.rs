//! Post-decision scoring with Stockfish: centipawn loss of a chosen move and
//! MultiPV margins for deriving a tactical set. The model has already
//! committed to its move before anything here runs (WAR-0005 engine-zero).

use anyhow::{Context, Result};
use knightlm_core::SCHEMA_VERSION;
use knightlm_core::state::{GameRecord, GameState};
use knightlm_engine::{Analysis, Engine};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, Write};

/// Scores are clipped to this many centipawns (a mate counts as the clip).
pub const CLIP_CP: i32 = 1000;

/// What the scorer needs from an engine; `Engine` implements it, tests fake it.
pub trait Analyser {
    fn analyse(&mut self, fen: &str, depth: u32) -> Result<Analysis>;
    fn analyse_lines(&mut self, fen: &str, depth: u32, lines: u32) -> Result<Vec<Analysis>>;
    fn version(&self) -> String;
}

impl Analyser for Engine {
    fn analyse(&mut self, fen: &str, depth: u32) -> Result<Analysis> {
        self.new_game()?;
        Engine::analyse(self, fen, depth)
    }
    fn analyse_lines(&mut self, fen: &str, depth: u32, lines: u32) -> Result<Vec<Analysis>> {
        self.new_game()?;
        Engine::analyse_lines(self, fen, depth, lines)
    }
    fn version(&self) -> String {
        self.name().to_owned()
    }
}

#[derive(Debug, Deserialize)]
pub struct MoveRow {
    #[serde(default)]
    pub id: Value,
    pub fen: String,
    #[serde(rename = "move")]
    pub mv: String,
}

#[derive(Debug, Deserialize)]
pub struct PositionRow {
    #[serde(default)]
    pub id: Value,
    pub fen: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct MoveScore {
    pub id: Value,
    pub fen: String,
    #[serde(rename = "move")]
    pub mv: String,
    /// `scored`, `illegal` (not legal in the position) or `invalid_fen`.
    pub status: &'static str,
    pub best_move: Option<String>,
    pub best_cp: Option<i32>,
    pub played_cp: Option<i32>,
    pub cpl: Option<i32>,
    pub depth: u32,
    pub engine: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct LineScore {
    #[serde(rename = "move")]
    pub mv: String,
    pub cp: Option<i32>,
    pub mate: Option<i32>,
    pub clipped_cp: i32,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct PositionRank {
    pub id: Value,
    pub fen: String,
    pub status: &'static str,
    pub best_move: Option<String>,
    pub lines: Vec<LineScore>,
    /// Clipped best minus clipped second-best; `None` with a single legal move.
    pub margin_cp: Option<i32>,
    pub depth: u32,
    pub engine: String,
}

/// Side-to-move score of an analysis, clipped to +-CLIP_CP; mate counts as the clip.
pub fn clipped(cp: Option<i32>, mate: Option<i32>) -> i32 {
    match (mate, cp) {
        (Some(m), _) if m > 0 => CLIP_CP,
        (Some(_), _) => -CLIP_CP,
        (None, Some(c)) => c.clamp(-CLIP_CP, CLIP_CP),
        (None, None) => 0,
    }
}

fn state_from_fen(fen: &str) -> Result<GameState> {
    GameState::replay(&GameRecord {
        schema_version: SCHEMA_VERSION,
        game_id: "score".into(),
        source: "score".into(),
        initial_fen: fen.into(),
        history_complete: false,
        moves: vec![],
        variant: "standard".into(),
    })
}

/// Centipawn loss of `mv` in `fen`: best-move score at `depth` minus the score
/// of the position after `mv` at `depth`, both from the mover's side, clipped.
pub fn score_move(an: &mut dyn Analyser, row: &MoveRow, depth: u32) -> Result<MoveScore> {
    let engine = an.version();
    let mut out = MoveScore {
        id: row.id.clone(),
        fen: row.fen.clone(),
        mv: row.mv.clone(),
        status: "invalid_fen",
        best_move: None,
        best_cp: None,
        played_cp: None,
        cpl: None,
        depth,
        engine,
    };
    let Ok(mut state) = state_from_fen(&row.fen) else {
        return Ok(out);
    };
    if !state.is_legal(&row.mv) {
        out.status = "illegal";
        return Ok(out);
    }
    let best = an.analyse(&row.fen, depth).context("analyse position")?;
    let best_cp = clipped(best.score_cp, best.mate);
    out.best_move = Some(best.best_move.clone());
    out.best_cp = Some(best_cp);
    out.status = "scored";
    let played_cp = if best.best_move == row.mv {
        best_cp
    } else {
        state.play(&row.mv)?;
        let f = state.facts();
        if f.checkmate {
            CLIP_CP
        } else if f.terminal_reason.is_some() {
            0
        } else {
            let after = an
                .analyse(&state.fen(), depth)
                .context("analyse position after move")?;
            -clipped(after.score_cp, after.mate)
        }
    };
    out.played_cp = Some(played_cp);
    out.cpl = Some((best_cp - played_cp).max(0));
    Ok(out)
}

/// MultiPV ranking of a position: top `lines` moves with scores and the margin
/// between the best and second-best move.
pub fn rank_position(
    an: &mut dyn Analyser,
    row: &PositionRow,
    depth: u32,
    lines: u32,
) -> Result<PositionRank> {
    let engine = an.version();
    let mut out = PositionRank {
        id: row.id.clone(),
        fen: row.fen.clone(),
        status: "invalid_fen",
        best_move: None,
        lines: vec![],
        margin_cp: None,
        depth,
        engine,
    };
    let Ok(state) = state_from_fen(&row.fen) else {
        return Ok(out);
    };
    if state.legal_moves().is_empty() {
        out.status = "terminal";
        return Ok(out);
    }
    let found = an
        .analyse_lines(&row.fen, depth, lines)
        .context("multipv analysis")?;
    out.status = "ranked";
    out.lines = found
        .iter()
        .map(|a| LineScore {
            mv: a.best_move.clone(),
            cp: a.score_cp,
            mate: a.mate,
            clipped_cp: clipped(a.score_cp, a.mate),
        })
        .collect();
    out.best_move = out.lines.first().map(|l| l.mv.clone());
    if out.lines.len() >= 2 {
        out.margin_cp = Some(out.lines[0].clipped_cp - out.lines[1].clipped_cp);
    }
    Ok(out)
}

/// Stream JSONL `{id?, fen, move}` rows to JSONL scores. A row that cannot be
/// parsed is an error (the caller owns the input), never silently skipped.
pub fn score_moves(
    an: &mut dyn Analyser,
    input: impl BufRead,
    mut output: impl Write,
    depth: u32,
) -> Result<usize> {
    let mut n = 0;
    for (i, line) in input.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let row: MoveRow =
            serde_json::from_str(&line).with_context(|| format!("input line {}", i + 1))?;
        writeln!(
            output,
            "{}",
            serde_json::to_string(&score_move(an, &row, depth)?)?
        )?;
        n += 1;
    }
    Ok(n)
}

pub fn rank_positions(
    an: &mut dyn Analyser,
    input: impl BufRead,
    mut output: impl Write,
    depth: u32,
    lines: u32,
) -> Result<usize> {
    let mut n = 0;
    for (i, line) in input.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let row: PositionRow =
            serde_json::from_str(&line).with_context(|| format!("input line {}", i + 1))?;
        writeln!(
            output,
            "{}",
            serde_json::to_string(&rank_position(an, &row, depth, lines)?)?
        )?;
        n += 1;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Scripted engine: FEN -> (best move, cp) lines.
    type Script = Vec<(String, Option<i32>, Option<i32>)>;
    struct Fake(HashMap<String, Script>);
    impl Fake {
        fn lines(&self, fen: &str) -> Result<Vec<Analysis>> {
            let v = self
                .0
                .get(fen)
                .with_context(|| format!("unscripted {fen}"))?;
            Ok(v.iter()
                .enumerate()
                .map(|(i, (m, cp, mate))| Analysis {
                    engine: "fake".into(),
                    version: "0".into(),
                    depth: 1,
                    best_move: m.clone(),
                    score_cp: *cp,
                    mate: *mate,
                    pv: vec![],
                    nodes: None,
                    nps: None,
                    multipv: Some(i as u16 + 1),
                })
                .collect())
        }
    }
    impl Analyser for Fake {
        fn analyse(&mut self, fen: &str, _: u32) -> Result<Analysis> {
            Ok(self.lines(fen)?.remove(0))
        }
        fn analyse_lines(&mut self, fen: &str, _: u32, n: u32) -> Result<Vec<Analysis>> {
            Ok(self.lines(fen)?.into_iter().take(n as usize).collect())
        }
        fn version(&self) -> String {
            "fake 0".into()
        }
    }

    const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    const AFTER_A3: &str = "rnbqkbnr/pppppppp/8/8/8/P7/1PPPPPPP/RNBQKBNR b KQkq - 0 1";

    fn fake() -> Fake {
        let mut m = HashMap::new();
        m.insert(
            START.to_owned(),
            vec![
                ("e2e4".to_owned(), Some(40), None),
                ("a2a3".to_owned(), Some(-20), None),
            ],
        );
        // Black to move after a3, black's own best score is +10 -> mover (white) -10.
        m.insert(
            AFTER_A3.to_owned(),
            vec![("e7e5".to_owned(), Some(10), None)],
        );
        Fake(m)
    }

    fn row(mv: &str) -> MoveRow {
        MoveRow {
            id: Value::Null,
            fen: START.into(),
            mv: mv.into(),
        }
    }

    #[test]
    fn best_move_has_zero_loss() {
        let s = score_move(&mut fake(), &row("e2e4"), 5).unwrap();
        assert_eq!(
            (s.status, s.cpl, s.best_cp, s.played_cp),
            ("scored", Some(0), Some(40), Some(40))
        );
    }

    #[test]
    fn weaker_move_loses_the_difference() {
        let s = score_move(&mut fake(), &row("a2a3"), 5).unwrap();
        assert_eq!(s.played_cp, Some(-10));
        assert_eq!(s.cpl, Some(50));
    }

    #[test]
    fn illegal_and_garbage_are_marked_not_scored() {
        for mv in ["e2e5", "zzzz", "", "e2e4 "] {
            let s = score_move(&mut fake(), &row(mv), 5).unwrap();
            assert_eq!(s.status, "illegal", "{mv:?}");
            assert_eq!(s.cpl, None);
        }
        let bad = MoveRow {
            id: Value::Null,
            fen: "not a fen".into(),
            mv: "e2e4".into(),
        };
        assert_eq!(
            score_move(&mut fake(), &bad, 5).unwrap().status,
            "invalid_fen"
        );
    }

    #[test]
    fn mate_scores_clip_and_delivering_mate_is_best() {
        assert_eq!(clipped(None, Some(3)), 1000);
        assert_eq!(clipped(None, Some(-2)), -1000);
        assert_eq!(clipped(Some(5000), None), 1000);
        // Fool's-mate position: Qh4# is a mate delivered.
        let fen = "rnbqkbnr/pppp1ppp/8/4p3/6P1/5P2/PPPPP2P/RNBQKBNR b KQkq - 0 2";
        let mut m = HashMap::new();
        m.insert(fen.to_owned(), vec![("d8h4".to_owned(), None, Some(1))]);
        let r = MoveRow {
            id: Value::Null,
            fen: fen.into(),
            mv: "d8h4".into(),
        };
        let s = score_move(&mut Fake(m), &r, 5).unwrap();
        assert_eq!((s.cpl, s.played_cp), (Some(0), Some(1000)));
    }

    #[test]
    fn rank_reports_margin_and_streams_jsonl() {
        let input = format!("{{\"id\":7,\"fen\":\"{START}\"}}\n");
        let mut out = Vec::new();
        rank_positions(&mut fake(), input.as_bytes(), &mut out, 5, 2).unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["margin_cp"], 60);
        assert_eq!(v["best_move"], "e2e4");
        assert_eq!(v["id"], 7);
    }

    #[test]
    fn unparseable_input_is_an_error() {
        let mut out = Vec::new();
        assert!(score_moves(&mut fake(), "{oops".as_bytes(), &mut out, 5).is_err());
    }
}
