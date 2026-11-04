//! Conformance tests against a real HTTP server.
//!
//! These drive [`OpenAiProvider`] over a loopback socket rather than a fake
//! transport, so chunked transfer encoding, streaming reads, headers and
//! status handling are all exercised as they are in production.
#![cfg(feature = "provider")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use rbc_ai::protocol::{StopReason, Usage};
use rbc_ai::provider::{
    Cancellation, ChatMessage, CompletionRequest, OpenAiProvider, Provider, ProviderConfig,
    ProviderEvent, RetryPolicy, ToolDefinition, error_code,
};

// --- a loopback server -----------------------------------------------------

/// One canned response, served to the next connection.
enum Reply {
    /// Let the client wait for a status, then answer with no stream.
    Status {
        status: u16,
        body: String,
    },
    /// A complete SSE stream: each frame is written as its own chunk.
    Sse(Vec<String>),
    /// An SSE stream whose connection dies before the terminator.
    BrokenSse(Vec<String>),
    /// One JSON document with no streaming.
    Json(String),
}

/// A server that answers the queued [`Reply`] values in order.
struct Mock {
    url: String,
    /// Raw request bodies, in the order they arrived.
    bodies: Arc<Mutex<Vec<String>>>,
    /// Raw request headers, in the order they arrived.
    headers: Arc<Mutex<Vec<String>>>,
}

impl Mock {
    fn start(replies: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let port = listener.local_addr().expect("local addr").port();
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let headers = Arc::new(Mutex::new(Vec::new()));

        let bodies_for_thread = Arc::clone(&bodies);
        let headers_for_thread = Arc::clone(&headers);
        thread::spawn(move || {
            for reply in replies {
                let Ok((mut stream, _peer)) = listener.accept() else {
                    return;
                };
                let (head, body) = read_request(&mut stream);
                headers_for_thread
                    .lock()
                    .expect("headers lock")
                    .push(head);
                bodies_for_thread.lock().expect("bodies lock").push(body);
                let _ = serve(&mut stream, reply);
            }
        });

        Self {
            url: format!("http://127.0.0.1:{port}/v1/chat/completions"),
            bodies,
            headers,
        }
    }

    fn request_bodies(&self) -> Vec<String> {
        self.bodies.lock().expect("bodies lock").clone()
    }

    fn requests(&self) -> usize {
        self.bodies.lock().expect("bodies lock").len()
    }

    fn request_headers(&self) -> Vec<String> {
        self.headers.lock().expect("headers lock").clone()
    }
}

fn read_request(stream: &mut TcpStream) -> (String, String) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
    let mut head = String::new();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed
            .to_ascii_lowercase()
            .strip_prefix("content-length:")
            .map(|value| value.trim().to_string())
        {
            content_length = value.parse().unwrap_or(0);
        }
        head.push_str(&line);
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body).expect("request body");
    }
    (head, String::from_utf8_lossy(&body).into_owned())
}

fn serve(stream: &mut TcpStream, reply: Reply) -> std::io::Result<()> {
    match reply {
        Reply::Status { status, body } => {
            write!(
                stream,
                "HTTP/1.1 {status} Error\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )?;
            stream.flush()
        }
        Reply::Json(body) => {
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )?;
            stream.flush()
        }
        Reply::Sse(frames) => {
            stream.write_all(
                b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n",
            )?;
            for frame in frames {
                write_chunk(stream, &frame)?;
                // A real model trickles; this makes sure nothing in the client
                // depends on the whole body arriving at once.
                thread::sleep(Duration::from_millis(2));
            }
            stream.write_all(b"0\r\n\r\n")?;
            stream.flush()
        }
        Reply::BrokenSse(frames) => {
            stream.write_all(
                b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n",
            )?;
            for frame in frames {
                write_chunk(stream, &frame)?;
            }
            // No terminator: the connection drops mid-stream.
            stream.flush()
        }
    }
}

fn write_chunk(stream: &mut TcpStream, text: &str) -> std::io::Result<()> {
    write!(stream, "{:x}\r\n", text.len())?;
    stream.write_all(text.as_bytes())?;
    stream.write_all(b"\r\n")?;
    stream.flush()
}

fn sse(value: &str) -> String {
    format!("data: {value}\n\n")
}

/// A chunk carrying visible text.
fn delta(text: &str) -> String {
    sse(&format!(
        r#"{{"id":"chatcmpl-1","choices":[{{"index":0,"delta":{{"content":{}}}}}]}}"#,
        json_str(text)
    ))
}

fn json_str(text: &str) -> String {
    serde_json::to_string(text).expect("encode string")
}

const DONE: &str = "data: [DONE]\n\n";

// --- provider fixture ------------------------------------------------------

fn provider(mock: &Mock, retry: RetryPolicy) -> OpenAiProvider {
    let config = ProviderConfig::openai_compatible(&mock.url, "mock", "knightlm-test")
        .with_api_key("sk-test-secret")
        .with_retry(retry)
        .with_temperature(0.1);
    OpenAiProvider::new(config).expect("provider configuration")
}

fn short_retry() -> RetryPolicy {
    RetryPolicy {
        max_attempts: 3,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(5),
    }
}

fn ask(tools: Vec<ToolDefinition>) -> CompletionRequest {
    CompletionRequest::new("knightlm-test", vec![ChatMessage::user("analyze this")])
        .with_tools(tools)
}

/// Run a request, collecting both the deltas and the assembled answer.
fn run(
    provider: &OpenAiProvider,
    request: &CompletionRequest,
    cancel: &Cancellation,
) -> (Result<rbc_ai::provider::CompletionResponse, rbc_ai::protocol::ErrorInfo>, Vec<ProviderEvent>) {
    let mut events = Vec::new();
    let result = provider.complete(request, cancel, &mut |event| events.push(event));
    (result, events)
}

// --- streaming -------------------------------------------------------------

#[test]
fn provider_streams_content_in_order_and_reports_usage() {
    let mock = Mock::start(vec![Reply::Sse(vec![
        delta("The position "),
        delta("is equal."),
        sse(r#"{"id":"chatcmpl-1","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#),
        sse(r#"{"id":"chatcmpl-1","choices":[],"usage":{"prompt_tokens":24,"completion_tokens":7}}"#),
        DONE.to_string(),
    ])]);
    let provider = provider(&mock, RetryPolicy::none());
    let (result, events) = run(&provider, &ask(Vec::new()), &Cancellation::new());

    let response = result.expect("streamed response");
    assert_eq!(response.content, "The position is equal.");
    assert_eq!(response.stop_reason, StopReason::EndTurn);
    assert_eq!(response.usage.tokens_in, 24);
    assert_eq!(response.usage.tokens_out, 7);
    assert_eq!(response.provider_request_id.as_deref(), Some("chatcmpl-1"));

    let texts: Vec<&str> = events
        .iter()
        .filter_map(|event| match event {
            ProviderEvent::ContentDelta(text) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, vec!["The position ", "is equal."]);
    assert!(matches!(
        events.last(),
        Some(ProviderEvent::Usage(Usage { tokens_in: 24, .. }))
    ));
    assert_eq!(mock.requests(), 1);
}

#[test]
fn provider_keeps_reasoning_deltas_out_of_the_answer() {
    let mock = Mock::start(vec![Reply::Sse(vec![
        sse(r#"{"choices":[{"index":0,"delta":{"reasoning_content":"because e4 "}}]}"#),
        sse(r#"{"choices":[{"index":0,"delta":{"reasoning_content":"controls d5"}}]}"#),
        delta("Play e4."),
        DONE.to_string(),
    ])]);
    let provider = provider(&mock, RetryPolicy::none());
    let (result, events) = run(&provider, &ask(Vec::new()), &Cancellation::new());

    let response = result.expect("streamed response");
    assert_eq!(response.content, "Play e4.");
    assert_eq!(response.reasoning.as_deref(), Some("because e4 controls d5"));
    assert!(events
        .iter()
        .any(|event| matches!(event, ProviderEvent::ReasoningDelta(_))));
}

#[test]
fn provider_assembles_tool_calls_split_across_chunks() {
    let mock = Mock::start(vec![Reply::Sse(vec![
        sse(r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_a","function":{"name":"get_board","arguments":"{\"board_"}}]}}]}"#),
        sse(r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"id\":\"b1\"}"}}]}}]}"#),
        sse(r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":1,"id":"call_b","function":{"name":"list_boards","arguments":"{}"}}]}}]}"#),
        sse(r#"{"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}"#),
        DONE.to_string(),
    ])]);
    let provider = provider(&mock, RetryPolicy::none());
    let (result, _events) = run(&provider, &ask(Vec::new()), &Cancellation::new());

    let response = result.expect("streamed response");
    assert_eq!(response.stop_reason, StopReason::ToolCalls);
    assert_eq!(response.tool_calls.len(), 2);
    assert_eq!(response.tool_calls[0].id, "call_a");
    assert_eq!(response.tool_calls[0].name, "get_board");
    assert_eq!(
        response.tool_calls[0].arguments,
        serde_json::json!({"board_id": "b1"})
    );
    assert_eq!(response.tool_calls[1].name, "list_boards");
    assert_eq!(response.tool_calls[1].arguments, serde_json::json!({}));
}

#[test]
fn provider_surfaces_an_in_stream_error_object() {
    let mock = Mock::start(vec![Reply::Sse(vec![
        delta("partial"),
        sse(r#"{"error":{"message":"context window exceeded","type":"context_length_exceeded"}}"#),
    ])]);
    let provider = provider(&mock, RetryPolicy::none());
    let (result, _events) = run(&provider, &ask(Vec::new()), &Cancellation::new());

    let error = result.expect_err("stream error");
    assert_eq!(error.code, error_code::CONTEXT_TOO_LONG);
    assert!(error.message.contains("context window exceeded"));
}

// --- request shape ---------------------------------------------------------

#[test]
fn provider_sends_the_model_tools_and_bearer_token() {
    let mock = Mock::start(vec![Reply::Sse(vec![delta("ok"), DONE.to_string()])]);
    let provider = provider(&mock, RetryPolicy::none());
    let registry = rbc_ai::tools::ToolRegistry::builtin();
    let tools: Vec<ToolDefinition> = ["get_board", "play_move"]
        .iter()
        .filter_map(|name| registry.get(name))
        .map(ToolDefinition::from)
        .collect();
    let (result, _events) = run(&provider, &ask(tools), &Cancellation::new());
    result.expect("streamed response");

    let headers = mock.request_headers();
    let head = headers.first().expect("one request");
    assert!(
        head.to_ascii_lowercase()
            .contains("authorization: bearer sk-test-secret"),
        "authorization header missing from {head}"
    );
    assert!(head.to_ascii_lowercase().contains("accept: text/event-stream"));

    let bodies = mock.request_bodies();
    let body: serde_json::Value = serde_json::from_str(&bodies[0]).expect("request json");
    assert_eq!(body["model"], serde_json::json!("knightlm-test"));
    assert_eq!(body["stream"], serde_json::json!(true));
    assert_eq!(body["stream_options"]["include_usage"], serde_json::json!(true));
    assert_eq!(body["tool_choice"], serde_json::json!("auto"));
    assert_eq!(body["messages"][0]["role"], serde_json::json!("user"));
    assert_eq!(body["messages"][0]["content"], serde_json::json!("analyze this"));
    let names: Vec<&str> = body["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .map(|tool| tool["function"]["name"].as_str().expect("tool name"))
        .collect();
    assert_eq!(names, vec!["get_board", "play_move"]);
    // Secrets must never be baked into the body.
    assert!(!bodies[0].contains("sk-test-secret"));
}

// --- failures and retries --------------------------------------------------

#[test]
fn provider_retries_a_rate_limit_and_then_succeeds() {
    let mock = Mock::start(vec![
        Reply::Status {
            status: 429,
            body: r#"{"error":{"message":"slow down"}}"#.to_string(),
        },
        Reply::Sse(vec![delta("recovered"), DONE.to_string()]),
    ]);
    let provider = provider(&mock, short_retry());
    let (result, _events) = run(&provider, &ask(Vec::new()), &Cancellation::new());

    let response = result.expect("second attempt succeeds");
    assert_eq!(response.content, "recovered");
    assert_eq!(mock.requests(), 2);
}

#[test]
fn provider_gives_up_after_the_retry_budget() {
    let down = Reply::Status {
        status: 503,
        body: "upstream unavailable".to_string(),
    };
    let mock = Mock::start(vec![
        Reply::Status {
            status: 503,
            body: "upstream unavailable".to_string(),
        },
        down,
        Reply::Status {
            status: 503,
            body: "upstream unavailable".to_string(),
        },
        Reply::Status {
            status: 503,
            body: "upstream unavailable".to_string(),
        },
    ]);
    let provider = provider(&mock, short_retry());
    let (result, _events) = run(&provider, &ask(Vec::new()), &Cancellation::new());

    let error = result.expect_err("all attempts fail");
    assert_eq!(error.code, error_code::SERVER);
    assert!(error.retryable);
    assert!(error.message.contains("upstream unavailable"));
    assert_eq!(mock.requests(), 3, "three attempts, no more");
}

#[test]
fn provider_does_not_retry_a_rejected_key() {
    let mock = Mock::start(vec![Reply::Status {
        status: 401,
        body: r#"{"error":{"message":"invalid api key"}}"#.to_string(),
    }]);
    let provider = provider(&mock, short_retry());
    let (result, _events) = run(&provider, &ask(Vec::new()), &Cancellation::new());

    let error = result.expect_err("rejected key");
    assert_eq!(error.code, error_code::AUTH);
    assert!(!error.retryable);
    assert_eq!(mock.requests(), 1);
    // The key must not be echoed back to the user.
    assert!(!error.message.contains("sk-test-secret"));
}

#[test]
fn provider_does_not_retry_after_content_was_shown() {
    let mock = Mock::start(vec![
        Reply::BrokenSse(vec![delta("half an ")])
    ]);
    let provider = provider(&mock, short_retry());
    let mut events = Vec::new();
    let result = provider.complete(&ask(Vec::new()), &Cancellation::new(), &mut |event| {
        events.push(event);
    });

    let error = result.expect_err("stream died");
    assert!(!error.retryable || error.code == error_code::CANCELLED);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ProviderEvent::ContentDelta(text) if text == "half an ")),
        "the partial answer should still have been streamed"
    );
    assert_eq!(mock.requests(), 1, "a half-shown turn is not replayed");
}

// --- non-streaming and legacy servers --------------------------------------

#[test]
fn provider_reads_a_single_json_completion() {
    let mock = Mock::start(vec![Reply::Json(
        r#"{"id":"cmpl-9","choices":[{"message":{"content":"One shot."},"finish_reason":"stop"}],"usage":{"prompt_tokens":11,"completion_tokens":2}}"#.to_string(),
    )]);
    let provider = provider(&mock, RetryPolicy::none());
    let (result, events) = run(&provider, &ask(Vec::new()), &Cancellation::new());

    let response = result.expect("json completion");
    assert_eq!(response.content, "One shot.");
    assert_eq!(response.usage.tokens_in, 11);
    assert_eq!(response.stop_reason, StopReason::EndTurn);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, ProviderEvent::ContentDelta(_)))
            .count(),
        1
    );
}

#[test]
fn provider_reads_a_tool_call_from_a_single_json_completion() {
    let mock = Mock::start(vec![Reply::Json(
        r#"{"choices":[{"message":{"content":null,"tool_calls":[{"id":"call_x","type":"function","function":{"name":"play_move","arguments":"{\"move\":\"e4\"}"}}]},"finish_reason":"tool_calls"}]}"#.to_string(),
    )]);
    let provider = provider(&mock, RetryPolicy::none());
    let (result, _events) = run(&provider, &ask(Vec::new()), &Cancellation::new());

    let response = result.expect("json completion");
    assert_eq!(response.stop_reason, StopReason::ToolCalls);
    assert_eq!(response.tool_calls.len(), 1);
    assert_eq!(response.tool_calls[0].arguments, serde_json::json!({"move": "e4"}));
}

#[test]
fn provider_reads_the_plain_text_reply_shape() {
    let mock = Mock::start(vec![Reply::Json(
        r#"{"reply":"Plain servers still work."}"#.to_string(),
    )]);
    let provider = provider(&mock, RetryPolicy::none());
    let (result, _events) = run(&provider, &ask(Vec::new()), &Cancellation::new());

    let response = result.expect("plain reply");
    assert_eq!(response.content, "Plain servers still work.");
    assert_eq!(response.stop_reason, StopReason::EndTurn);
}

// --- cancellation ----------------------------------------------------------

#[test]
fn provider_stops_before_connecting_when_already_cancelled() {
    let mock = Mock::start(vec![Reply::Sse(vec![delta("never"), DONE.to_string()])]);
    let provider = provider(&mock, short_retry());
    let cancel = Cancellation::new();
    cancel.cancel();
    let (result, _events) = run(&provider, &ask(Vec::new()), &cancel);

    let error = result.expect_err("cancelled");
    assert_eq!(error.code, error_code::CANCELLED);
    assert_eq!(mock.requests(), 0, "a cancelled turn makes no request");
}

#[test]
fn provider_stops_mid_stream_when_cancelled() {
    let mock = Mock::start(vec![Reply::Sse(vec![
        delta("first "),
        delta("second "),
        delta("third"),
        DONE.to_string(),
    ])]);
    let provider = provider(&mock, short_retry());
    let cancel = Cancellation::new();
    let mut events = Vec::new();
    let result = provider.complete(&ask(Vec::new()), &cancel, &mut |event| {
        // A person hitting stop after the first visible delta.
        if matches!(event, ProviderEvent::ContentDelta(_)) {
            cancel.cancel();
        }
        events.push(event);
    });

    let error = result.expect_err("cancelled mid-stream");
    assert_eq!(error.code, error_code::CANCELLED);
    assert_eq!(mock.requests(), 1, "cancelling is not a retryable failure");
}

// --- configuration ---------------------------------------------------------

#[test]
fn provider_refuses_to_stream_to_an_unconfigured_endpoint() {
    let config = ProviderConfig::openai_compatible("", "mock", "m");
    let error = OpenAiProvider::new(config).expect_err("no endpoint");
    assert_eq!(error.code, error_code::NOT_CONFIGURED);
}

#[test]
fn provider_reports_a_connection_failure_as_network_and_retries() {
    // Port 1 on loopback refuses connections immediately.
    let config = ProviderConfig::openai_compatible(
        "http://127.0.0.1:1/v1/chat/completions",
        "mock",
        "m",
    )
    .with_retry(short_retry())
    .with_timeouts(
        Duration::from_millis(200),
        Duration::from_millis(200),
        Duration::from_millis(500),
    );
    let provider = OpenAiProvider::new(config).expect("valid config");
    let (result, _events) = run(&provider, &ask(Vec::new()), &Cancellation::new());

    let error = result.expect_err("nothing is listening");
    assert_eq!(error.code, error_code::NETWORK);
    assert!(error.retryable);
}
