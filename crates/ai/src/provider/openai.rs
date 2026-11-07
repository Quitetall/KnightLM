//! The OpenAI-compatible chat-completions adapter.
//!
//! This is the first [`Provider`] implementation and the one an OpenAI gateway,
//! llama.cpp, Ollama's compatibility endpoint and local mock servers all speak.
//! It reads both the streamed (`text/event-stream`) and the single-shot (`application/json`)
//! shapes, because a local server may answer either even when asked to stream.
//!
//! Both shapes go through [`Accumulator`], so the assembled
//! [`CompletionResponse`] is built by exactly one piece of code.

use std::cell::Cell;
use std::io::{BufRead, BufReader, Read};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{Value, json};

use crate::protocol::{ErrorInfo, StopReason, ToolCall, TurnRole, Usage};
use crate::provider::sse::{SseEvent, SseParser};
use crate::provider::{
    Cancellation, ChatMessage, CompletionRequest, CompletionResponse, Provider, ProviderConfig,
    ProviderEvent, error, error_code, error_for_status, retry_after_seconds,
};

/// An OpenAI-compatible endpoint.
#[derive(Debug)]
pub struct OpenAiProvider {
    config: ProviderConfig,
    agent: ureq::Agent,
    thinking_budget: Option<(bool, u32)>,
}

impl OpenAiProvider {
    /// Build a provider, rejecting a configuration that cannot reach anything.
    pub fn new(config: ProviderConfig) -> Result<Self, ErrorInfo> {
        config.validate()?;
        let builder = ureq::config::Config::builder()
            // Statuses are mapped to [`ErrorInfo`] by hand below.
            .http_status_as_error(false)
            .timeout_global(Some(config.total_timeout))
            .timeout_connect(Some(config.connect_timeout))
            .timeout_recv_body(Some(config.idle_timeout));
        let agent = ureq::Agent::new_with_config(builder.build());
        Ok(Self {
            config,
            agent,
            thinking_budget: None,
        })
    }

    /// Limit move-only requests on endpoints that support `enable_thinking`.
    pub fn with_direct_answers(mut self, max_tokens: u32) -> Self {
        self.thinking_budget = Some((false, max_tokens));
        self
    }

    /// Give a compatible model more room to reason before answering.
    pub fn with_thinking(mut self, max_tokens: u32) -> Self {
        self.thinking_budget = Some((true, max_tokens));
        self
    }

    /// The configuration this provider was built from.
    pub fn config(&self) -> &ProviderConfig {
        &self.config
    }

    fn request_body(&self, request: &CompletionRequest) -> Value {
        let messages: Vec<Value> = request.messages.iter().map(message_json).collect();
        let mut body = json!({
            "model": request.model,
            "messages": messages,
            "temperature": request.temperature,
            "stream": request.stream,
        });
        if request.stream {
            // Ask for a final usage chunk; servers that do not know the field
            // ignore it.
            body["stream_options"] = json!({ "include_usage": true });
        }
        if let Some((enabled, _)) = self.thinking_budget {
            body["enable_thinking"] = json!(enabled);
        }
        if let Some(max_tokens) = request
            .max_tokens
            .or(self.thinking_budget.map(|(_, budget)| budget))
        {
            body["max_tokens"] = json!(max_tokens);
        }
        if !request.tools.is_empty() {
            let tools: Vec<Value> = request
                .tools
                .iter()
                .map(|tool| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": tool.parameters,
                        }
                    })
                })
                .collect();
            body["tools"] = json!(tools);
            body["tool_choice"] = json!("auto");
        }
        body
    }

    fn attempt(
        &self,
        request: &CompletionRequest,
        cancel: &Cancellation,
        on_event: &mut dyn FnMut(ProviderEvent),
    ) -> Result<CompletionResponse, AttemptError> {
        let started = Instant::now();
        let body = self.request_body(request);

        let mut builder = self.agent.post(&self.config.endpoint).header(
            "accept",
            if request.stream {
                "text/event-stream"
            } else {
                "application/json"
            },
        );
        if let Some(key) = &self.config.api_key {
            builder = builder.header("authorization", format!("Bearer {key}"));
        }
        for (name, value) in &self.config.extra_headers {
            builder = builder.header(name.as_str(), value.as_str());
        }

        let mut response = builder.send_json(body).map_err(map_transport_error)?;
        let status = response.status().as_u16();
        if status >= 400 {
            let retry_after = retry_after_seconds(
                response
                    .headers()
                    .get("retry-after")
                    .and_then(|value| value.to_str().ok()),
            );
            // A bounded read: an error body is a diagnostic, not content.
            let text = read_body_limited(&mut response, 64 * 1024).unwrap_or_default();
            return Err(AttemptError {
                info: error_for_status(status, &text),
                retry_after,
            });
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();

        if request.stream && content_type.contains("text/event-stream") {
            self.consume_stream(&mut response, cancel, on_event, started)
        } else {
            self.consume_json(&mut response, on_event, started)
        }
    }

    fn consume_json(
        &self,
        response: &mut ureq::http::Response<ureq::Body>,
        on_event: &mut dyn FnMut(ProviderEvent),
        started: Instant,
    ) -> Result<CompletionResponse, AttemptError> {
        let text = read_body_limited(response, self.config.max_response_bytes).map_err(|info| {
            AttemptError {
                info,
                retry_after: None,
            }
        })?;
        let wire: CompletionWire = serde_json::from_str(&text).map_err(|e| AttemptError {
            info: error(
                error_code::PROTOCOL,
                format!("the model response was not JSON: {e}"),
                false,
            ),
            retry_after: None,
        })?;
        if let Some(detail) = wire.error {
            return Err(AttemptError {
                info: wire_error(detail),
                retry_after: None,
            });
        }

        let mut accumulator = Accumulator {
            request_id: wire.id.clone(),
            ..Accumulator::default()
        };

        // A legacy plain-text server: one field, no choices.
        if let Some(reply) = wire.reply {
            accumulator.apply(&ProviderEvent::ContentDelta(reply), on_event);
        }
        if let Some(choice) = wire.choices.first() {
            let message = &choice.message;
            if let Some(reasoning) = message.reasoning_text() {
                accumulator.apply(
                    &ProviderEvent::ReasoningDelta(reasoning.to_string()),
                    on_event,
                );
            }
            if let Some(content) = &message.content {
                accumulator.apply(&ProviderEvent::ContentDelta(content.clone()), on_event);
            }
            for (index, call) in message.tool_calls.iter().enumerate() {
                accumulator.apply(
                    &ProviderEvent::ToolCallDelta {
                        index,
                        id: call.id.clone(),
                        name: Some(call.function.name.clone()),
                        arguments: arguments_string(&call.function.arguments),
                    },
                    on_event,
                );
            }
            accumulator.stop = choice.finish_reason.as_deref().and_then(stop_reason);
        }
        if let Some(usage) = wire.usage {
            accumulator.apply(&ProviderEvent::Usage(usage.into_usage()), on_event);
        }
        accumulator.finish(started).map_err(|info| AttemptError {
            info,
            retry_after: None,
        })
    }

    fn consume_stream(
        &self,
        response: &mut ureq::http::Response<ureq::Body>,
        cancel: &Cancellation,
        on_event: &mut dyn FnMut(ProviderEvent),
        started: Instant,
    ) -> Result<CompletionResponse, AttemptError> {
        let mut reader = BufReader::new(response.body_mut().as_reader());
        let mut parser = SseParser::new();
        let mut accumulator = Accumulator::default();
        let mut line = Vec::new();
        let mut total = 0usize;

        loop {
            cancel.check().map_err(|info| AttemptError {
                info,
                retry_after: None,
            })?;
            line.clear();
            let read = reader
                .read_until(b'\n', &mut line)
                .map_err(map_read_error)?;
            if read == 0 {
                break;
            }
            total += read;
            if total > self.config.max_response_bytes {
                return Err(AttemptError {
                    info: error(
                        error_code::TOO_LARGE,
                        format!(
                            "the model stream exceeded {} bytes",
                            self.config.max_response_bytes
                        ),
                        false,
                    ),
                    retry_after: None,
                });
            }
            for event in parser.push(&line) {
                if self.handle_event(event, &mut accumulator, on_event)? {
                    return accumulator.finish(started).map_err(|info| AttemptError {
                        info,
                        retry_after: None,
                    });
                }
            }
        }

        // A server that closes the socket without a blank line still gets its
        // last event parsed.
        if let Some(event) = parser.finish() {
            self.handle_event(event, &mut accumulator, on_event)?;
        }
        accumulator.finish(started).map_err(|info| AttemptError {
            info,
            retry_after: None,
        })
    }

    /// Apply one SSE event. Returns `true` when the stream is complete.
    fn handle_event(
        &self,
        event: SseEvent,
        accumulator: &mut Accumulator,
        on_event: &mut dyn FnMut(ProviderEvent),
    ) -> Result<bool, AttemptError> {
        if !event.has_data() {
            return Ok(false);
        }
        if event.data.trim() == "[DONE]" {
            return Ok(true);
        }
        let chunk: StreamChunk = match serde_json::from_str(&event.data) {
            Ok(chunk) => chunk,
            Err(e) => {
                return Err(AttemptError {
                    info: error(
                        error_code::PROTOCOL,
                        format!("the model stream sent a chunk that is not JSON: {e}"),
                        false,
                    ),
                    retry_after: None,
                });
            }
        };
        if let Some(detail) = chunk.error {
            return Err(AttemptError {
                info: wire_error(detail),
                retry_after: None,
            });
        }
        if chunk.id.is_some() {
            accumulator.request_id = chunk.id.clone();
        }
        for choice in &chunk.choices {
            if let Some(text) = choice.delta.content.as_ref().filter(|t| !t.is_empty()) {
                accumulator.apply(&ProviderEvent::ContentDelta(text.clone()), on_event);
            }
            if let Some(text) = choice.delta.reasoning_text().filter(|t| !t.is_empty()) {
                accumulator.apply(&ProviderEvent::ReasoningDelta(text.to_string()), on_event);
            }
            for call in &choice.delta.tool_calls {
                let arguments = call
                    .function
                    .as_ref()
                    .map(|function| function.arguments.clone().unwrap_or_default())
                    .unwrap_or_default();
                accumulator.apply(
                    &ProviderEvent::ToolCallDelta {
                        index: call.index,
                        id: call.id.clone(),
                        name: call
                            .function
                            .as_ref()
                            .and_then(|function| function.name.clone()),
                        arguments,
                    },
                    on_event,
                );
            }
            if let Some(reason) = choice.finish_reason.as_deref().and_then(stop_reason) {
                accumulator.apply(&ProviderEvent::Stopped(reason), on_event);
            }
        }
        if let Some(usage) = &chunk.usage {
            accumulator.apply(&ProviderEvent::Usage(usage.clone().into_usage()), on_event);
        }
        Ok(false)
    }
}

impl Provider for OpenAiProvider {
    fn name(&self) -> &str {
        self.config.kind.as_str()
    }

    fn complete(
        &self,
        request: &CompletionRequest,
        cancel: &Cancellation,
        on_event: &mut dyn FnMut(ProviderEvent),
    ) -> Result<CompletionResponse, ErrorInfo> {
        self.config.validate()?;
        let mut attempts = 0u32;
        loop {
            cancel.check()?;
            attempts += 1;
            let emitted = Cell::new(false);
            let mut tracked = |event: ProviderEvent| {
                if matches!(
                    event,
                    ProviderEvent::ContentDelta(_)
                        | ProviderEvent::ReasoningDelta(_)
                        | ProviderEvent::ToolCallDelta { .. }
                ) {
                    emitted.set(true);
                }
                on_event(event);
            };
            match self.attempt(request, cancel, &mut tracked) {
                Ok(response) => return Ok(response),
                Err(failure) => {
                    // Retrying after content was shown would duplicate text, so
                    // only a failure before the first delta is retried.
                    let retry = failure.info.retryable
                        && !emitted.get()
                        && !cancel.is_cancelled()
                        && !self.config.retry.is_exhausted(attempts);
                    if !retry {
                        return Err(failure.info);
                    }
                    let wait = self.config.retry.delay(attempts, failure.retry_after);
                    if !wait.is_zero() {
                        std::thread::sleep(wait);
                    }
                }
            }
        }
    }
}

/// A failed attempt, with the header the retry loop needs.
#[derive(Debug)]
struct AttemptError {
    info: ErrorInfo,
    retry_after: Option<Duration>,
}

/// Assembles deltas into a response.
#[derive(Debug, Default)]
struct Accumulator {
    content: String,
    reasoning: String,
    calls: Vec<CallDraft>,
    usage: Usage,
    stop: Option<StopReason>,
    request_id: Option<String>,
}

#[derive(Debug)]
struct CallDraft {
    index: usize,
    id: String,
    name: String,
    arguments: String,
}

impl Accumulator {
    fn apply(&mut self, event: &ProviderEvent, on_event: &mut dyn FnMut(ProviderEvent)) {
        match event {
            ProviderEvent::ContentDelta(text) => self.content.push_str(text),
            ProviderEvent::ReasoningDelta(text) => self.reasoning.push_str(text),
            ProviderEvent::ToolCallDelta {
                index,
                id,
                name,
                arguments,
            } => {
                let position = match self.calls.iter().position(|call| call.index == *index) {
                    Some(position) => position,
                    None => {
                        self.calls.push(CallDraft {
                            index: *index,
                            id: String::new(),
                            name: String::new(),
                            arguments: String::new(),
                        });
                        self.calls.len() - 1
                    }
                };
                let call = &mut self.calls[position];
                if let Some(id) = id.as_ref().filter(|_| call.id.is_empty()) {
                    call.id = id.clone();
                }
                if let Some(name) = name.as_ref().filter(|_| call.name.is_empty()) {
                    call.name = name.clone();
                }
                call.arguments.push_str(arguments);
            }
            ProviderEvent::Usage(usage) => {
                if usage.tokens_in > 0 {
                    self.usage.tokens_in = usage.tokens_in;
                }
                if usage.tokens_out > 0 {
                    self.usage.tokens_out = usage.tokens_out;
                }
                if usage.cost_estimate.is_some() {
                    self.usage.cost_estimate = usage.cost_estimate;
                }
            }
            ProviderEvent::Stopped(reason) => self.stop = Some(*reason),
        }
        on_event(event.clone());
    }

    fn finish(self, started: Instant) -> Result<CompletionResponse, ErrorInfo> {
        let mut tool_calls = Vec::with_capacity(self.calls.len());
        for call in &self.calls {
            if call.name.trim().is_empty() {
                return Err(error(
                    error_code::PROTOCOL,
                    format!("the model streamed tool call {} without a name", call.index),
                    false,
                ));
            }
            let raw = call.arguments.trim();
            let arguments = if raw.is_empty() {
                json!({})
            } else {
                serde_json::from_str(raw).map_err(|e| {
                    error(
                        error_code::PROTOCOL,
                        format!(
                            "the model sent arguments for {} that are not JSON: {e}",
                            call.name
                        ),
                        false,
                    )
                })?
            };
            tool_calls.push(ToolCall {
                id: if call.id.is_empty() {
                    format!("call_{}", call.index)
                } else {
                    call.id.clone()
                },
                name: call.name.clone(),
                arguments,
            });
        }

        // A server that says "stop" while having sent tool calls means tool
        // calls; the calls are the reason the turn is not finished.
        let stop_reason = match self.stop {
            Some(StopReason::EndTurn) if !tool_calls.is_empty() => StopReason::ToolCalls,
            Some(reason) => reason,
            None if tool_calls.is_empty() => StopReason::EndTurn,
            None => StopReason::ToolCalls,
        };
        let mut usage = self.usage;
        usage.latency_ms = started.elapsed().as_millis() as u64;

        Ok(CompletionResponse {
            content: self.content,
            reasoning: (!self.reasoning.is_empty()).then_some(self.reasoning),
            tool_calls,
            usage,
            stop_reason,
            provider_request_id: self.request_id,
        })
    }
}

fn message_json(message: &ChatMessage) -> Value {
    match message.role {
        TurnRole::Tool => json!({
            "role": "tool",
            "tool_call_id": message.tool_call_id.clone().unwrap_or_default(),
            "content": message.content.clone().unwrap_or_default(),
        }),
        TurnRole::Assistant if !message.tool_calls.is_empty() => {
            let calls: Vec<Value> = message
                .tool_calls
                .iter()
                .map(|call| {
                    json!({
                        "id": call.id,
                        "type": "function",
                        "function": {
                            "name": call.name,
                            // The wire carries arguments as a JSON string.
                            "arguments": serde_json::to_string(&call.arguments)
                                .unwrap_or_else(|_| "{}".to_string()),
                        }
                    })
                })
                .collect();
            json!({
                "role": "assistant",
                "content": message.content.clone(),
                "tool_calls": calls,
            })
        }
        role => json!({
            "role": role.as_str(),
            "content": message.content.clone().unwrap_or_default(),
        }),
    }
}

fn arguments_string(value: &Value) -> String {
    match value {
        Value::Null => "{}".to_string(),
        Value::String(text) => text.clone(),
        other => serde_json::to_string(other).unwrap_or_else(|_| "{}".to_string()),
    }
}

fn stop_reason(reason: &str) -> Option<StopReason> {
    match reason {
        "stop" | "end_turn" | "content_filter" => Some(StopReason::EndTurn),
        "tool_calls" | "function_call" => Some(StopReason::ToolCalls),
        "length" | "max_tokens" => Some(StopReason::MaxTokens),
        "error" => Some(StopReason::Error),
        _ => None,
    }
}

fn wire_error(detail: WireError) -> ErrorInfo {
    let message = detail
        .message
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| "the model provider reported an error".to_string());
    let code = match detail.kind.as_deref() {
        Some("invalid_request_error") | Some("invalid_request") => error_code::BAD_REQUEST,
        Some("authentication_error") | Some("permission_error") => error_code::AUTH,
        Some("rate_limit_error") | Some("rate_limit_exceeded") => error_code::RATE_LIMITED,
        Some("context_length_exceeded") => error_code::CONTEXT_TOO_LONG,
        Some("server_error") | Some("api_error") | Some("overloaded_error") => error_code::SERVER,
        _ => error_code::PROTOCOL,
    };
    let mut info = error(code, message, false);
    info.provider_code = detail.code;
    info
}

fn map_transport_error(failure: ureq::Error) -> AttemptError {
    let info = match &failure {
        ureq::Error::Timeout(_) => error(
            error_code::TIMEOUT,
            "the model provider did not answer in time",
            true,
        ),
        ureq::Error::Io(io) => error(
            error_code::NETWORK,
            format!("could not reach the model provider: {io}"),
            true,
        ),
        ureq::Error::ConnectionFailed | ureq::Error::HostNotFound => error(
            error_code::NETWORK,
            "could not connect to the model provider",
            true,
        ),
        ureq::Error::BadUri(reason) => error(
            error_code::NOT_CONFIGURED,
            format!("the model endpoint is not a usable URL: {reason}"),
            false,
        ),
        ureq::Error::BodyExceedsLimit(limit) => error(
            error_code::TOO_LARGE,
            format!("the model response exceeded {limit} bytes"),
            false,
        ),
        ureq::Error::Protocol(_) => error(
            error_code::PROTOCOL,
            "the model provider broke the HTTP protocol",
            false,
        ),
        other => error(
            error_code::NETWORK,
            format!("the model request failed: {other}"),
            false,
        ),
    };
    AttemptError {
        info,
        retry_after: None,
    }
}

/// A failure while reading the response body.
///
/// The body reader reports `io::Error`, so a stalled stream and a truncated one
/// are told apart here rather than inside the read loop.
fn map_read_error(failure: std::io::Error) -> AttemptError {
    let (code, message, retryable) = match failure.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => (
            error_code::TIMEOUT,
            "the model stream stopped sending data".to_string(),
            true,
        ),
        // Some servers close the socket instead of writing `[DONE]`; the
        // accumulator still holds everything that arrived.
        std::io::ErrorKind::UnexpectedEof => (
            error_code::PROTOCOL,
            "the model stream ended before it completed".to_string(),
            false,
        ),
        _ => (
            error_code::NETWORK,
            format!("the model stream could not be read: {failure}"),
            true,
        ),
    };
    AttemptError {
        info: error(code, message, retryable),
        retry_after: None,
    }
}

fn read_body_limited(
    response: &mut ureq::http::Response<ureq::Body>,
    limit: usize,
) -> Result<String, ErrorInfo> {
    let mut reader = response.body_mut().as_reader().take(limit as u64);
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer).map_err(|failure| {
        error(
            error_code::NETWORK,
            format!("the model response could not be read: {failure}"),
            false,
        )
    })?;
    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

// --- wire shapes -----------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CompletionWire {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    choices: Vec<CompletionChoice>,
    #[serde(default)]
    usage: Option<WireUsage>,
    /// Plain-text servers used by local tests and fixtures.
    #[serde(default)]
    reply: Option<String>,
    #[serde(default)]
    error: Option<WireError>,
}

#[derive(Debug, Deserialize)]
struct CompletionChoice {
    #[serde(default)]
    message: WireMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct WireMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<WireCall>,
}

impl WireMessage {
    fn reasoning_text(&self) -> Option<&str> {
        self.reasoning
            .as_deref()
            .or(self.reasoning_content.as_deref())
            .filter(|text| !text.is_empty())
    }
}

#[derive(Debug, Deserialize)]
struct WireCall {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: WireFunction,
}

#[derive(Debug, Default, Deserialize)]
struct WireFunction {
    #[serde(default)]
    name: String,
    #[serde(default)]
    arguments: Value,
}

#[derive(Debug, Deserialize)]
struct StreamChunk {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    choices: Vec<StreamChoice>,
    #[serde(default)]
    usage: Option<WireUsage>,
    #[serde(default)]
    error: Option<WireError>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    #[serde(default)]
    delta: StreamDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<StreamCall>,
}

impl StreamDelta {
    fn reasoning_text(&self) -> Option<&str> {
        self.reasoning
            .as_deref()
            .or(self.reasoning_content.as_deref())
            .filter(|text| !text.is_empty())
    }
}

#[derive(Debug, Deserialize)]
struct StreamCall {
    #[serde(default)]
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<StreamFunction>,
}

#[derive(Debug, Deserialize)]
struct StreamFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct WireUsage {
    #[serde(default, alias = "input_tokens")]
    prompt_tokens: u32,
    #[serde(default, alias = "output_tokens")]
    completion_tokens: u32,
}

impl WireUsage {
    fn into_usage(self) -> Usage {
        Usage {
            tokens_in: self.prompt_tokens,
            tokens_out: self.completion_tokens,
            latency_ms: 0,
            cost_estimate: None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct WireError {
    #[serde(default)]
    message: Option<String>,
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    code: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(index: usize, id: &str, name: &str, arguments: &str) -> CallDraft {
        CallDraft {
            index,
            id: id.to_string(),
            name: name.to_string(),
            arguments: arguments.to_string(),
        }
    }

    #[test]
    fn provider_accumulator_merges_tool_call_fragments_by_index() {
        let mut accumulator = Accumulator::default();
        let mut seen = |_event: ProviderEvent| {};
        // The id and name arrive once; the arguments dribble in per index.
        accumulator.apply(
            &ProviderEvent::ToolCallDelta {
                index: 0,
                id: Some("call_a".to_string()),
                name: Some("get_board".to_string()),
                arguments: "{\"board_".to_string(),
            },
            &mut seen,
        );
        accumulator.apply(
            &ProviderEvent::ToolCallDelta {
                index: 0,
                id: None,
                name: None,
                arguments: "id\":\"b1\"}".to_string(),
            },
            &mut seen,
        );
        accumulator.apply(
            &ProviderEvent::ToolCallDelta {
                index: 1,
                id: Some("call_b".to_string()),
                name: Some("list_boards".to_string()),
                arguments: "{}".to_string(),
            },
            &mut seen,
        );

        let response = accumulator.finish(Instant::now()).expect("valid response");
        assert_eq!(response.tool_calls.len(), 2);
        assert_eq!(response.tool_calls[0].id, "call_a");
        assert_eq!(response.tool_calls[0].name, "get_board");
        assert_eq!(response.tool_calls[0].arguments, json!({"board_id": "b1"}));
        assert_eq!(response.tool_calls[1].name, "list_boards");
        assert_eq!(response.stop_reason, StopReason::ToolCalls);
    }

    #[test]
    fn provider_accumulator_names_an_unnamed_tool_call_a_protocol_error() {
        let accumulator = Accumulator {
            calls: vec![draft(0, "call_a", "", "{}")],
            ..Accumulator::default()
        };
        let error = accumulator
            .finish(Instant::now())
            .expect_err("no tool name");
        assert_eq!(error.code, error_code::PROTOCOL);
        assert!(error.message.contains("without a name"));
    }

    #[test]
    fn provider_accumulator_rejects_arguments_that_are_not_json() {
        let accumulator = Accumulator {
            calls: vec![draft(0, "call_a", "play_move", "{oops")],
            ..Accumulator::default()
        };
        let error = accumulator
            .finish(Instant::now())
            .expect_err("bad arguments");
        assert_eq!(error.code, error_code::PROTOCOL);
        assert!(error.message.contains("play_move"));
    }

    #[test]
    fn provider_accumulator_defaults_empty_arguments_to_an_object() {
        let accumulator = Accumulator {
            calls: vec![draft(0, "call_a", "list_boards", "   ")],
            ..Accumulator::default()
        };
        let response = accumulator.finish(Instant::now()).expect("valid response");
        assert_eq!(response.tool_calls[0].arguments, json!({}));
    }

    #[test]
    fn provider_accumulator_keeps_reasoning_out_of_content() {
        let mut accumulator = Accumulator::default();
        let mut seen = |_event: ProviderEvent| {};
        accumulator.apply(&ProviderEvent::ReasoningDelta("thinking".into()), &mut seen);
        accumulator.apply(&ProviderEvent::ContentDelta("answer".into()), &mut seen);
        let response = accumulator.finish(Instant::now()).expect("valid response");
        assert_eq!(response.content, "answer");
        assert_eq!(response.reasoning.as_deref(), Some("thinking"));
    }

    #[test]
    fn provider_accumulator_treats_stop_with_calls_as_tool_calls() {
        let accumulator = Accumulator {
            calls: vec![draft(0, "call_a", "list_boards", "{}")],
            stop: Some(StopReason::EndTurn),
            ..Accumulator::default()
        };
        let response = accumulator.finish(Instant::now()).expect("valid response");
        assert_eq!(response.stop_reason, StopReason::ToolCalls);
    }

    #[test]
    fn provider_accumulator_reports_latency_and_request_id() {
        let accumulator = Accumulator {
            request_id: Some("req_1".to_string()),
            ..Accumulator::default()
        };
        let response = accumulator
            .finish(Instant::now() - Duration::from_millis(5))
            .expect("valid response");
        assert_eq!(response.provider_request_id.as_deref(), Some("req_1"));
        assert!(response.usage.latency_ms >= 5);
    }

    #[test]
    fn provider_finish_reasons_are_normalized() {
        assert_eq!(stop_reason("stop"), Some(StopReason::EndTurn));
        assert_eq!(stop_reason("tool_calls"), Some(StopReason::ToolCalls));
        assert_eq!(stop_reason("length"), Some(StopReason::MaxTokens));
        assert_eq!(stop_reason("error"), Some(StopReason::Error));
        assert_eq!(stop_reason("something_new"), None);
    }

    #[test]
    fn provider_message_encoding_covers_every_role() {
        let call = ToolCall::new("call_1", "play_move", json!({"move": "e4"}));
        let assistant = message_json(&ChatMessage::assistant_tools(None, vec![call]));
        assert_eq!(assistant["role"], json!("assistant"));
        assert_eq!(assistant["content"], Value::Null);
        assert_eq!(assistant["tool_calls"][0]["type"], json!("function"));
        assert_eq!(
            assistant["tool_calls"][0]["function"]["arguments"],
            json!("{\"move\":\"e4\"}")
        );

        let tool = message_json(&ChatMessage::tool_result("call_1", "played e4"));
        assert_eq!(tool["role"], json!("tool"));
        assert_eq!(tool["tool_call_id"], json!("call_1"));

        let user = message_json(&ChatMessage::user("hello"));
        assert_eq!(user["role"], json!("user"));
        assert_eq!(user["content"], json!("hello"));
    }

    #[test]
    fn provider_wire_errors_map_to_stable_codes() {
        let limited = wire_error(WireError {
            message: Some("slow down".into()),
            kind: Some("rate_limit_exceeded".into()),
            code: Some("rate_limit".into()),
        });
        assert_eq!(limited.code, error_code::RATE_LIMITED);
        assert_eq!(limited.provider_code.as_deref(), Some("rate_limit"));

        let long = wire_error(WireError {
            message: None,
            kind: Some("context_length_exceeded".into()),
            code: None,
        });
        assert_eq!(long.code, error_code::CONTEXT_TOO_LONG);
        assert!(!long.message.is_empty());
    }

    #[test]
    fn provider_request_body_offers_tools_and_usage() {
        let config = ProviderConfig::openai_compatible("http://localhost:1/v1", "mock", "m1");
        let provider = OpenAiProvider::new(config).expect("valid config");
        let registry = crate::tools::ToolRegistry::builtin();
        let tools: Vec<crate::provider::ToolDefinition> = ["get_board", "play_move"]
            .iter()
            .filter_map(|name| registry.get(name))
            .map(Into::into)
            .collect();
        let request = CompletionRequest::new("m1", vec![ChatMessage::user("hi")])
            .with_tools(tools)
            .with_max_tokens(64);
        let body = provider.request_body(&request);
        assert_eq!(body["stream"], json!(true));
        assert_eq!(body["stream_options"]["include_usage"], json!(true));
        assert_eq!(body["max_tokens"], json!(64));
        assert_eq!(body["tool_choice"], json!("auto"));
        assert_eq!(body["tools"][0]["type"], json!("function"));
        assert_eq!(body["tools"][0]["function"]["name"], json!("get_board"));
        assert!(body["tools"][0]["function"]["parameters"].is_object());
    }

    #[test]
    fn lamu_thinking_mode_changes_the_wire_flag_and_budget() {
        let provider = || {
            OpenAiProvider::new(ProviderConfig::openai_compatible(
                "http://localhost:1/v1",
                "mock",
                "m1",
            ))
            .expect("valid config")
        };
        let request = CompletionRequest::new("m1", vec![ChatMessage::user("Your move")]);
        let direct = provider().with_direct_answers(96).request_body(&request);
        assert_eq!(direct["enable_thinking"], json!(false));
        assert_eq!(direct["max_tokens"], json!(96));
        let thinking = provider().with_thinking(2048).request_body(&request);
        assert_eq!(thinking["enable_thinking"], json!(true));
        assert_eq!(thinking["max_tokens"], json!(2048));
    }

    #[test]
    fn provider_request_body_omits_tools_when_none_are_offered() {
        let provider = OpenAiProvider::new(ProviderConfig::openai_compatible(
            "http://localhost:1/v1",
            "mock",
            "m1",
        ))
        .expect("valid config");
        let body =
            provider.request_body(&CompletionRequest::new("m1", vec![ChatMessage::user("hi")]));
        assert!(body.get("tools").is_none());
        assert!(body.get("tool_choice").is_none());
        assert!(body.get("max_tokens").is_none());
    }
}
