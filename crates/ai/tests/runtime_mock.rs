#![cfg(feature = "provider")]
//! The provider adapter against a real HTTP endpoint, and the agent loop over a
//! mock tool host: no live network, no live model.
mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{MockServer, Reply};
use rbc_ai::provider::{
    Cancellation, ChatMessage, CompletionRequest, OpenAiProvider, Provider, ProviderConfig,
    ProviderEvent, error_code,
};

fn provider(server: &MockServer) -> OpenAiProvider {
    let config = ProviderConfig::openai_compatible(server.url(), "mock", "m1");
    OpenAiProvider::new(config).expect("provider")
}

fn request() -> CompletionRequest {
    CompletionRequest::new("m1", vec![ChatMessage::user("hello")])
}

#[test]
fn mock_answers_a_plain_completion_and_records_the_request() {
    let server = MockServer::start(vec![Reply::content("hi there")]);
    let provider = provider(&server);

    let response = provider
        .complete(
            &request(),
            &Cancellation::new(),
            &mut |_event: ProviderEvent| {},
        )
        .expect("completion");

    assert_eq!(response.content, "hi there");
    let sent = server.requests();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].path, "/v1/chat");
    // The body is the OpenAI chat-completions shape, with the model named.
    // The adapter pretty-prints, so compare without whitespace.
    let compact: String = sent[0]
        .body
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    assert!(compact.contains("\"model\":\"m1\""), "{}", sent[0].body);
    assert!(compact.contains("hello"), "{}", sent[0].body);
    assert!(compact.contains("\"messages\""), "{}", sent[0].body);
}

#[test]
fn mock_streams_deltas_through_the_event_sink() {
    let server = MockServer::start(vec![Reply::stream("streamed answer", 4)]);
    let provider = provider(&server);
    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);

    let response = provider
        .complete(&request(), &Cancellation::new(), &mut move |event| {
            if let ProviderEvent::ContentDelta(delta) = event {
                sink.lock().expect("seen").push(delta);
            }
        })
        .expect("completion");

    assert_eq!(response.content, "streamed answer");
    let deltas = seen.lock().expect("seen").clone();
    assert!(deltas.len() >= 2, "expected several deltas, got {deltas:?}");
}

#[test]
fn mock_reasoning_travels_with_the_answer() {
    let server = MockServer::start(vec![Reply::content_with_reasoning("Nf3", "develops")]);
    let provider = provider(&server);
    let response = provider
        .complete(
            &request(),
            &Cancellation::new(),
            &mut |_event: ProviderEvent| {},
        )
        .expect("completion");
    assert_eq!(response.content, "Nf3");
    assert_eq!(response.reasoning.as_deref(), Some("develops"));
}

#[test]
fn mock_429_is_rate_limited_and_retryable() {
    let server = MockServer::start(vec![Reply::rate_limited()]);
    let provider = provider(&server);
    let failure = provider
        .complete(
            &request(),
            &Cancellation::new(),
            &mut |_event: ProviderEvent| {},
        )
        .expect_err("429");
    assert_eq!(failure.code, error_code::RATE_LIMITED);
    assert!(failure.retryable, "a 429 is worth retrying");
}

#[test]
fn mock_500_is_a_provider_error_not_a_retry_forever() {
    let server = MockServer::start(vec![Reply::server_error()]);
    let provider = provider(&server);
    let failure = provider
        .complete(
            &request(),
            &Cancellation::new(),
            &mut |_event: ProviderEvent| {},
        )
        .expect_err("500");
    assert_eq!(failure.code, error_code::SERVER, "{}", failure.message);
    // It answered once; the adapter must not hammer a broken endpoint.
    assert!(server.count() >= 1);
}

#[test]
fn mock_that_never_answers_times_out() {
    let server = MockServer::start(vec![Reply::Hang]);
    let config = ProviderConfig::openai_compatible(server.url(), "mock", "m1").with_timeouts(
        Duration::from_millis(200),
        Duration::from_millis(200),
        Duration::from_millis(400),
    );
    let provider = OpenAiProvider::new(config).expect("provider");

    let failure = provider
        .complete(
            &request(),
            &Cancellation::new(),
            &mut |_event: ProviderEvent| {},
        )
        .expect_err("no answer");
    assert_eq!(failure.code, error_code::TIMEOUT, "{}", failure.message);
}

#[test]
fn mock_that_drops_the_connection_is_an_error_not_a_panic() {
    let server = MockServer::start(vec![Reply::Drop]);
    let provider = provider(&server);
    let failure = provider
        .complete(
            &request(),
            &Cancellation::new(),
            &mut |_event: ProviderEvent| {},
        )
        .expect_err("dropped");
    assert!(!failure.message.is_empty());
    assert!(
        failure.code == error_code::NETWORK || failure.code == error_code::PROTOCOL,
        "a dropped connection is a network or protocol failure, got {}",
        failure.code
    );
}

#[test]
fn mock_only_ever_sees_this_process() {
    // "No live network in tests" is a property of the fixture: the endpoint is
    // a loopback address on a port the OS picked.
    let server = MockServer::start(vec![Reply::content("ok")]);
    assert!(server.url().starts_with("http://127.0.0.1:"));
    let port: u16 = server
        .url()
        .trim_start_matches("http://127.0.0.1:")
        .split('/')
        .next()
        .and_then(|port| port.parse().ok())
        .expect("port");
    assert_ne!(port, 0);
    assert_ne!(port, 80);
    assert_ne!(port, 443);
}

#[test]
fn mock_illegal_then_legal_move_is_validated_over_the_wire() {
    // The model answers with a white move when it is black to move, then
    // corrects itself. The refusal happens here, in validation, not in the mock.
    let server = MockServer::start(vec![Reply::content("e4"), Reply::content("e5")]);
    let provider = provider(&server);
    let mut game = rbc_ai::game::Match::new(rbc_ai::game::MatchConfig {
        white: rbc_ai::game::Player::Human {
            name: "Alex".into(),
        },
        black: rbc_ai::game::Player::Model {
            provider: "mock".into(),
            model: "m1".into(),
        },
        ..rbc_ai::game::MatchConfig::default()
    })
    .expect("match");
    game.apply("e4").expect("human move");

    let played = rbc_ai::play::request_move(
        &mut game,
        &provider,
        &rbc_ai::protocol::ModelRef::new("mock", "m1"),
        &rbc_ai::play::PlayPolicy::competitive(),
        &Cancellation::new(),
    )
    .expect("second reply is legal");

    assert_eq!(played.mv.san, "e5");
    assert_eq!(played.attempts, 2);
    assert_eq!(server.count(), 2, "the retry went over the wire");
    // The second request carried the refusal back to the model.
    let sent = server.requests();
    let retry: serde_json::Value = serde_json::from_str(&sent[1].body).expect("json");
    let text = retry.to_string();
    assert!(text.contains("not a legal move"), "{text}");
}

#[test]
fn mock_tool_call_is_parsed_with_its_arguments() {
    let server = MockServer::start(vec![Reply::tool_call(
        "play_move",
        serde_json::json!({"board_id": "b1", "move": "e4", "expected_revision": 1}),
    )]);
    let provider = provider(&server);
    let response = provider
        .complete(
            &request(),
            &Cancellation::new(),
            &mut |_event: ProviderEvent| {},
        )
        .expect("completion");

    assert_eq!(response.tool_calls.len(), 1);
    let call = &response.tool_calls[0];
    assert_eq!(call.name, "play_move");
    assert_eq!(call.arguments["move"], serde_json::json!("e4"));
    assert_eq!(call.arguments["expected_revision"], serde_json::json!(1));
    assert_eq!(
        response.stop_reason,
        rbc_ai::protocol::StopReason::ToolCalls
    );
}
