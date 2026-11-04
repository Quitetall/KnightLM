//! Provider adapters.
//!
//! One trait ([`Provider`]), one normalized event stream ([`ProviderEvent`])
//! and one error shape ([`ErrorInfo`]). Everything a provider returns is
//! expressed in this crate's own vocabulary, so the agent loop never has to
//! know which vendor it is talking to.
//!
//! The flow for one request:
//!
//! 1. the caller builds a [`CompletionRequest`] from the compiled context and
//!    the tool registry;
//! 2. the provider streams [`ProviderEvent`]s into a sink as they arrive;
//! 3. the provider also returns the assembled [`CompletionResponse`], so a
//!    caller that only wants the final text can ignore the sink.
//!
//! Providers are synchronous and own no thread. The host runs them on a
//! blocking worker (see `ticket` t-7e92) and cancels through [`Cancellation`].
//!
//! Errors are always [`ErrorInfo`] with a stable code from [`error_code`], so
//! the UI can decide between "retry", "fix your key" and "the model is down"
//! without string matching. Secrets never appear in an error message: request
//! headers and the API key are never echoed.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::protocol::{ErrorInfo, ModelRef, ProviderKind, StopReason, ToolCall, TurnRole, Usage};

pub mod openai;
pub mod sse;

pub use openai::OpenAiProvider;
pub use sse::{SseEvent, SseParser};

/// Stable error codes returned by providers.
pub mod error_code {
    /// The endpoint did not answer in time.
    pub const TIMEOUT: &str = "provider_timeout";
    /// The provider asked us to slow down (HTTP 429).
    pub const RATE_LIMITED: &str = "provider_rate_limited";
    /// The key is missing, wrong or not allowed to use this model (401/403).
    pub const AUTH: &str = "provider_auth";
    /// The request was rejected as malformed (400/422).
    pub const BAD_REQUEST: &str = "provider_bad_request";
    /// The context is longer than the model accepts.
    pub const CONTEXT_TOO_LONG: &str = "provider_context_too_long";
    /// The provider failed on its side (5xx).
    pub const SERVER: &str = "provider_server_error";
    /// The socket failed, the host is unknown or the connection was refused.
    pub const NETWORK: &str = "provider_network";
    /// The bytes came back but did not follow the protocol.
    pub const PROTOCOL: &str = "provider_protocol";
    /// A person or the host stopped the turn.
    pub const CANCELLED: &str = "provider_cancelled";
    /// The provider is configured incompletely (no endpoint, bad URL).
    pub const NOT_CONFIGURED: &str = "provider_not_configured";
    /// The response was larger than the accepted bound.
    pub const TOO_LARGE: &str = "provider_too_large";
    /// The provider speaks a protocol this build does not implement.
    pub const UNSUPPORTED: &str = "provider_unsupported";
}

/// A cancellation signal shared by the request that started the turn and the
/// worker running it.
///
/// Both sides hold a clone. The provider checks it before every network call
/// and between streamed chunks, so cancel lands within one read rather than
/// after the whole response.
#[derive(Debug, Clone, Default)]
pub struct Cancellation {
    flag: Arc<AtomicBool>,
}

impl Cancellation {
    /// A fresh, live signal.
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask the running turn to stop. Safe to call many times.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    /// Whether [`Cancellation::cancel`] has been called.
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    /// The cancelled error, or `Ok(())` while the turn may continue.
    pub fn check(&self) -> Result<(), ErrorInfo> {
        if self.is_cancelled() {
            Err(cancelled())
        } else {
            Ok(())
        }
    }
}

/// The error returned when a turn is cancelled.
pub fn cancelled() -> ErrorInfo {
    ErrorInfo {
        code: error_code::CANCELLED.to_string(),
        message: "the turn was cancelled".to_string(),
        retryable: false,
        provider_code: None,
    }
}

/// How many times to retry a failed request, and how long to wait in between.
///
/// Only failures that happened *before any content was emitted* are retried:
/// replaying a half-streamed turn would duplicate text the user already saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Total attempts including the first. `1` means "never retry".
    pub max_attempts: u32,
    /// Wait before the second attempt; doubled for each further attempt.
    pub base_delay: Duration,
    /// Ceiling for the computed wait, and for a provider's `Retry-After`.
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(200),
            max_delay: Duration::from_secs(5),
        }
    }
}

impl RetryPolicy {
    /// One attempt, no retry.
    pub const fn none() -> Self {
        Self {
            max_attempts: 1,
            base_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
        }
    }

    /// Whether `attempts` failed attempts already use up the allowance.
    pub const fn is_exhausted(&self, attempts: u32) -> bool {
        attempts >= self.max_attempts
    }

    /// The wait after `attempts` failed attempts.
    ///
    /// A provider's `Retry-After` wins over the exponential backoff, capped by
    /// [`RetryPolicy::max_delay`] so a hostile header cannot stall the turn.
    pub fn delay(&self, attempts: u32, retry_after: Option<Duration>) -> Duration {
        if let Some(wait) = retry_after {
            return wait.min(self.max_delay);
        }
        let exponent = attempts.saturating_sub(1).min(16);
        let scaled = self
            .base_delay
            .saturating_mul(1u32 << exponent)
            .min(self.max_delay);
        scaled
    }
}

/// A message in the provider conversation.
///
/// This mirrors the OpenAI chat shape while staying transport-neutral: the
/// assistant turn that requested tools keeps [`ChatMessage::tool_calls`], and
/// each answer to it is a [`TurnRole::Tool`] message carrying
/// [`ChatMessage::tool_call_id`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: TurnRole,
    /// Text content. Absent for an assistant turn that only calls tools.
    #[serde(default)]
    pub content: Option<String>,
    /// Tool calls proposed by this assistant message.
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    /// Which [`ToolCall`] this tool message answers.
    #[serde(default)]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    /// A system instruction.
    pub fn system(content: impl Into<String>) -> Self {
        Self::text(TurnRole::System, content)
    }

    /// A user turn.
    pub fn user(content: impl Into<String>) -> Self {
        Self::text(TurnRole::User, content)
    }

    /// An assistant turn with text and no tool calls.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::text(TurnRole::Assistant, content)
    }

    /// An assistant turn that asked for tools.
    pub fn assistant_tools(content: Option<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: TurnRole::Assistant,
            content,
            tool_calls,
            tool_call_id: None,
        }
    }

    /// The result of one tool call, keyed by the call id the model chose.
    pub fn tool_result(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: TurnRole::Tool,
            content: Some(content.into()),
            tool_calls: Vec::new(),
            tool_call_id: Some(call_id.into()),
        }
    }

    fn text(role: TurnRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: Some(content.into()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }
}

/// One tool offered to the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema object, as held by [`crate::tools::ToolSpec`].
    pub parameters: serde_json::Value,
}

impl From<&crate::tools::ToolSpec> for ToolDefinition {
    fn from(spec: &crate::tools::ToolSpec) -> Self {
        Self {
            name: spec.name.clone(),
            description: spec.description.clone(),
            parameters: spec.parameters.clone(),
        }
    }
}

/// One request to a model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    /// Tools the model may call. Empty means "answer in prose only".
    #[serde(default)]
    pub tools: Vec<ToolDefinition>,
    #[serde(default)]
    pub temperature: f32,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    /// Ask for a streamed response. A provider may still answer in one piece.
    #[serde(default)]
    pub stream: bool,
}

impl CompletionRequest {
    /// A text-only request.
    pub fn new(model: impl Into<String>, messages: Vec<ChatMessage>) -> Self {
        Self {
            model: model.into(),
            messages,
            tools: Vec::new(),
            temperature: 0.2,
            max_tokens: None,
            stream: true,
        }
    }

    /// Offer the model a set of tools.
    pub fn with_tools(mut self, tools: impl IntoIterator<Item = ToolDefinition>) -> Self {
        self.tools = tools.into_iter().collect();
        self
    }

    /// Set the sampling temperature.
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    /// Ask for at most this many completion tokens.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Turn streaming on or off.
    pub fn with_stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }
}

/// One normalized streaming event.
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderEvent {
    /// Visible text, appended in order.
    ContentDelta(String),
    /// Model reasoning, kept out of the visible answer.
    ReasoningDelta(String),
    /// A fragment of a tool call. Fragments share an index and their
    /// `arguments` are concatenated before parsing.
    ToolCallDelta {
        index: usize,
        id: Option<String>,
        name: Option<String>,
        arguments: String,
    },
    /// Accounting reported by the provider, usually once at the end.
    Usage(Usage),
    /// The model is done and says why.
    Stopped(StopReason),
}

/// The assembled result of one request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub content: String,
    #[serde(default)]
    pub reasoning: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default)]
    pub usage: Usage,
    pub stop_reason: StopReason,
    /// The provider's own id for this request, when it reports one.
    #[serde(default)]
    pub provider_request_id: Option<String>,
}

/// A model endpoint this build can talk to.
pub trait Provider: Send + Sync {
    /// Stable name for logs, for example `openai_compatible`.
    fn name(&self) -> &str;

    /// Run one request, streaming deltas into `on_event`.
    ///
    /// The same information is returned assembled, so a caller that only needs
    /// the final answer can pass a sink that drops every event.
    fn complete(
        &self,
        request: &CompletionRequest,
        cancel: &Cancellation,
        on_event: &mut dyn FnMut(ProviderEvent),
    ) -> Result<CompletionResponse, ErrorInfo>;
}

/// Everything needed to reach one model endpoint.
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    /// Full URL of the chat endpoint, for example
    /// `http://127.0.0.1:8100/v1/chat`.
    pub endpoint: String,
    pub model: ModelRef,
    /// Bearer token. Never logged, never echoed in an error.
    pub api_key: Option<String>,
    /// Extra request headers, for gateways that need a project or org id.
    pub extra_headers: Vec<(String, String)>,
    /// Time allowed to establish the connection.
    pub connect_timeout: Duration,
    /// Time allowed between two chunks once the response has started.
    pub idle_timeout: Duration,
    /// Time allowed for the whole request, including the stream.
    pub total_timeout: Duration,
    pub retry: RetryPolicy,
    /// Largest accepted response body.
    pub max_response_bytes: usize,
    /// Default temperature for requests that do not set their own.
    pub temperature: f32,
}

impl ProviderConfig {
    /// A configuration for an OpenAI-compatible endpoint.
    pub fn openai_compatible(
        endpoint: impl Into<String>,
        provider: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let provider = provider.into();
        Self {
            kind: ProviderKind::OpenAiCompatible,
            endpoint: endpoint.into(),
            model: ModelRef::new(provider, model),
            api_key: None,
            extra_headers: Vec::new(),
            connect_timeout: Duration::from_secs(10),
            idle_timeout: Duration::from_secs(60),
            total_timeout: Duration::from_secs(300),
            retry: RetryPolicy::default(),
            max_response_bytes: 16 * 1024 * 1024,
            temperature: 0.2,
        }
    }

    /// Attach the bearer token.
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Add one request header.
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_headers.push((name.into(), value.into()));
        self
    }

    /// Replace the retry policy.
    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// Replace the default temperature.
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    /// Replace the three timeouts at once.
    pub fn with_timeouts(mut self, connect: Duration, idle: Duration, total: Duration) -> Self {
        self.connect_timeout = connect;
        self.idle_timeout = idle;
        self.total_timeout = total;
        self
    }

    /// Check that the configuration can reach anything at all.
    ///
    /// Only the endpoint is checked: a key is optional because a local model
    /// server usually has none.
    pub fn validate(&self) -> Result<(), ErrorInfo> {
        let endpoint = self.endpoint.trim();
        if endpoint.is_empty() {
            return Err(ErrorInfo {
                code: error_code::NOT_CONFIGURED.to_string(),
                message: "no model endpoint is configured".to_string(),
                retryable: false,
                provider_code: None,
            });
        }
        if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
            return Err(ErrorInfo {
                code: error_code::NOT_CONFIGURED.to_string(),
                message: "the model endpoint must be an http:// or https:// URL".to_string(),
                retryable: false,
                provider_code: None,
            });
        }
        if self.model.model.trim().is_empty() {
            return Err(ErrorInfo {
                code: error_code::NOT_CONFIGURED.to_string(),
                message: "no model id is configured".to_string(),
                retryable: false,
                provider_code: None,
            });
        }
        Ok(())
    }
}

/// Build an error with the given code.
pub fn error(code: &str, message: impl Into<String>, retryable: bool) -> ErrorInfo {
    ErrorInfo {
        code: code.to_string(),
        message: message.into(),
        retryable,
        provider_code: None,
    }
}

/// Whether an HTTP status is worth retrying.
///
/// `408`, `425` and `429` are transient by definition; `5xx` is the provider's
/// own failure. Everything else is the request's fault and will fail again.
pub fn is_retryable_status(status: u16) -> bool {
    matches!(status, 408 | 425 | 429 | 500..=599)
}

/// Map an HTTP status to a normalized error.
pub fn error_for_status(status: u16, body: &str) -> ErrorInfo {
    let (code, message) = match status {
        401 | 403 => (
            error_code::AUTH,
            "the model provider rejected the credentials",
        ),
        404 => (
            error_code::BAD_REQUEST,
            "the model endpoint or model id was not found",
        ),
        408 => (error_code::TIMEOUT, "the model provider timed out"),
        413 => (
            error_code::CONTEXT_TOO_LONG,
            "the request is too large for this model",
        ),
        422 => (
            error_code::BAD_REQUEST,
            "the model provider rejected the request",
        ),
        429 => (
            error_code::RATE_LIMITED,
            "the model provider is rate limiting this key",
        ),
        400..=499 => (
            error_code::BAD_REQUEST,
            "the model provider rejected the request",
        ),
        500..=599 => (
            error_code::SERVER,
            "the model provider reported a server error",
        ),
        _ => (
            error_code::PROTOCOL,
            "the model provider gave an unexpected status",
        ),
    };
    let mut info = error(code, message, is_retryable_status(status));
    let snippet = body_snippet(body);
    if !snippet.is_empty() {
        info.message = format!("{}: {}", info.message, snippet);
    }
    // A 429 that stops being a 429 is still a rate limit; keep the status for
    // logs without leaking headers.
    info.provider_code = Some(format!("http_{status}"));
    info
}

/// The part of an error body worth showing: trimmed, single-line and short.
pub fn body_snippet(body: &str) -> String {
    let collapsed: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = collapsed.chars().take(240).collect();
    if collapsed.chars().count() > 240 {
        out.push('…');
    }
    out
}

/// Read a `Retry-After` header in seconds.
pub fn retry_after_seconds(value: Option<&str>) -> Option<Duration> {
    let raw = value?.trim();
    let seconds: u64 = raw.parse().ok()?;
    Some(Duration::from_secs(seconds.min(3600)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_retry_policy_backs_off_and_caps() {
        let policy = RetryPolicy {
            max_attempts: 5,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(350),
        };
        assert_eq!(policy.delay(1, None), Duration::from_millis(100));
        assert_eq!(policy.delay(2, None), Duration::from_millis(200));
        assert_eq!(policy.delay(3, None), Duration::from_millis(350));
        assert_eq!(policy.delay(9, None), Duration::from_millis(350));
        assert!(!policy.is_exhausted(4));
        assert!(policy.is_exhausted(5));
    }

    #[test]
    fn provider_retry_policy_honours_retry_after_with_a_ceiling() {
        let policy = RetryPolicy {
            max_attempts: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
        };
        assert_eq!(
            policy.delay(1, Some(Duration::from_secs(2))),
            Duration::from_secs(2)
        );
        assert_eq!(
            policy.delay(1, Some(Duration::from_secs(120))),
            Duration::from_secs(5)
        );
        assert_eq!(RetryPolicy::none().delay(1, None), Duration::ZERO);
    }

    #[test]
    fn provider_status_mapping_separates_transient_from_permanent() {
        let auth = error_for_status(401, "bad key");
        assert_eq!(auth.code, error_code::AUTH);
        assert!(!auth.retryable);
        assert!(!auth.message.contains("sk-"));

        let limited = error_for_status(429, "");
        assert_eq!(limited.code, error_code::RATE_LIMITED);
        assert!(limited.retryable);

        let down = error_for_status(503, "upstream unavailable");
        assert_eq!(down.code, error_code::SERVER);
        assert!(down.retryable);
        assert!(down.message.contains("upstream unavailable"));

        let too_big = error_for_status(413, "");
        assert_eq!(too_big.code, error_code::CONTEXT_TOO_LONG);
        assert!(!too_big.retryable);

        assert!(is_retryable_status(408));
        assert!(is_retryable_status(425));
        assert!(!is_retryable_status(400));
        assert!(!is_retryable_status(404));
    }

    #[test]
    fn provider_error_body_is_collapsed_and_bounded() {
        let long = "x".repeat(500);
        let snippet = body_snippet(&format!(" line one\n\tline   two {long}"));
        assert!(snippet.starts_with("line one line two"));
        assert!(!snippet.contains('\n'));
        assert!(snippet.chars().count() <= 241);
        assert!(snippet.ends_with('…'));
    }

    #[test]
    fn provider_config_refuses_incomplete_endpoints() {
        let mut config = ProviderConfig::openai_compatible("", "mock", "m");
        assert_eq!(
            config.validate().expect_err("empty endpoint").code,
            error_code::NOT_CONFIGURED
        );

        config.endpoint = "127.0.0.1:8100".to_string();
        assert_eq!(
            config.validate().expect_err("relative endpoint").code,
            error_code::NOT_CONFIGURED
        );

        config.endpoint = "http://127.0.0.1:8100/v1/chat".to_string();
        config.model.model = "  ".to_string();
        assert_eq!(
            config.validate().expect_err("empty model").code,
            error_code::NOT_CONFIGURED
        );

        config.model.model = "knightlm".to_string();
        config.validate().expect("complete configuration");
    }

    #[test]
    fn provider_cancellation_flips_once_for_every_clone() {
        let cancel = Cancellation::new();
        let worker = cancel.clone();
        assert!(cancel.check().is_ok());
        worker.cancel();
        assert!(cancel.is_cancelled());
        assert_eq!(
            cancel.check().expect_err("cancelled").code,
            error_code::CANCELLED
        );
    }

    #[test]
    fn provider_messages_encode_the_tool_round_trip() {
        let call = ToolCall::new("call_1", "get_board", serde_json::json!({"board_id": "b"}));
        let assistant = ChatMessage::assistant_tools(None, vec![call.clone()]);
        assert_eq!(assistant.role, TurnRole::Assistant);
        assert!(assistant.content.is_none());
        assert_eq!(assistant.tool_calls.len(), 1);

        let result = ChatMessage::tool_result("call_1", "ok");
        assert_eq!(result.role, TurnRole::Tool);
        assert_eq!(result.tool_call_id.as_deref(), Some("call_1"));
    }

    #[test]
    fn provider_tool_definition_copies_the_spec_schema() {
        let registry = crate::tools::ToolRegistry::builtin();
        let spec = registry.get("get_board").expect("get_board is built in");
        let definition = ToolDefinition::from(spec);
        assert_eq!(definition.name, "get_board");
        assert_eq!(definition.parameters, spec.parameters);
        assert_eq!(
            definition.parameters.get("additionalProperties"),
            Some(&serde_json::Value::Bool(false))
        );
    }

    #[test]
    fn provider_retry_after_parses_seconds_only() {
        assert_eq!(retry_after_seconds(Some("3")), Some(Duration::from_secs(3)));
        assert_eq!(retry_after_seconds(Some(" 0 ")), Some(Duration::ZERO));
        assert_eq!(
            retry_after_seconds(Some("Wed, 21 Oct 2015 07:28:00 GMT")),
            None
        );
        assert_eq!(retry_after_seconds(None), None);
    }
}
