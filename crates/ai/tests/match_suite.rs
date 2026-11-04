//! Match-level behaviour with scripted models: move sequences, a model that
//! has to be corrected, provider failures that must not decide a game, and the
//! three ways a game ends.
//!
//! These are the match's rules, not the wire's: `runtime_mock.rs` covers the
//! HTTP side with a real socket. One command runs everything here:
//!
//! ```sh
//! cargo test -p rbc-ai --all-features
//! ```
#![cfg(feature = "provider")]

use std::sync::Mutex;

use rbc_ai::game::{Match, MatchConfig, MatchState, Player, TimeControl};
use rbc_ai::play::{WatchSeats, watch_seats, watch_step};
use rbc_ai::protocol::{ErrorInfo, StopReason};
use rbc_ai::provider::{
    Cancellation, CompletionRequest, CompletionResponse, Provider, ProviderEvent, error_code,
};

// --- a scripted model ------------------------------------------------------

enum Scripted {
    Reply(String),
    Fail(ErrorInfo),
}

struct Script {
    steps: Mutex<Vec<Scripted>>,
    asked: Mutex<usize>,
}

impl Script {
    fn replies(moves: &[&str]) -> Self {
        Self {
            steps: Mutex::new(
                moves
                    .iter()
                    .map(|m| Scripted::Reply((*m).to_string()))
                    .collect(),
            ),
            asked: Mutex::new(0),
        }
    }

    fn failing(error: ErrorInfo) -> Self {
        Self {
            steps: Mutex::new(vec![Scripted::Fail(error)]),
            asked: Mutex::new(0),
        }
    }

    fn asked(&self) -> usize {
        *self.asked.lock().expect("asked")
    }
}

impl Provider for Script {
    fn name(&self) -> &str {
        "scripted"
    }

    fn complete(
        &self,
        _request: &CompletionRequest,
        _cancel: &Cancellation,
        _on_event: &mut dyn FnMut(ProviderEvent),
    ) -> Result<CompletionResponse, ErrorInfo> {
        *self.asked.lock().expect("asked") += 1;
        let mut steps = self.steps.lock().expect("steps");
        if steps.is_empty() {
            return Err(ErrorInfo {
                code: "scripted_exhausted".into(),
                message: "no reply left".into(),
                retryable: false,
                provider_code: None,
            });
        }
        match steps.remove(0) {
            Scripted::Reply(content) => Ok(CompletionResponse {
                content,
                reasoning: None,
                tool_calls: Vec::new(),
                usage: Default::default(),
                stop_reason: StopReason::EndTurn,
                provider_request_id: None,
            }),
            Scripted::Fail(error) => Err(error),
        }
    }
}

fn two_models(config: MatchConfig) -> Match {
    Match::new(config).expect("match")
}

fn seats(game: &Match) -> WatchSeats {
    watch_seats(game.config()).expect("two models")
}

fn watching() -> MatchConfig {
    MatchConfig {
        white: Player::Model {
            provider: "scripted".into(),
            model: "w".into(),
        },
        black: Player::Model {
            provider: "scripted".into(),
            model: "b".into(),
        },
        ..MatchConfig::default()
    }
}

// --- move sequences --------------------------------------------------------

#[test]
fn match_scripts_an_opening_sequence() {
    let mut game = two_models(watching());
    let script = Script::replies(&["e4", "e5", "Nf3", "Nc6", "Bb5", "a6"]);
    let seats = seats(&game);

    for _ in 0..6 {
        watch_step(&mut game, &script, &seats, &Cancellation::new()).expect("legal");
    }

    let sans: Vec<String> = game.moves().iter().map(|mv| mv.san.clone()).collect();
    assert_eq!(sans, ["e4", "e5", "Nf3", "Nc6", "Bb5", "a6"]);
    assert_eq!(script.asked(), 6, "one request per ply");
    // The PGN carries the same moves in order.
    let pgn = game.pgn();
    assert!(pgn.contains("1. e4 e5 2. Nf3 Nc6 3. Bb5 a6"), "{pgn}");
}

// --- a model that has to be corrected --------------------------------------

#[test]
fn match_retries_an_illegal_move_then_continues() {
    let mut game = two_models(watching());
    // "e5" as white is illegal; "Nf3" is not.
    let script = Script::replies(&["e5", "Nf3", "Nf6"]);
    let seats = seats(&game);

    let first = watch_step(&mut game, &script, &seats, &Cancellation::new()).expect("retried");
    assert_eq!(first.mv.san, "Nf3");
    // The illegal reply and the legal one were both this step's attempts.
    assert_eq!(first.attempts, 2, "the retry happened inside one step");

    watch_step(&mut game, &script, &seats, &Cancellation::new()).expect("black replies");
    assert_eq!(game.moves().len(), 2);
}

#[test]
fn match_forfeits_the_turn_when_no_legal_move_comes() {
    // Three illegal replies and no legal one: the turn fails and the board is
    // untouched, which is a forfeited turn rather than a corrupt game.
    let mut config = watching();
    config.max_retries = 3;
    let mut game = two_models(config);
    let script = Script::replies(&["e5", "e5", "e5"]);
    let seats = seats(&game);

    let failure =
        watch_step(&mut game, &script, &seats, &Cancellation::new()).expect_err("no legal move");

    assert_eq!(failure.code, error_code::PROTOCOL);
    assert!(game.moves().is_empty(), "nothing was recorded");
    assert_eq!(
        game.state(),
        &MatchState::AwaitingWhite,
        "it is still white's turn"
    );
    assert!(game.result().is_none(), "a forfeited turn is not a result");
}

// --- provider failures do not decide a game --------------------------------

#[test]
fn match_leaves_the_game_unrated_when_the_provider_times_out() {
    let mut game = two_models(watching());
    let script = Script::failing(ErrorInfo {
        code: error_code::TIMEOUT.to_string(),
        message: "the endpoint did not answer".into(),
        retryable: true,
        provider_code: None,
    });
    let seats = seats(&game);

    let failure =
        watch_step(&mut game, &script, &seats, &Cancellation::new()).expect_err("timeout");

    assert_eq!(failure.code, error_code::TIMEOUT);
    assert!(game.result().is_none(), "a timeout is not a draw or a loss");
    assert!(game.moves().is_empty());
    assert_eq!(game.state(), &MatchState::AwaitingWhite);
    // Nothing was written, so a match record saved here would carry no result.
    assert!(game.result().is_none(), "nothing to record");
}

#[test]
fn match_leaves_the_game_unrated_when_the_provider_errors() {
    let mut game = two_models(watching());
    let script = Script::failing(ErrorInfo {
        code: error_code::SERVER.to_string(),
        message: "upstream exploded".into(),
        retryable: true,
        provider_code: None,
    });
    let seats = seats(&game);

    let failure =
        watch_step(&mut game, &script, &seats, &Cancellation::new()).expect_err("server error");
    assert_eq!(failure.code, error_code::SERVER);
    assert!(game.result().is_none());
    assert_eq!(game.moves().len(), 0);
}

// --- how a game ends -------------------------------------------------------

#[test]
fn match_ends_on_checkmate() {
    // 1. f3 e5 2. g4 Qh4#: the shortest mate there is.
    let mut game = two_models(watching());
    let script = Script::replies(&["f3", "e5", "g4", "Qh4#"]);
    let seats = seats(&game);

    for ply in 0..4 {
        watch_step(&mut game, &script, &seats, &Cancellation::new())
            .unwrap_or_else(|error| panic!("ply {ply}: {}", error.message));
    }

    let result = game.result().expect("a result");
    assert_eq!(game.state(), &MatchState::Finished);
    assert_eq!(result.score, "0-1", "black delivered mate");
    assert!(
        result.termination.contains("mate"),
        "{}",
        result.termination
    );
    assert!(game.legal_moves().is_empty(), "the game is over");
    // A finished game refuses further play.
    assert!(watch_step(&mut game, &script, &seats, &Cancellation::new()).is_err());
}

#[test]
fn match_ends_on_resignation() {
    let mut game = two_models(watching());
    let script = Script::replies(&["e4"]);
    let seats = seats(&game);
    watch_step(&mut game, &script, &seats, &Cancellation::new()).expect("e4");

    game.resign(shakmaty::Color::White);

    let result = game.result().expect("a result");
    assert_eq!(result.score, "0-1", "white resigned");
    assert_eq!(game.state(), &MatchState::Finished);
    assert!(
        result.termination.contains("resign"),
        "{}",
        result.termination
    );
}

#[test]
fn match_ends_at_the_ply_limit() {
    let mut config = watching();
    config.max_plies = 2;
    let mut game = two_models(config);
    let script = Script::replies(&["e4", "e5"]);
    let seats = seats(&game);

    watch_step(&mut game, &script, &seats, &Cancellation::new()).expect("first");
    watch_step(&mut game, &script, &seats, &Cancellation::new()).expect("second");

    assert_eq!(game.state(), &MatchState::Finished);
    let result = game.result().expect("a result");
    assert!(
        result.termination.contains("plies"),
        "{}",
        result.termination
    );
    assert_eq!(result.score, "1/2-1/2", "a truncated game scores as a draw");
}

// --- clocks ----------------------------------------------------------------

#[test]
fn match_clock_runs_and_a_flag_falls() {
    let mut config = watching();
    config.time_control = Some(TimeControl {
        initial_ms: 1_000,
        increment_ms: 0,
    });
    let mut game = two_models(config);
    let script = Script::replies(&["e4", "e5"]);
    let seats = seats(&game);

    // The clock starts with the match, so it has to be ticked once.
    game.tick(0);
    watch_step(&mut game, &script, &seats, &Cancellation::new()).expect("e4");

    // After e4 it is black's turn, so it is black's clock that runs out.
    let flagged = game.tick(1_500).expect("the side to move ran out of time");
    assert_eq!(flagged.score, "1-0", "black flagged");
    assert!(
        flagged.termination.contains("time"),
        "{}",
        flagged.termination
    );
}
