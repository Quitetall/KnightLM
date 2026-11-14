use anyhow::{Context, Result};
use knightlm_core::SCHEMA_VERSION;
use knightlm_core::state::{GameRecord, GameState, START_FEN};
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
    /// The error text when a player failed to produce a move (forfeit).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
}

/// Protocol knobs beyond the two players.
#[derive(Debug, Clone, Default)]
pub struct MatchOptions {
    /// Plies (opening included) before the game is drawn as a "move limit".
    pub max_plies: usize,
    /// Fixed opening, UCI moves played before either player is asked. Each
    /// must be legal; a bad opening is a configuration error, not a result.
    pub opening: Vec<String>,
    /// Draw immediately when a threefold-repetition or fifty-move claim is
    /// available, instead of playing on to the automatic fivefold/75-move draw.
    pub claim_draws: bool,
    /// A player error (HTTP failure, unparseable reply, timeout, a model's
    /// illegal move) forfeits the game instead of aborting the match.
    pub forfeit_on_error: bool,
}

/// A move the player named that is not legal in the position. Typed so the
/// driver can report it as an illegal move rather than a generic failure.
#[derive(Debug)]
pub struct IllegalMove(pub String);

impl std::fmt::Display for IllegalMove {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "illegal move: {}", self.0)
    }
}

impl std::error::Error for IllegalMove {}

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
pub fn play_game(white: &dyn Player, black: &dyn Player, max_plies: usize) -> Result<MatchResult> {
    play_game_with(
        white,
        black,
        &MatchOptions {
            max_plies,
            ..Default::default()
        },
    )
}

/// `play_game` with a fixed opening, draw claims and forfeit-on-error.
pub fn play_game_with(
    white: &dyn Player,
    black: &dyn Player,
    opts: &MatchOptions,
) -> Result<MatchResult> {
    let max_plies = opts.max_plies;
    let mut state = start();
    let mut sans = Vec::new();
    let mut moves = Vec::new();
    let mut plies = 0usize;
    for mv in &opts.opening {
        if !state.is_legal(mv) {
            anyhow::bail!("opening move {mv} is not legal at ply {}", plies + 1);
        }
        let san = knightlm_notation::to_san(state.position(), mv).unwrap_or_else(|_| mv.clone());
        state.play(mv).context("opening")?;
        moves.push(mv.clone());
        sans.push(san);
        plies += 1;
    }

    loop {
        let facts = state.facts();
        if facts.checkmate {
            // The side to move is the side that has been mated.
            let result = if state.record().moves.len().is_multiple_of(2) {
                "0-1"
            } else {
                "1-0"
            };
            return Ok(finish(
                moves,
                sans,
                result,
                "checkmate".into(),
                white.name(),
                black.name(),
            ));
        }
        if facts.stalemate {
            return Ok(finish(
                moves,
                sans,
                "1/2-1/2",
                "stalemate".into(),
                white.name(),
                black.name(),
            ));
        }
        if facts.terminal_reason.is_some() {
            return Ok(finish(
                moves,
                sans,
                "1/2-1/2",
                facts
                    .terminal_reason
                    .unwrap_or_else(|| "terminal".to_string()),
                white.name(),
                black.name(),
            ));
        }
        if opts.claim_draws {
            let claim = if facts.threefold_claim_now == Some(true) {
                Some("threefold repetition claimed")
            } else if facts.fifty_move_claim_now {
                Some("fifty-move rule claimed")
            } else {
                None
            };
            if let Some(why) = claim {
                return Ok(finish(
                    moves,
                    sans,
                    "1/2-1/2",
                    why.into(),
                    white.name(),
                    black.name(),
                ));
            }
        }
        if plies >= max_plies {
            return Ok(finish(
                moves,
                sans,
                "1/2-1/2",
                "move limit".into(),
                white.name(),
                black.name(),
            ));
        }
        let white_to_move = state.record().moves.len().is_multiple_of(2);
        let side = if white_to_move { "white" } else { "black" };
        let picked = if white_to_move {
            white.select(&state)
        } else {
            black.select(&state)
        };
        let mv = match picked {
            Ok(mv) => mv,
            Err(e) if opts.forfeit_on_error => {
                let result = if white_to_move { "0-1" } else { "1-0" };
                let illegal = e.downcast_ref::<IllegalMove>().is_some();
                let mut r = finish(
                    moves,
                    sans,
                    result,
                    if illegal {
                        format!("illegal move by {side}")
                    } else {
                        format!("player failure by {side}")
                    },
                    white.name(),
                    black.name(),
                );
                r.failure = Some(format!("{e:#}"));
                return Ok(r);
            }
            Err(e) => return Err(e).context(format!("{side} move")),
        };
        if !state.is_legal(&mv) {
            let result = if white_to_move { "0-1" } else { "1-0" };
            return Ok(finish(
                moves,
                sans,
                result,
                format!(
                    "illegal move by {}",
                    if white_to_move { "white" } else { "black" }
                ),
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
    MatchResult {
        moves,
        sans,
        result: result.to_owned(),
        termination,
        white: white.to_owned(),
        black: black.to_owned(),
        failure: None,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::players::RandomLegal;
    use knightlm_core::state::GameState;

    struct Script(std::cell::RefCell<Vec<Result<String, &'static str>>>);
    impl Player for Script {
        fn select(&self, _: &GameState) -> Result<String> {
            match self.0.borrow_mut().remove(0) {
                Ok(m) => Ok(m),
                Err("illegal") => Err(IllegalMove("e2e5".into()).into()),
                Err(e) => Err(anyhow::anyhow!(e)),
            }
        }
    }
    fn script(v: Vec<Result<String, &'static str>>) -> Script {
        Script(std::cell::RefCell::new(v))
    }
    fn opts() -> MatchOptions {
        MatchOptions {
            max_plies: 40,
            forfeit_on_error: true,
            claim_draws: true,
            ..Default::default()
        }
    }

    #[test]
    fn opening_is_played_and_recorded() {
        let mut o = opts();
        o.opening = vec!["e2e4".into(), "e7e5".into()];
        o.max_plies = 2;
        let r = play_game_with(&RandomLegal { seed: 1 }, &RandomLegal { seed: 2 }, &o).unwrap();
        assert_eq!(r.moves, ["e2e4", "e7e5"]);
        assert_eq!(r.termination, "move limit");
    }

    #[test]
    fn illegal_opening_is_a_config_error() {
        let mut o = opts();
        o.opening = vec!["e2e5".into()];
        let r = RandomLegal { seed: 1 };
        assert!(play_game_with(&r, &r, &o).is_err());
    }

    #[test]
    fn model_illegal_move_loses_and_failure_forfeits() {
        let r = RandomLegal { seed: 1 };
        let g = play_game_with(&script(vec![Err("illegal")]), &r, &opts()).unwrap();
        assert_eq!(
            (g.result.as_str(), g.termination.as_str()),
            ("0-1", "illegal move by white")
        );
        let g = play_game_with(
            &script(vec![Ok("e2e4".into())]),
            &script(vec![Err("timeout")]),
            &opts(),
        )
        .unwrap();
        assert_eq!(
            (g.result.as_str(), g.termination.as_str()),
            ("1-0", "player failure by black")
        );
        assert!(g.failure.unwrap().contains("timeout"));
        // Without forfeit_on_error the same failure aborts the match.
        let mut o = opts();
        o.forfeit_on_error = false;
        assert!(play_game_with(&script(vec![Err("timeout")]), &r, &o).is_err());
    }

    #[test]
    fn checkmate_is_a_win_for_the_mating_side() {
        // Fool's mate: Black mates on ply 4.
        let white = script(vec![Ok("f2f3".into()), Ok("g2g4".into())]);
        let black = script(vec![Ok("e7e5".into()), Ok("d8h4".into())]);
        let g = play_game_with(&white, &black, &opts()).unwrap();
        assert_eq!(
            (g.result.as_str(), g.termination.as_str()),
            ("0-1", "checkmate")
        );
        // Scholar's mate: White mates on ply 7.
        let white = script(vec![
            Ok("e2e4".into()),
            Ok("f1c4".into()),
            Ok("d1h5".into()),
            Ok("h5f7".into()),
        ]);
        let black = script(vec![
            Ok("e7e5".into()),
            Ok("b8c6".into()),
            Ok("g8f6".into()),
        ]);
        let g = play_game_with(&white, &black, &opts()).unwrap();
        assert_eq!(
            (g.result.as_str(), g.termination.as_str()),
            ("1-0", "checkmate")
        );
    }

    #[test]
    fn threefold_repetition_is_claimed_as_a_draw() {
        let shuffle = |a: &str, b: &str| {
            script(vec![
                Ok(a.into()),
                Ok(b.into()),
                Ok(a.into()),
                Ok(b.into()),
                Ok(a.into()),
            ])
        };
        let g =
            play_game_with(&shuffle("g1f3", "f3g1"), &shuffle("g8f6", "f6g8"), &opts()).unwrap();
        assert_eq!(g.result, "1/2-1/2");
        assert_eq!(g.termination, "threefold repetition claimed");
    }
}
