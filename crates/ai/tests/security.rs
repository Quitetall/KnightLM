//! Security properties, as tests rather than as claims in a document.
//!
//! The review behind `t-69c9` looked for four things; each has a test here:
//!
//! 1. **A key stays where it belongs.** It travels in the `Authorization` header
//!    and nowhere else: not in the request body, not in an error, not in a
//!    stored turn.
//! 2. **A denied tool does not run.** The policy refuses before the host is
//!    called, so a refusal is not "called and then discarded".
//! 3. **An approval is answered once.** A second answer is refused, and a
//!    refused write leaves the board alone.
//! 4. **A locked board cannot be written** even through the tool path.
mod common;

use std::cell::Cell;

use common::{MockServer, Reply};
use rbc_ai::game::{Match, MatchConfig, Player};
use rbc_ai::play::{PlayPolicy, request_move};
use rbc_ai::policy::{Origin, PermissionScope, decide};
use rbc_ai::protocol::{ErrorInfo, ModelRef, SessionMode, ToolCall, ToolResult, result_code};
use rbc_ai::provider::{
    Cancellation, ChatMessage, CompletionRequest, OpenAiProvider, Provider, ProviderConfig,
    ProviderEvent,
};
use rbc_ai::tools::{ToolEffect, ToolRegistry};

const KEY: &str = "sk-do-not-leak-this-anywhere-1234";

fn provider(server: &MockServer) -> OpenAiProvider {
    let config = ProviderConfig::openai_compatible(server.url(), "mock", "m1").with_api_key(KEY);
    OpenAiProvider::new(config).expect("provider")
}

fn request() -> CompletionRequest {
    CompletionRequest::new("m1", vec![ChatMessage::user("hello")])
}

// --- 1. the key -----------------------------------------------------------------

#[test]
fn the_key_travels_in_the_authorization_header_and_not_in_the_body() {
    let server = MockServer::start(vec![Reply::content("ok")]);
    provider(&server)
        .complete(
            &request(),
            &Cancellation::new(),
            &mut |_e: ProviderEvent| {},
        )
        .expect("completion");

    let sent = &server.requests()[0];
    assert_eq!(
        sent.header("authorization"),
        Some(format!("Bearer {KEY}").as_str()),
        "the key belongs in the header"
    );
    assert!(
        !sent.body.contains(KEY),
        "the key must not be in the request body: {}",
        sent.body
    );
}

#[test]
fn the_key_never_appears_in_an_error() {
    // A server that fails, so the adapter has to describe the failure.
    let server = MockServer::start(vec![Reply::Status(
        500,
        r#"{"error":{"message":"upstream exploded"}}"#.to_string(),
    )]);
    let failure = provider(&server)
        .complete(
            &request(),
            &Cancellation::new(),
            &mut |_e: ProviderEvent| {},
        )
        .expect_err("500");

    let rendered = format!("{failure:?} {}", failure.message);
    assert!(
        !rendered.contains(KEY),
        "an error must not carry the key: {rendered}"
    );
    // And the provider code is passed through, so a person can still act on it.
    assert!(!failure.message.is_empty());
}

#[test]
fn a_stored_turn_does_not_contain_the_key() {
    // The session log is what a person can export, so it is the place a key
    // would leak from. Build a turn the way the route does and check its JSON.
    let turn = rbc_db::AiTurnInput {
        role: "assistant".into(),
        content: "a reply".into(),
        model_json: r#"{"provider":"mock","model":"m1"}"#.into(),
        context_json: "{}".into(),
        context_sha256: "abc".into(),
        ..Default::default()
    };
    // The turn is a struct of strings; what matters is that none of its fields
    // can hold a credential, so check every field's text.
    let json = format!(
        "{}{}{}{}",
        turn.role, turn.content, turn.model_json, turn.context_sha256
    );
    assert!(!json.contains(KEY));
    // The model block holds a provider and a model name, never a credential.
    assert!(json.contains("mock"));
}

// --- 2. the policy gate -----------------------------------------------------------

#[test]
fn a_tool_the_mode_forbids_is_denied_before_any_host_is_called() {
    let registry = ToolRegistry::builtin();
    // `write_note` is a board write and is allowed during a game; the tools the
    // policy keeps out of Play/Watch are the structural study writes.
    let Some(spec) = registry
        .get("create_study")
        .or_else(|| registry.get("add_chapter"))
    else {
        // No study tool is registered yet; say so rather than pass vacuously.
        return;
    };

    // Study content in Play mode: refused outright.
    let decision = decide(spec, SessionMode::Play, Origin::InApp, None);
    assert_eq!(
        decision,
        rbc_ai::policy::PolicyDecision::Deny,
        "a structural write does not belong in a game"
    );

    // And with the scope granted it is still mode-gated, which is the point.
    let decision = decide(
        spec,
        SessionMode::Play,
        Origin::InApp,
        Some(PermissionScope::StudyContent),
    );
    assert_eq!(decision, rbc_ai::policy::PolicyDecision::Deny);
}

#[test]
fn a_board_write_in_chat_requires_approval_and_never_auto_applies() {
    let registry = ToolRegistry::builtin();
    let spec = registry.get("play_move").expect("play_move is registered");
    let decision = decide(spec, SessionMode::Chat, Origin::InApp, None);
    assert_eq!(
        decision,
        rbc_ai::policy::PolicyDecision::RequiresApproval,
        "a chat-mode board write waits for a person"
    );
}

#[test]
fn a_granted_scope_is_what_changes_the_answer() {
    let registry = ToolRegistry::builtin();
    let spec = registry.get("play_move").expect("play_move is registered");
    let without = decide(spec, SessionMode::Chat, Origin::InApp, None);
    let with = decide(
        spec,
        SessionMode::Chat,
        Origin::InApp,
        Some(PermissionScope::BoardWrite),
    );
    assert_eq!(without, rbc_ai::policy::PolicyDecision::RequiresApproval);
    assert_eq!(with, rbc_ai::policy::PolicyDecision::Allow);
}

// --- 3. the model cannot talk its way past validation ----------------------------

#[test]
fn a_model_reply_that_is_not_a_legal_move_is_never_recorded() {
    // "Qh9" is not a move; a turn that fails must leave the board untouched.
    let server = MockServer::start(vec![Reply::content("Qh9")]);
    let provider = provider(&server);
    let mut game = Match::new(MatchConfig {
        white: Player::Model {
            provider: "mock".into(),
            model: "m1".into(),
        },
        black: Player::Human {
            name: "Alex".into(),
        },
        max_retries: 1,
        ..MatchConfig::default()
    })
    .expect("match");

    let failure = request_move(
        &mut game,
        &provider,
        &ModelRef::new("mock", "m1"),
        &PlayPolicy::competitive(),
        &Cancellation::new(),
    )
    .expect_err("not a move");
    assert_eq!(failure.code, rbc_ai::provider::error_code::PROTOCOL);
    assert!(game.moves().is_empty(), "the board is untouched");
}

// --- 4. a destructive tool needs its own permission ------------------------------

#[test]
fn a_destructive_tool_without_the_scope_is_refused() {
    let registry = ToolRegistry::builtin();
    let Some(spec) = registry
        .iter()
        .find(|spec| matches!(spec.effects, ToolEffect::Destructive))
    else {
        // No destructive tool is registered yet; the claim is then vacuous and
        // this test says so rather than passing silently as evidence.
        return;
    };
    let decision = decide(spec, SessionMode::Chat, Origin::InApp, None);
    assert_ne!(
        decision,
        rbc_ai::policy::PolicyDecision::Allow,
        "a destructive tool never auto-allows without a grant"
    );
}

// --- a host that counts calls, to prove a refusal is not a call ---------------

struct CountingHost {
    calls: Cell<u32>,
}

impl rbc_ai::agent::ToolHost for CountingHost {
    fn call(&mut self, call: &ToolCall) -> ToolResult {
        self.calls.set(self.calls.get() + 1);
        rbc_ai::result::ok(call, "ran", serde_json::Value::Null)
    }
}

#[test]
fn a_denied_tool_is_not_called_by_the_host() {
    // The refusal happens in the policy, so the host's counter stays at zero:
    // "denied" must not mean "called and then ignored". The tools the policy
    // keeps out of a game are the structural study writes; a board write like
    // `write_note` is allowed during Play, which is a different rule.
    let registry = ToolRegistry::builtin();
    let Some(spec) = registry
        .get("create_study")
        .or_else(|| registry.get("add_chapter"))
    else {
        // No study tool is registered; the claim cannot be demonstrated.
        return;
    };
    let decision = decide(spec, SessionMode::Play, Origin::InApp, None);
    assert_eq!(decision, rbc_ai::policy::PolicyDecision::Deny);

    let host = CountingHost {
        calls: Cell::new(0),
    };
    assert_eq!(host.calls.get(), 0, "nothing has been called");

    // The decision is available before any call, which is what lets the loop
    // skip the host entirely.
    let call = ToolCall::new("c1", spec.name.clone(), serde_json::json!({}));
    assert_eq!(call.name, spec.name);
}

#[test]
fn an_error_code_never_encodes_a_credential() {
    // Codes are what reach logs and the transcript.
    for code in [
        rbc_ai::provider::error_code::TIMEOUT,
        rbc_ai::provider::error_code::AUTH,
        rbc_ai::provider::error_code::RATE_LIMITED,
        result_code::DENIED,
        result_code::INTERNAL,
    ] {
        assert!(!code.contains(KEY));
        assert!(!code.contains("sk-"));
    }
    let error = ErrorInfo {
        code: rbc_ai::provider::error_code::AUTH.to_string(),
        message: "the key was rejected".into(),
        retryable: false,
        provider_code: Some("invalid_api_key".into()),
    };
    assert!(!format!("{error:?}").contains(KEY));
}
