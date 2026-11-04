//! Watching two models play: seats, pausing, and the annotated record.
#![cfg(feature = "provider")]

use std::sync::Mutex;

use rbc_ai::game::{Match, MatchConfig, MatchState, Player, WatchSettings};
use rbc_ai::play::{PlayPolicy, WatchSeats, model_seat, watch_seats, watch_step};
use rbc_ai::protocol::{ErrorInfo, ModelRef, StopReason};
use rbc_ai::provider::{
    Cancellation, CompletionRequest, CompletionResponse, Provider, ProviderEvent, error_code,
};

struct Scripted {
    replies: Mutex<Vec<(String, Option<String>)>>,
    seen: Mutex<usize>,
}

impl Scripted {
    fn new(replies: Vec<(&str, Option<&str>)>) -> Self {
        Self {
            replies: Mutex::new(
                replies
                    .into_iter()
                    .map(|(content, reasoning)| (content.to_string(), reasoning.map(str::to_string)))
                    .collect(),
            ),
            seen: Mutex::new(0),
        }
    }
}

impl Provider for Scripted {
    fn name(&self) -> &str {
        "scripted"
    }

    fn complete(
        &self,
        _request: &CompletionRequest,
        _cancel: &Cancellation,
        _on_event: &mut dyn FnMut(ProviderEvent),
    ) -> Result<CompletionResponse, ErrorInfo> {
        *self.seen.lock().expect("seen") += 1;
        let mut replies = self.replies.lock().expect("replies");
        if replies.is_empty() {
            return Err(ErrorInfo {
                code: "scripted_exhausted".into(),
                message: "no reply left".into(),
                retryable: false,
                provider_code: None,
            });
        }
        let (content, reasoning) = replies.remove(0);
        Ok(CompletionResponse {
            content,
            reasoning,
            tool_calls: Vec::new(),
            usage: Default::default(),
            stop_reason: StopReason::EndTurn,
            provider_request_id: None,
        })
    }
}

fn watched(settings: WatchSettings) -> Match {
    Match::new(MatchConfig {
        white: Player::Model {
            provider: "scripted".into(),
            model: "w".into(),
        },
        black: Player::Model {
            provider: "scripted".into(),
            model: "b".into(),
        },
        watch: settings,
        ..MatchConfig::default()
    })
    .expect("match")
}

fn seats() -> WatchSeats {
    watch_seats(&watched(WatchSettings::default()).config().clone()).expect("two models")
}

#[test]
fn watch_finds_both_seats() {
    let seats = seats();
    assert_eq!(seats.white.qualified_id(), "scripted/w");
    assert_eq!(seats.black.qualified_id(), "scripted/b");

    // A game with a person in it is not a watched game.
    let mixed = Match::new(MatchConfig {
        white: Player::Model {
            provider: "s".into(),
            model: "w".into(),
        },
        black: Player::Human {
            name: "Alex".into(),
        },
        ..MatchConfig::default()
    })
    .expect("match");
    assert!(watch_seats(mixed.config()).is_none());
    // `model_seat` still finds the model side of a mixed game.
    assert!(model_seat(mixed.config()).is_some());
}

#[test]
fn watch_plays_one_ply_for_each_side() {
    let mut game = watched(WatchSettings::default());
    let provider = Scripted::new(vec![("e4", None), ("e5", None)]);
    let seats = seats();

    let first = watch_step(&mut game, &provider, &seats, &Cancellation::new()).expect("white");
    assert_eq!(first.mv.san, "e4");
    assert_eq!(first.mv.color, "white");

    let second = watch_step(&mut game, &provider, &seats, &Cancellation::new()).expect("black");
    assert_eq!(second.mv.san, "e5");
    assert_eq!(second.mv.color, "black");
    assert_eq!(game.moves().len(), 2);
}

#[test]
fn watch_pauses_and_resumes() {
    let mut game = watched(WatchSettings::default());
    let provider = Scripted::new(vec![("e4", None)]);
    let seats = seats();

    assert!(game.pause());
    assert!(game.is_paused());
    let refusal = watch_step(&mut game, &provider, &seats, &Cancellation::new())
        .expect_err("no moves while paused");
    assert_eq!(refusal.code, error_code::UNSUPPORTED);
    assert!(game.moves().is_empty());

    assert!(game.resume());
    assert!(!game.is_paused());
    assert_eq!(game.state(), &MatchState::AwaitingWhite);
    watch_step(&mut game, &provider, &seats, &Cancellation::new()).expect("plays again");
    assert_eq!(game.moves().len(), 1);
}

#[test]
fn watch_pause_is_refused_twice_and_after_the_end() {
    let mut game = watched(WatchSettings::default());
    assert!(game.pause());
    assert!(!game.pause(), "already paused");
    assert!(game.resume());
    game.resign(shakmaty::Color::White);
    assert!(!game.pause(), "a finished game cannot be paused");
    assert!(!game.resume(), "and nothing resumes it");
}

#[test]
fn watch_reset_returns_to_the_start_and_drops_the_record() {
    let mut game = watched(WatchSettings::default());
    let provider = Scripted::new(vec![("e4", None), ("e5", None)]);
    let seats = seats();
    watch_step(&mut game, &provider, &seats, &Cancellation::new()).expect("white");
    watch_step(&mut game, &provider, &seats, &Cancellation::new()).expect("black");

    assert!(game.reset());
    assert!(game.moves().is_empty(), "a reset is a new game");
    assert_eq!(game.state(), &MatchState::AwaitingWhite);
    assert_eq!(
        game.fen(),
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
    );
}

#[test]
fn watch_annotates_only_the_side_whose_toggle_is_on() {
    let mut game = watched(WatchSettings {
        white_reasoning: true,
        black_reasoning: false,
    });
    let provider = Scripted::new(vec![
        ("e4", Some("claiming the centre")),
        ("e5", Some("the same, symmetrically")),
    ]);
    let seats = seats();

    let white = watch_step(&mut game, &provider, &seats, &Cancellation::new()).expect("white");
    assert!(white.annotated);
    assert_eq!(white.reasoning.as_deref(), Some("claiming the centre"));

    let black = watch_step(&mut game, &provider, &seats, &Cancellation::new()).expect("black");
    assert!(!black.annotated, "black's toggle is off");
    assert_eq!(black.reasoning, None);

    let pgn = game.pgn();
    assert!(pgn.contains("claiming the centre"), "{pgn}");
    assert!(!pgn.contains("symmetrically"), "{pgn}");
}

#[test]
fn watch_pgn_is_annotated_when_both_sides_ask() {
    let mut game = watched(WatchSettings::both());
    let provider = Scripted::new(vec![
        ("e4", Some("centre first")),
        ("e5", Some("mirror")),
    ]);
    let seats = seats();
    watch_step(&mut game, &provider, &seats, &Cancellation::new()).expect("white");
    watch_step(&mut game, &provider, &seats, &Cancellation::new()).expect("black");

    let pgn = game.pgn();
    assert!(pgn.contains("centre first"), "{pgn}");
    assert!(pgn.contains("mirror"), "{pgn}");
    // Each comment sits after its own move, so the moves are not adjacent.
    assert!(pgn.contains("1. e4 {centre first} e5 {mirror}"), "{pgn}");
}

#[test]
fn watch_policy_never_talks_and_keeps_reasoning_by_toggle() {
    // The watched-game policy is moves-only, whatever the reasoning toggles say.
    let policy = PlayPolicy {
        kind: rbc_ai::play::MatchKind::Competitive,
        request_reasoning: true,
        show_reasoning: true,
        export_reasoning: true,
        allow_chat: false,
        allow_hints: false,
        allow_engine: false,
        allow_takeback: false,
    };
    assert!(policy.check(rbc_ai::play::PlayAction::Chat).is_some());
    assert_eq!(
        policy.keep_reasoning(Some("kept".into())),
        Some("kept".to_string())
    );
}

#[test]
fn watch_settings_default_to_silence() {
    let settings = WatchSettings::default();
    assert!(!settings.for_side(shakmaty::Color::White));
    assert!(!settings.for_side(shakmaty::Color::Black));
    let both = WatchSettings::both();
    assert!(both.for_side(shakmaty::Color::White));
    assert!(both.for_side(shakmaty::Color::Black));
}

#[test]
fn watch_model_of_a_stale_ply_is_ignored() {
    // annotating a ply that does not exist must not invent a comment.
    let mut game = watched(WatchSettings::both());
    assert!(!game.annotate_ply(99, "nowhere"));
    assert!(!game.pgn().contains("nowhere"));
}
