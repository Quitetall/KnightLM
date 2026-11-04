//! The match state machine: rules, terminations, clocks, PGN and persistence.
//!
//! A match is the one thing in the assistant layer a person can lose, so the
//! properties checked here are the ones that decide a result: every termination
//! reason, the clock, resignation, and the fact that a stored match reloads into
//! exactly the position and result it had.
#![cfg(feature = "host")]

use rbc_ai::game::{Match, MatchConfig, MatchState, Player, TimeControl};
use rbc_db::Db;
use shakmaty::Color;

fn untimed() -> MatchConfig {
    MatchConfig::default()
}

fn timed(initial_ms: u64, increment_ms: u64) -> MatchConfig {
    MatchConfig {
        time_control: Some(TimeControl {
            initial_ms,
            increment_ms,
        }),
        ..MatchConfig::default()
    }
}

fn play(moves: &[&str]) -> Match {
    let mut game = Match::new(untimed()).expect("match");
    for mv in moves {
        game.apply(mv).expect("legal move");
    }
    game
}

// --- configuration and the move log ---------------------------------------

#[test]
fn match_config_round_trips_through_json_with_and_without_a_clock() {
    let untimed: MatchConfig =
        serde_json::from_str(&serde_json::to_string(&untimed()).unwrap()).unwrap();
    assert!(untimed.time_control.is_none());

    let with_clock: MatchConfig =
        serde_json::from_str(&serde_json::to_string(&timed(180_000, 2_000)).unwrap()).unwrap();
    assert_eq!(
        with_clock.time_control,
        Some(TimeControl {
            initial_ms: 180_000,
            increment_ms: 2_000
        })
    );

    // Player shapes survive the same trip.
    let black = serde_json::to_string(&Player::Model {
        provider: "openai".into(),
        model: "gpt".into(),
    })
    .unwrap();
    assert!(black.contains("\"kind\":\"model\""));
}

#[test]
fn match_records_every_ply_with_colour_and_position() {
    let game = play(&["e4", "e5", "Nf3"]);

    let moves = game.moves();
    assert_eq!(moves.len(), 3);
    assert_eq!(moves[0].ply, 1);
    assert_eq!(moves[0].color, "white");
    assert_eq!(moves[0].san, "e4");
    assert_eq!(moves[0].uci, "e2e4");
    assert_eq!(moves[1].color, "black");
    assert_eq!(moves[1].san, "e5");
    assert_eq!(moves[2].san, "Nf3");
    // Each ply records the position after it.
    assert!(moves[2].fen.starts_with("rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2"));
    assert_eq!(game.turn(), Color::Black);
}

#[test]
fn match_accepts_uci_as_well_as_san() {
    let game = play(&["e2e4", "e7e5"]);
    assert_eq!(game.moves()[0].san, "e4");
    assert_eq!(game.moves()[1].uci, "e7e5");
}

#[test]
fn match_refuses_an_illegal_or_empty_move_without_recording_it() {
    let mut game = Match::new(untimed()).unwrap();
    assert!(game.apply("e5").is_err());
    assert!(game.apply("").is_err());
    assert!(game.apply("Zz9").is_err());
    assert!(game.moves().is_empty());
}

#[test]
fn match_starts_from_a_custom_position_and_awaits_the_right_side() {
    let mut config = untimed();
    config.start_fen = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1".into();
    let game = Match::new(config).unwrap();
    assert_eq!(game.state(), &MatchState::AwaitingBlack);
    assert_eq!(game.legal_moves().len(), 20);
}

// --- terminations ----------------------------------------------------------

#[test]
fn match_ends_on_checkmate() {
    let game = play(&["e4", "e5", "Qh5", "Nc6", "Bc4", "Nf6", "Qxf7#"]);
    assert_eq!(game.state(), &MatchState::Finished);
    let result = game.result().expect("result");
    assert_eq!(result.score, "1-0");
    assert_eq!(result.termination, "checkmate");
    // A finished match takes no more moves.
    let mut game = game;
    assert!(game.apply("Kxf7").is_err());
}

#[test]
fn match_ends_on_stalemate() {
    // Black to move after Qg6 has no legal move and is not in check.
    let mut config = untimed();
    config.start_fen = "7k/5K2/4Q3/8/8/8/8/8 w - - 0 1".into();
    let mut game = Match::new(config).unwrap();
    game.apply("Qg6").expect("Qg6 is legal");

    assert_eq!(game.state(), &MatchState::Finished);
    let result = game.result().expect("result");
    assert_eq!(result.score, "1/2-1/2");
    assert_eq!(result.termination, "stalemate");
}

#[test]
fn match_ends_on_insufficient_material() {
    // King and bishop against king: capture the last pawn-equivalent.
    let mut config = untimed();
    config.start_fen = "7k/8/8/8/8/8/8/K6B w - - 0 1".into();
    let mut game = Match::new(config).unwrap();
    // The position is already insufficient; play a waiting move to have it noticed.
    game.apply("Bg2").expect("legal");
    assert_eq!(game.state(), &MatchState::Finished);
    let result = game.result().expect("result");
    assert_eq!(result.termination, "insufficient material");
    assert_eq!(result.score, "1/2-1/2");
}

#[test]
fn match_ends_at_the_ply_limit() {
    let mut config = untimed();
    config.max_plies = 2;
    let mut game = Match::new(config).unwrap();
    game.apply("e4").unwrap();
    game.apply("e5").unwrap();
    assert_eq!(game.state(), &MatchState::Finished);
    let result = game.result().expect("result");
    assert_eq!(result.termination, "max plies");
    assert_eq!(result.score, "1/2-1/2");
}

#[test]
fn match_resignation_gives_the_win_to_the_other_side() {
    let mut game = play(&["e4", "e5"]);
    game.resign(Color::White);
    assert_eq!(game.state(), &MatchState::Finished);
    let result = game.result().expect("result");
    assert_eq!(result.score, "0-1");
    assert_eq!(result.termination, "resignation");
}

// --- clocks ----------------------------------------------------------------

#[test]
fn match_clock_charges_the_side_to_move_and_adds_the_increment() {
    let mut game = Match::new(timed(60_000, 1_000)).unwrap();
    assert_eq!(game.clock().expect("clock").white_ms, 60_000);
    // The match starts now; the first tick charges nothing.
    assert!(game.tick(0).is_none());

    // White thinks for 4s and moves.
    game.apply_at("e4", 4_000).expect("e4");
    let clock = game.clock().expect("clock");
    assert_eq!(
        clock.white_ms,
        60_000 - 4_000 + 1_000,
        "4s spent, 1s increment"
    );
    assert_eq!(clock.black_ms, 60_000);

    // Black thinks for 2.5s.
    game.apply_at("e5", 6_500).expect("e5");
    let clock = game.clock().expect("clock");
    assert_eq!(clock.black_ms, 60_000 - 2_500 + 1_000);
    assert_eq!(clock.white_ms, 57_000, "white is not charged while waiting");
}

#[test]
fn match_clock_flags_a_side_that_runs_out_of_time() {
    let mut game = Match::new(timed(5_000, 0)).unwrap();
    assert!(game.tick(0).is_none(), "the clock starts here");
    game.apply_at("e4", 1_000).expect("in time");
    // Black never moves; a tick past the remaining time ends it.
    let result = game.tick(10_000).expect("flagged");
    assert_eq!(result.termination, "timeout");
    assert_eq!(result.score, "1-0", "black ran out, so white wins");
    assert_eq!(game.state(), &MatchState::Finished);
    assert_eq!(game.clock().expect("clock").black_ms, 0);
    assert_eq!(game.moves().len(), 1, "a flag records no move");
}

#[test]
fn match_clock_refuses_to_move_once_flagged() {
    let mut game = Match::new(timed(2_000, 0)).unwrap();
    assert!(game.tick(0).is_none(), "the clock starts here");
    let error = game.apply_at("e4", 2_500).expect_err("out of time");
    assert!(error.to_string().contains("clock"));
    assert_eq!(game.state(), &MatchState::Finished);
    assert!(
        game.tick(3_000).is_none(),
        "a flagged match has nothing to charge"
    );
}

#[test]
fn match_ticks_are_cumulative_not_additive() {
    let mut game = Match::new(timed(10_000, 0)).unwrap();
    assert!(game.tick(0).is_none(), "the clock starts here");
    assert!(game.tick(1_000).is_none());
    assert!(game.tick(2_000).is_none());
    assert_eq!(game.clock().expect("clock").white_ms, 8_000);
    // Charging the same instant twice does not double-charge.
    assert!(game.tick(2_000).is_none());
    assert_eq!(game.clock().expect("clock").white_ms, 8_000);
}

#[test]
fn an_untimed_match_ignores_timestamps() {
    let mut game = Match::new(untimed()).unwrap();
    game.apply_at("e4", 1_000_000).expect("e4");
    assert!(game.clock().is_none());
    assert!(game.tick(2_000_000).is_none());
}

// --- PGN and snapshots -----------------------------------------------------

#[test]
fn match_pgn_names_both_players_and_the_result() {
    let mut config = untimed();
    config.white = Player::Human {
        name: "Alex".into(),
    };
    config.black = Player::Model {
        provider: "mock".into(),
        model: "m1".into(),
    };
    let mut game = Match::new(config).unwrap();
    for mv in ["e4", "e5", "Qh5", "Nc6", "Bc4", "Nf6", "Qxf7#"] {
        game.apply(mv).unwrap();
    }
    let pgn = game.pgn();
    assert!(pgn.contains("[White \"Alex\"]"), "{pgn}");
    assert!(pgn.contains("[Black \"mock/m1\"]"));
    assert!(pgn.contains("[Result \"1-0\"]"));
    assert!(pgn.contains("[Termination \"checkmate\"]"));
    assert!(pgn.contains("1. e4 e5 2. Qh5 Nc6"));
    assert!(pgn.trim_end().ends_with("1-0"));
    assert!(
        !pgn.contains("[TimeControl"),
        "an untimed match has no clock header"
    );
}

#[test]
fn match_pgn_records_an_unfinished_match_as_open() {
    let game = play(&["e4", "e5"]);
    let pgn = game.pgn();
    assert!(pgn.contains("[Result \"*\"]"));
    assert!(pgn.contains("[Termination \"unterminated\"]"));
    assert!(pgn.trim_end().ends_with('*'));
}

#[test]
fn match_pgn_carries_the_time_control_when_timed() {
    let mut game = Match::new(timed(300_000, 2_000)).unwrap();
    game.apply("e4").unwrap();
    assert!(
        game.pgn().contains("[TimeControl \"300:2\"]"),
        "{}",
        game.pgn()
    );
}

#[test]
fn match_snapshot_replays_position_moves_result_and_clock() {
    let mut game = Match::new(timed(60_000, 0)).unwrap();
    game.apply_at("e4", 5_000).unwrap();
    game.apply_at("e5", 9_000).unwrap();

    let replay = Match::from_snapshot(game.snapshot()).expect("replay");
    assert_eq!(replay.fen(), game.fen());
    assert_eq!(replay.moves(), game.moves());
    assert_eq!(replay.state(), game.state());
    // A clock cannot be recomputed from moves, so it is restored.
    assert_eq!(replay.clock(), game.clock());
}

#[test]
fn match_snapshot_of_a_finished_match_keeps_its_result() {
    let game = play(&["e4", "e5", "Qh5", "Nc6", "Bc4", "Nf6", "Qxf7#"]);
    let replay = Match::from_snapshot(game.snapshot()).expect("replay");
    assert_eq!(replay.state(), &MatchState::Finished);
    let result = replay.result().expect("result");
    assert_eq!(result.termination, "checkmate");
    assert_eq!(result.score, "1-0");
}

// --- persistence -----------------------------------------------------------

#[test]
fn match_survives_the_repository_and_resumes() {
    let store = Db::open_memory().expect("database");
    let config = timed(180_000, 2_000);
    let row = store
        .create_ai_match(
            None,
            &serde_json::to_string(&config.white).unwrap(),
            &serde_json::to_string(&config.black).unwrap(),
            &serde_json::to_string(&config).unwrap(),
        )
        .expect("stored match");

    let mut game = Match::new(config).unwrap();
    game.apply_at("e4", 1_000).unwrap();
    game.apply_at("e5", 3_000).unwrap();
    store
        .save_ai_match(
            &row.id,
            &serde_json::to_string(&game.snapshot()).unwrap(),
            &game.pgn(),
            None,
            None,
            false,
        )
        .expect("saved");

    // Reload from the stored snapshot and continue the game.
    let stored = store.ai_match(&row.id).expect("read").expect("row");
    let snapshot: rbc_ai::game::MatchSnapshot =
        serde_json::from_str(&stored.state_json).expect("stored snapshot");
    let mut resumed = Match::from_snapshot(snapshot).expect("resume");
    assert_eq!(resumed.fen(), game.fen());
    assert_eq!(resumed.clock(), game.clock());
    resumed
        .apply_at("Nf3", 8_000)
        .expect("continue after reload");
    assert_eq!(resumed.moves().len(), 3);
}

#[test]
fn a_stored_match_keeps_its_termination() {
    let store = Db::open_memory().expect("database");
    let config = untimed();
    let row = store
        .create_ai_match(
            None,
            &serde_json::to_string(&config.white).unwrap(),
            &serde_json::to_string(&config.black).unwrap(),
            &serde_json::to_string(&config).unwrap(),
        )
        .expect("stored match");

    let mut game = Match::new(config).unwrap();
    for mv in ["e4", "e5", "Qh5", "Nc6", "Bc4", "Nf6", "Qxf7#"] {
        game.apply(mv).unwrap();
    }
    let result = game.result().expect("result");
    let stored = store
        .save_ai_match(
            &row.id,
            &serde_json::to_string(&game.snapshot()).unwrap(),
            &game.pgn(),
            Some(&serde_json::to_string(result).unwrap()),
            Some(&result.termination),
            true,
        )
        .expect("saved");
    // The row's convenience columns agree with the snapshot.
    assert!(stored.pgn.contains("[Result \"1-0\"]"));
    assert_eq!(stored.termination.as_deref(), Some("checkmate"));
    let snapshot: rbc_ai::game::MatchSnapshot = serde_json::from_str(&stored.state_json).unwrap();
    let resumed = Match::from_snapshot(snapshot).expect("resume");
    assert_eq!(resumed.result().expect("result").score, "1-0");
}

// --- PGN comments ----------------------------------------------------------

#[test]
fn match_pgn_writes_ply_comments() {
    let mut game = play(&["e4", "e5"]);
    assert!(game.annotate_ply(1, "the king's pawn"), "ply 1 exists");
    assert!(game.annotate_ply(2, "symmetry"));
    let pgn = game.pgn();
    assert!(
        pgn.contains("1. e4 {the king's pawn} e5 {symmetry}"),
        "{pgn}"
    );
}

#[test]
fn match_refuses_a_comment_on_a_ply_that_was_not_played() {
    let mut game = play(&["e4"]);
    assert!(!game.annotate_ply(9, "too far"));
    assert!(!game.pgn().contains("too far"));
}

#[test]
fn match_comment_cannot_break_out_of_its_pgn_braces() {
    let mut game = play(&["e4"]);
    game.annotate_ply(1, "closes } early");
    let pgn = game.pgn();
    assert_eq!(pgn.matches('{').count(), pgn.matches('}').count(), "{pgn}");
    assert!(pgn.contains("{closes ) early}"));
}

#[test]
fn match_comments_survive_a_snapshot() {
    let mut game = play(&["e4", "e5"]);
    game.annotate_ply(1, "kept");
    let replay = Match::from_snapshot(game.snapshot()).expect("replay");
    assert_eq!(replay.moves()[0].comment.as_deref(), Some("kept"));
    assert!(replay.pgn().contains("{kept}"));
}
