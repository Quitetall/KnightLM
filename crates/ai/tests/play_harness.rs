//! The modes-and-tools harness (docs/model/modes-and-tools.md sections 2, 3, 5):
//! play-mode tool calls with a shared read budget, the coach `Provisional:` rule,
//! the constrained-decoding fallback, and an unchanged default path.
#![cfg(feature = "provider")]

use std::sync::Mutex;

use rbc_ai::game::{Match, MatchConfig, Player};
use rbc_ai::model_turn::{ChatLine, READ_BUDGET, format_play_completion};
use rbc_ai::play::{
    CoachContext, MoveHarness, PlayPolicy, request_move, request_move_with_harness,
};
use rbc_ai::protocol::{ErrorInfo, ModelRef, StopReason};
use rbc_ai::provider::{
    Cancellation, CompletionRequest, CompletionResponse, Provider, ProviderEvent, error_code,
};

enum Step {
    Say(String),
    /// Stream this text, then fail as a timeout would.
    CutOff(String),
    /// Reply that stopped because the token limit was reached.
    Truncated(String),
}

struct Scripted {
    steps: Mutex<Vec<Step>>,
    seen: Mutex<Vec<CompletionRequest>>,
}

impl Scripted {
    fn new(steps: Vec<Step>) -> Self {
        Self {
            steps: Mutex::new(steps),
            seen: Mutex::new(Vec::new()),
        }
    }
    fn says(texts: &[&str]) -> Self {
        Self::new(texts.iter().map(|t| Step::Say((*t).into())).collect())
    }
    fn requests(&self) -> Vec<CompletionRequest> {
        self.seen.lock().unwrap().clone()
    }
}

fn response(content: String, stop_reason: StopReason) -> CompletionResponse {
    CompletionResponse {
        content,
        reasoning: None,
        tool_calls: Vec::new(),
        usage: Default::default(),
        stop_reason,
        provider_request_id: None,
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
        on_event: &mut dyn FnMut(ProviderEvent),
    ) -> Result<CompletionResponse, ErrorInfo> {
        self.seen.lock().unwrap().push(request.clone());
        let mut steps = self.steps.lock().unwrap();
        assert!(
            !steps.is_empty(),
            "the harness asked for more replies than scripted"
        );
        match steps.remove(0) {
            Step::Say(text) => Ok(response(text, StopReason::EndTurn)),
            Step::Truncated(text) => Ok(response(text, StopReason::MaxTokens)),
            Step::CutOff(text) => {
                on_event(ProviderEvent::ContentDelta(text));
                Err(ErrorInfo {
                    code: error_code::TIMEOUT.into(),
                    message: "too slow".into(),
                    retryable: true,
                    provider_code: None,
                })
            }
        }
    }
}

fn game() -> Match {
    Match::new(MatchConfig {
        white: Player::Model {
            provider: "scripted".into(),
            model: "m1".into(),
        },
        black: Player::Human {
            name: "Alex".into(),
        },
        ..MatchConfig::default()
    })
    .unwrap()
}

fn model() -> ModelRef {
    ModelRef::new("scripted", "m1")
}

fn submit(reasoning: &str, mv: &str) -> String {
    format_play_completion(reasoning, mv)
}

fn call(name: &str, arguments: &str) -> String {
    format!("<tool_call>{{\"name\":\"{name}\",\"arguments\":{arguments}}}</tool_call>")
}

fn run(
    game: &mut Match,
    provider: &Scripted,
    harness: &MoveHarness,
    policy: &PlayPolicy,
) -> rbc_ai::play::HarnessMove {
    request_move_with_harness(
        game,
        provider,
        &model(),
        policy,
        harness,
        &Cancellation::new(),
    )
    .expect("a move")
}

fn text(request: &CompletionRequest, index: usize) -> String {
    request.messages[index].content.clone().unwrap_or_default()
}

fn last(request: &CompletionRequest) -> String {
    request
        .messages
        .last()
        .unwrap()
        .content
        .clone()
        .unwrap_or_default()
}

fn coach() -> MoveHarness {
    MoveHarness::Coach(CoachContext::default())
}

// A reply the constrained-decoding fallback accepts: the exact legal UCI move.
const FALLBACK_MOVE: &str = "d2d4";

#[test]
fn the_default_harness_is_the_existing_request_move() {
    let replies = ["e4"];
    let mut a = game();
    let direct = request_move(
        &mut a,
        &Scripted::says(&replies),
        &model(),
        &PlayPolicy::competitive(),
        &Cancellation::new(),
    )
    .unwrap();
    let mut b = game();
    let provider = Scripted::says(&replies);
    let via = run(
        &mut b,
        &provider,
        &MoveHarness::Default,
        &PlayPolicy::competitive(),
    );
    assert_eq!(via.played, direct);
    assert!(!via.fallback && via.read_calls == 0 && via.reply.is_none());
    assert_eq!(a.moves(), b.moves());
    // And it still sent the old prompt, with no tool schema in it.
    let sent = &text(&provider.requests()[0], 0);
    assert!(!sent.contains("submit_move"));
}

#[test]
fn play_reads_then_submits_and_the_prompt_never_shows_coach_tools() {
    let provider = Scripted::says(&[
        &call("attackers_of", r#"{"square":"e2"}"#),
        &call("list_legal_moves", "{}"),
        &submit("Open the center.", "e2e4"),
    ]);
    let mut g = game();
    let out = run(&mut g, &provider, &MoveHarness::Play, &PlayPolicy::casual());
    assert_eq!(out.played.mv.uci, "e2e4");
    assert_eq!(out.played.reasoning.as_deref(), Some("Open the center."));
    assert_eq!((out.read_calls, out.rejected, out.fallback), (2, 0, false));
    assert_eq!(out.played.attempts, 1, "reads are not move attempts");
    let first = &text(&provider.requests()[0], 0);
    assert!(first.contains("submit_move") && !first.contains("scratch_set"));
    // The tool result was fed back to the model.
    let second = last(&provider.requests()[1]);
    assert!(second.starts_with("<tool_result>"), "{second}");
    // A competitive policy keeps no reasoning.
    let provider = Scripted::says(&[&submit("secret", "e2e4")]);
    let out = run(
        &mut game(),
        &provider,
        &MoveHarness::Play,
        &PlayPolicy::competitive(),
    );
    assert_eq!(out.played.reasoning, None);
}

#[test]
fn the_ninth_read_is_refused_by_the_shared_budget() {
    let mut steps: Vec<String> = (0..READ_BUDGET + 1)
        .map(|_| call("list_legal_moves", "{}"))
        .collect();
    steps.push(submit("ok", "e2e4"));
    let refs: Vec<&str> = steps.iter().map(String::as_str).collect();
    let provider = Scripted::says(&refs);
    let out = run(
        &mut game(),
        &provider,
        &MoveHarness::Play,
        &PlayPolicy::casual(),
    );
    assert_eq!(out.read_calls, READ_BUDGET);
    let ninth_result = last(&provider.requests()[READ_BUDGET + 1]);
    assert!(ninth_result.contains("budget exhausted"), "{ninth_result}");
    assert_eq!(out.played.mv.uci, "e2e4");
}

#[test]
fn bad_attempts_are_bounded_by_max_retries_then_fall_back() {
    // max_retries is 3: a coach tool, malformed text and an illegal move.
    let provider = Scripted::says(&[
        &call("scratch_set", r#"{"fen":"x"}"#),
        "I will play e4",
        &submit("hm", "e2e5"),
        FALLBACK_MOVE,
    ]);
    let mut g = game();
    let out = run(&mut g, &provider, &MoveHarness::Play, &PlayPolicy::casual());
    assert!(out.fallback, "attempts spent: constrained decoding chose");
    assert_eq!(out.played.mv.uci, FALLBACK_MOVE);
    assert_eq!(out.rejected, 3);
    let reqs = provider.requests();
    assert_eq!(reqs.len(), 4);
    assert!(
        reqs[3].grammar.is_some(),
        "the last request is the grammar-constrained one"
    );
    // The refusal of the other mode's tool names the tool and the mode.
    let refusal = last(&reqs[1]);
    assert!(
        refusal.contains("scratch_set") && refusal.contains("play mode"),
        "{refusal}"
    );
}

#[test]
fn an_illegal_submit_can_be_corrected_within_the_budget() {
    let provider = Scripted::says(&[&submit("oops", "e2e5"), &submit("fixed", "g1f3")]);
    let out = run(
        &mut game(),
        &provider,
        &MoveHarness::Play,
        &PlayPolicy::casual(),
    );
    assert_eq!(
        (out.played.mv.uci.as_str(), out.rejected, out.fallback),
        ("g1f3", 1, false)
    );
    assert_eq!(out.played.attempts, 2);
    assert_eq!(out.played.mv.attempts, 2, "recorded on the match too");
}

#[test]
fn coach_last_valid_provisional_wins_and_the_reply_is_clean() {
    let provider = Scripted::says(&[
        "Provisional: e2e4\nHmm, the knight is better.\nProvisional: g1f3\nDevelop first.",
    ]);
    let ctx = CoachContext {
        summary: Some("We talked openings.".into()),
        recent: vec![ChatLine::new("user", "What now?")],
    };
    let out = run(
        &mut game(),
        &provider,
        &MoveHarness::Coach(ctx),
        &PlayPolicy::casual(),
    );
    assert_eq!(out.played.mv.uci, "g1f3");
    assert_eq!(out.revisions, 1);
    assert!(!out.fallback);
    assert_eq!(
        out.reply.as_deref(),
        Some("Hmm, the knight is better.\nDevelop first.")
    );
    let prompt = &text(&provider.requests()[0], 0);
    assert!(
        prompt.contains("Conversation summary: We talked openings.")
            && prompt.contains("user: What now?")
    );
    assert!(
        !prompt.contains("submit_move"),
        "coach never sees a play tool"
    );
}

#[test]
fn coach_illegal_later_provisional_keeps_the_legal_earlier_one() {
    let provider = Scripted::says(&["Provisional: e2e4\nProvisional: e2e9\nWell."]);
    let out = run(&mut game(), &provider, &coach(), &PlayPolicy::casual());
    assert_eq!(out.played.mv.uci, "e2e4");
    assert_eq!(out.rejected, 1);
}

#[test]
fn coach_with_no_valid_provisional_retries_then_falls_back_without_a_reply() {
    // Round 1: nothing usable. Round 2: only illegal. Round 3: exhausted.
    let provider = Scripted::says(&[
        "Move: e2e4\nLet me think.",
        "Provisional: e2e5\n",
        "Provisional: a1a1\nProvisional: b1b1\n",
        FALLBACK_MOVE,
    ]);
    let out = run(&mut game(), &provider, &coach(), &PlayPolicy::casual());
    assert!(out.fallback && out.reply.is_none());
    assert_eq!(out.played.mv.uci, FALLBACK_MOVE);
    let reqs = provider.requests();
    assert!(last(&reqs[1]).contains("Provisional: <legal UCI move>"));
    assert!(last(&reqs[2]).contains("not a legal move"));
}

#[test]
fn coach_a_retry_round_can_commit() {
    let provider = Scripted::says(&["Provisional: e2e5\nhm", "Provisional: d2d4\nok"]);
    let out = run(&mut game(), &provider, &coach(), &PlayPolicy::casual());
    assert_eq!(
        (out.played.mv.uci.as_str(), out.rejected, out.fallback),
        ("d2d4", 1, false)
    );
    assert_eq!(out.played.attempts, 2);
}

#[test]
fn coach_commits_what_arrived_when_time_or_tokens_run_out() {
    // Timed out mid-way through a second provisional line: the first stands.
    let provider = Scripted::new(vec![Step::CutOff(
        "Provisional: e2e4\nThinking... Provisional: g1".into(),
    )]);
    let out = run(&mut game(), &provider, &coach(), &PlayPolicy::casual());
    assert_eq!((out.played.mv.uci.as_str(), out.fallback), ("e2e4", false));
    // Out of tokens after a complete second line: the revision stands.
    let provider = Scripted::new(vec![Step::Truncated(
        "Provisional: e2e4\nProvisional: d2d4\nbecause".into(),
    )]);
    let out = run(&mut game(), &provider, &coach(), &PlayPolicy::casual());
    assert_eq!(out.played.mv.uci, "d2d4");
    // Out of time before anything legal: constrained fallback.
    let provider = Scripted::new(vec![
        Step::CutOff("Let me think".into()),
        Step::Say(FALLBACK_MOVE.into()),
    ]);
    let out = run(&mut game(), &provider, &coach(), &PlayPolicy::casual());
    assert!(out.fallback);
}

#[test]
fn a_coach_tool_can_never_submit_a_real_move() {
    // In coach mode the only way to a move is a Provisional line; a play call is
    // not in the schema and is just text with no provisional line.
    let provider = Scripted::says(&[&submit("sneaky", "e2e4"), "no", "no", FALLBACK_MOVE]);
    let mut g = game();
    let out = run(&mut g, &provider, &coach(), &PlayPolicy::casual());
    assert!(
        out.fallback,
        "no provisional line, so only the fallback moved"
    );
}

#[test]
fn oversized_chat_context_fails_before_any_model_call() {
    let provider = Scripted::says(&[]);
    let ctx = CoachContext {
        summary: Some("x".repeat(1201)),
        recent: vec![],
    };
    let err = request_move_with_harness(
        &mut game(),
        &provider,
        &model(),
        &PlayPolicy::casual(),
        &MoveHarness::Coach(ctx),
        &Cancellation::new(),
    )
    .unwrap_err();
    assert!(err.message.contains("MAX_SUMMARY_CHARS"));
    assert!(provider.requests().is_empty());
}
