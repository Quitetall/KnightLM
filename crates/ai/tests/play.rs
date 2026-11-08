//! Playing a match: what each kind of game allows, and how a model's move is
//! obtained and validated.
//!
//! The properties that matter are the ones a person would notice: a casual game
//! can be talked about and taken back, a competitive one cannot be talked to at
//! all, and no model reply is ever recorded unless it is legal.
#![cfg(feature = "provider")]

use std::sync::Mutex;

use rbc_ai::game::{Match, MatchConfig, MatchState, Player};
use rbc_ai::play::{
    MatchKind, PlayAction, PlayPolicy, model_seat, move_prompt, request_chat, request_move,
};
use rbc_ai::protocol::{ErrorInfo, ModelRef, StopReason};
use rbc_ai::provider::{
    Cancellation, CompletionRequest, CompletionResponse, Provider, ProviderEvent, error_code,
};

// --- a scripted model ------------------------------------------------------

struct Scripted {
    replies: Mutex<Vec<Result<CompletionResponse, ErrorInfo>>>,
    seen: Mutex<Vec<CompletionRequest>>,
}

impl Scripted {
    fn new(replies: Vec<(&str, Option<&str>)>) -> Self {
        Self {
            replies: Mutex::new(
                replies
                    .into_iter()
                    .map(|(content, reasoning)| {
                        Ok(CompletionResponse {
                            content: content.to_string(),
                            reasoning: reasoning.map(str::to_string),
                            tool_calls: Vec::new(),
                            usage: Default::default(),
                            stop_reason: StopReason::EndTurn,
                            provider_request_id: None,
                        })
                    })
                    .collect(),
            ),
            seen: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<CompletionRequest> {
        self.seen.lock().expect("seen").clone()
    }
}

impl Provider for Scripted {
    fn name(&self) -> &str {
        "scripted"
    }

    fn complete(
        &self,
        request: &CompletionRequest,
        _cancel: &Cancellation,
        _on_event: &mut dyn FnMut(ProviderEvent),
    ) -> Result<CompletionResponse, ErrorInfo> {
        self.seen.lock().expect("seen").push(request.clone());
        let mut replies = self.replies.lock().expect("replies");
        if replies.is_empty() {
            return Err(ErrorInfo {
                code: "scripted_exhausted".into(),
                message: "no reply left".into(),
                retryable: false,
                provider_code: None,
            });
        }
        replies.remove(0)
    }
}

fn model_match() -> Match {
    Match::new(MatchConfig {
        white: Player::Human {
            name: "Alex".into(),
        },
        black: Player::Model {
            provider: "scripted".into(),
            model: "m1".into(),
        },
        ..MatchConfig::default()
    })
    .expect("match")
}

fn model() -> ModelRef {
    ModelRef::new("scripted", "m1")
}

// --- the policy ------------------------------------------------------------

#[test]
fn play_policy_competitive_is_moves_only() {
    let policy = PlayPolicy::competitive();
    assert_eq!(policy.kind, MatchKind::Competitive);
    assert!(!policy.allow_chat);
    assert!(!policy.allow_hints);
    assert!(!policy.allow_engine);
    assert!(!policy.allow_takeback);
    assert!(!policy.request_reasoning, "reasoning is not even asked for");
    assert_eq!(policy.keep_reasoning(Some("because".into())), None);

    for action in [
        PlayAction::Chat,
        PlayAction::Hint,
        PlayAction::Engine,
        PlayAction::Takeback,
    ] {
        let refusal = policy.check(action).expect("refused");
        assert_eq!(refusal.code, error_code::UNSUPPORTED);
        assert!(
            refusal.message.contains("competitive"),
            "{}",
            refusal.message
        );
    }
}

#[test]
fn play_policy_casual_allows_the_conversation() {
    let policy = PlayPolicy::casual();
    assert_eq!(policy.kind, MatchKind::Casual);
    for action in [
        PlayAction::Chat,
        PlayAction::Hint,
        PlayAction::Engine,
        PlayAction::Takeback,
    ] {
        assert!(
            policy.check(action).is_none(),
            "{action:?} should be allowed"
        );
    }
    assert_eq!(
        policy.keep_reasoning(Some("because".into())),
        Some("because".to_string())
    );
}

#[test]
fn play_policy_keeps_reasoning_if_it_is_shown_or_exported() {
    let mut policy = PlayPolicy::competitive();
    policy.export_reasoning = true;
    assert_eq!(
        policy.keep_reasoning(Some("kept for the report".into())),
        Some("kept for the report".to_string()),
        "an exported report may hold reasoning a live view hides"
    );
}

// --- asking for a move -----------------------------------------------------

#[test]
fn play_asks_for_a_move_and_records_it() {
    let mut game = model_match();
    // Black to move after a human move.
    game.apply("e4").expect("human move");
    let provider = Scripted::new(vec![("e5", Some("symmetric"))]);

    let played = request_move(
        &mut game,
        &provider,
        &model(),
        &PlayPolicy::casual(),
        &Cancellation::new(),
    )
    .expect("model move");

    assert_eq!(played.mv.san, "e5");
    assert_eq!(played.mv.color, "black");
    assert_eq!(played.attempts, 1);
    assert_eq!(game.moves().len(), 2);
    assert_eq!(played.reasoning.as_deref(), Some("symmetric"));

    // The model was told the position and the reply format.
    let prompt = &provider.requests()[0].messages[0]
        .content
        .clone()
        .expect("system prompt");
    assert!(
        prompt.contains("FEN: rnbqkbnr/pppppppp/8/8/4P3"),
        "{prompt}"
    );
    assert!(prompt.contains("Your side: black"));
    assert!(prompt.contains("Legal moves:"));
    assert!(prompt.contains("exactly one move"));
}

#[test]
fn play_retries_an_illegal_reply_with_the_reason() {
    let mut game = model_match();
    game.apply("e4").expect("human move");
    // Black answers with a white move first: illegal, so it must be retried.
    let provider = Scripted::new(vec![("e4", None), ("e5", None)]);

    let played = request_move(
        &mut game,
        &provider,
        &model(),
        &PlayPolicy::competitive(),
        &Cancellation::new(),
    )
    .expect("second reply is legal");

    assert_eq!(played.mv.san, "e5");
    assert_eq!(played.attempts, 2);

    // The retry carried the refusal back to the model.
    let retry = &provider.requests()[1];
    let last = retry.messages.last().expect("retry message");
    assert!(
        last.content
            .as_deref()
            .unwrap_or("")
            .contains("not a legal move")
    );
}

#[test]
fn play_gives_up_after_the_retry_budget_without_touching_the_board() {
    let mut config = MatchConfig {
        white: Player::Human {
            name: "Alex".into(),
        },
        black: Player::Model {
            provider: "scripted".into(),
            model: "m1".into(),
        },
        max_retries: 2,
        ..MatchConfig::default()
    };
    config.max_plies = 300;
    let mut game = Match::new(config).expect("match");
    game.apply("e4").expect("human move");
    let provider = Scripted::new(vec![("Qh9", None), ("Kf9", None)]);

    let failure = request_move(
        &mut game,
        &provider,
        &model(),
        &PlayPolicy::competitive(),
        &Cancellation::new(),
    )
    .expect_err("no legal move");

    assert_eq!(failure.code, error_code::PROTOCOL);
    assert!(failure.retryable);
    assert_eq!(game.moves().len(), 1, "the board is untouched");
    assert_eq!(provider.requests().len(), 2, "the budget is respected");
}

#[test]
fn play_hides_reasoning_in_a_competitive_game() {
    let mut game = model_match();
    game.apply("e4").expect("human move");
    let provider = Scripted::new(vec![("e5", Some("I like symmetry"))]);

    let played = request_move(
        &mut game,
        &provider,
        &model(),
        &PlayPolicy::competitive(),
        &Cancellation::new(),
    )
    .expect("model move");

    assert_eq!(played.reasoning, None, "competitive keeps no reasoning");
    // It is not even asked for.
    let prompt = &provider.requests()[0].messages[0]
        .content
        .clone()
        .expect("system prompt");
    assert!(!prompt.contains("Think briefly"), "{prompt}");
}

// --- talking during a game -------------------------------------------------

#[test]
fn play_chat_is_refused_outright_in_a_competitive_game() {
    let game = model_match();
    let provider = Scripted::new(vec![("hello", None)]);
    let failure = request_chat(
        &game,
        &provider,
        &model(),
        &PlayPolicy::competitive(),
        "what should I play?",
        &Cancellation::new(),
    )
    .expect_err("no chat in a competitive game");

    assert_eq!(failure.code, error_code::UNSUPPORTED);
    assert!(
        provider.requests().is_empty(),
        "the model is never asked, so the policy is not a prompt request"
    );
}

#[test]
fn play_chat_answers_in_a_casual_game() {
    let game = model_match();
    let provider = Scripted::new(vec![("Try Nf3, it develops.", None)]);
    let reply = request_chat(
        &game,
        &provider,
        &model(),
        &PlayPolicy::casual(),
        "what should I play?",
        &Cancellation::new(),
    )
    .expect("casual chat");

    assert!(reply.contains("Nf3"));
    let prompt = &provider.requests()[0].messages[0]
        .content
        .clone()
        .expect("system prompt");
    assert!(prompt.contains("casual"), "{prompt}");
}

// --- takebacks -------------------------------------------------------------

#[test]
fn play_takeback_restores_the_position() {
    let mut game = model_match();
    for mv in ["e4", "e5", "Nf3"] {
        game.apply(mv).expect("legal");
    }
    let before = game.fen();

    let taken = game.takeback().expect("taken back");
    assert_eq!(taken.san, "Nf3");
    assert_eq!(game.moves().len(), 2);
    assert_ne!(game.fen(), before);
    assert_eq!(
        game.turn(),
        shakmaty::Color::White,
        "white is to move again"
    );

    // Two more takebacks reach the start, and then there is nothing to take.
    game.takeback();
    game.takeback();
    assert!(game.moves().is_empty());
    assert_eq!(
        game.fen(),
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
    );
    assert!(game.takeback().is_none());
}

#[test]
fn play_takeback_is_refused_once_the_match_is_finished() {
    let mut game = model_match();
    for mv in ["e4", "e5", "Qh5", "Nc6", "Bc4", "Nf6"] {
        game.apply(mv).expect("legal");
    }
    game.resign(shakmaty::Color::Black);
    assert_eq!(game.state(), &MatchState::Finished);
    assert!(
        game.takeback().is_none(),
        "a finished match is not reopened"
    );
}

#[test]
fn play_finds_the_model_seat() {
    let game = model_match();
    let seat = model_seat(game.config()).expect("black is a model");
    assert_eq!(seat.qualified_id(), "scripted/m1");

    let human_only = Match::new(MatchConfig {
        white: Player::Human { name: "a".into() },
        black: Player::Human { name: "b".into() },
        ..MatchConfig::default()
    })
    .expect("match");
    assert!(model_seat(human_only.config()).is_none());
}

#[test]
fn play_prompt_is_explicit_about_the_side_and_format() {
    let game = model_match();
    let prompt = move_prompt(&game, &PlayPolicy::competitive());
    assert!(prompt.contains("Your side: white"));
    assert!(prompt.contains("Reply with exactly one move"));
    assert!(
        prompt.contains("Nf3") || prompt.contains("g1f3"),
        "examples are given"
    );
}

#[test]
fn knightlm_sampling_uses_the_same_legal_move_grammar_for_every_vote() {
    let model = ModelRef::new("local:LAMU", "knightlm-smolvlm-500m");
    for (replies, expected, winning_votes) in [
        (vec!["c7c5", "e7e5", "e7e5", "c7c6"], "e7e5", 2),
        (vec!["c7c5", "e7e5", "e7e5", "c7c5"], "c7c5", 2),
    ] {
        let mut game = model_match();
        game.apply("e4").expect("human move");
        let provider = Scripted::new(replies.into_iter().map(|mv| (mv, None)).collect());
        let (played, vote) = rbc_ai::play::request_sampled_knightlm_move(
            &mut game,
            &provider,
            &model,
            &Cancellation::new(),
            4,
            0.6,
        )
        .expect("one sampled move");
        assert_eq!(game.moves().len(), 2);
        assert_eq!(played.mv.uci, expected);
        assert_eq!(vote.direct_move, "c7c5");
        assert_eq!(vote.legal_samples, 4);
        assert_eq!(vote.winning_votes, winning_votes);
        let requests = provider.requests();
        assert_eq!(requests.len(), 4);
        assert_eq!(requests[0].temperature, 0.0);
        let grammar = requests[0].grammar.as_deref().expect("legal move grammar");
        assert!(grammar.contains("\"c7c5\""));
        assert!(grammar.contains("\"e7e5\""));
        assert!(!grammar.contains("\"e2e4\""));
        for request in &requests[1..] {
            assert_eq!(request.temperature, 0.6);
            assert_eq!(request.max_tokens, Some(20));
            assert_eq!(request.grammar.as_deref(), Some(grammar));
            assert_eq!(request.messages[0].content, requests[0].messages[0].content);
        }
    }
}

#[test]
fn knightlm_rejects_a_decoder_that_ignores_the_grammar() {
    let mut game = model_match();
    game.apply("e4").expect("human move");
    let model = ModelRef::new("local:LAMU", "knightlm-smolvlm-500m");
    let provider = Scripted::new(vec![("e2e4", None)]);
    let error = request_move(
        &mut game,
        &provider,
        &model,
        &PlayPolicy::casual(),
        &Cancellation::new(),
    )
    .expect_err("the decoder returned a move outside the grammar");
    assert!(error.message.contains("outside the legal move list"));
    assert_eq!(provider.requests().len(), 1);
    assert!(provider.requests()[0].grammar.is_some());
    assert_eq!(game.moves().len(), 1);
}
