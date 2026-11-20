//! The move-player seam. A player is shown a position and answers a move, or
//! fails with an error. The UCI loop, the match driver and any arena depend on
//! this trait alone; engines, served models and baselines sit behind it.

use anyhow::Result;
use knightlm_core::state::GameState;

/// Something that picks moves: a baseline, an external UCI engine, a served model.
pub trait MovePlayer {
    /// The single move to play, in UCI notation.
    fn select(&self, state: &GameState) -> Result<String>;

    /// Up to `n` moves, best first. A player that can only name one move
    /// answers that move; the default does exactly that.
    fn ranked(&self, state: &GameState, n: usize) -> Result<Vec<String>> {
        if n == 0 {
            return Ok(Vec::new());
        }
        Ok(vec![self.select(state)?])
    }

    fn name(&self) -> &str {
        "player"
    }
}

/// A move the player named that is not legal in the position. Typed so a driver
/// can report it as an illegal move rather than a generic failure.
#[derive(Debug)]
pub struct IllegalMove(pub String);

impl std::fmt::Display for IllegalMove {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "illegal move: {}", self.0)
    }
}

impl std::error::Error for IllegalMove {}

#[cfg(test)]
mod tests {
    use super::*;
    use knightlm_core::state::{GameRecord, START_FEN};

    struct Fixed;
    impl MovePlayer for Fixed {
        fn select(&self, _: &GameState) -> Result<String> {
            Ok("e2e4".into())
        }
    }

    fn start() -> GameState {
        GameState::replay(&GameRecord {
            schema_version: knightlm_core::SCHEMA_VERSION,
            game_id: "t".into(),
            source: "t".into(),
            initial_fen: START_FEN.into(),
            history_complete: true,
            moves: vec![],
            variant: "standard".into(),
        })
        .unwrap()
    }

    #[test]
    fn ranked_defaults_to_the_selected_move() {
        let s = start();
        assert_eq!(Fixed.ranked(&s, 3).unwrap(), vec!["e2e4".to_string()]);
        assert!(Fixed.ranked(&s, 0).unwrap().is_empty());
        assert_eq!(Fixed.name(), "player");
    }

    #[test]
    fn illegal_move_downcasts_from_anyhow() {
        let e: anyhow::Error = IllegalMove("e2e5".into()).into();
        assert!(e.downcast_ref::<IllegalMove>().is_some());
    }
}
