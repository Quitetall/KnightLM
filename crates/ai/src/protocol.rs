//! Protocol DTOs shared by the server, the frontend and MCP adapters.
//!
//! These types are deliberately storage- and transport-agnostic. Identifiers are
//! opaque strings (the host stores UUIDs) and timestamps are Unix milliseconds.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use crate::policy::{Origin, PermissionProfile, PermissionScope};

/// The interaction mode of an assistant session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    /// Free-form analysis and authoring. Writes go through the approval policy.
    #[default]
    Chat,
    /// The assistant plays one side of a game.
    Play,
    /// The assistant drives both sides while the user observes.
    Watch,
}

impl SessionMode {
    /// Stable wire/database spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Play => "play",
            Self::Watch => "watch",
        }
    }
}

impl std::fmt::Display for SessionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Who produced a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnRole {
    User,
    Assistant,
    Tool,
    System,
}

impl TurnRole {
    /// Stable wire/database spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
            Self::System => "system",
        }
    }
}

impl std::fmt::Display for TurnRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A provider-qualified model identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelRef {
    /// Provider id, for example `openai` or `ollama`.
    pub provider: String,
    /// Provider-side model id, for example `gpt-4.1`.
    pub model: String,
    /// Optional human-readable label shown in the UI.
    #[serde(default)]
    pub display_name: Option<String>,
}

impl ModelRef {
    /// Build a reference without a display label.
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            display_name: None,
        }
    }

    /// The `provider/model` key used by ratings and logs.
    pub fn qualified_id(&self) -> String {
        format!("{}/{}", self.provider, self.model)
    }
}

/// The provider protocol spoken by an endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderKind {
    /// OpenAI-compatible chat completions (the default and first implementation).
    #[serde(rename = "openai_compatible")]
    OpenAiCompatible,
    Anthropic,
    Google,
    Ollama,
    Custom,
}

impl ProviderKind {
    /// Stable wire/database spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "openai_compatible",
            Self::Anthropic => "anthropic",
            Self::Google => "google",
            Self::Ollama => "ollama",
            Self::Custom => "custom",
        }
    }
}

impl std::fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A configured provider endpoint.
///
/// Secrets never live in this struct: `key_ref` points at an OS-keyring entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    /// Base URL including any API prefix, for example `http://localhost:11434/v1`.
    pub base_url: String,
    /// Reference to the keyring entry, never the key itself.
    #[serde(default)]
    pub key_ref: Option<String>,
    /// Model ids advertised by the provider, if known.
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

/// A stable reference to a context specification and template revision.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContextSpecRef {
    /// Specification id, for example `chat-v1`.
    pub id: String,
    /// Specification revision, recorded on every turn.
    pub version: String,
    /// Optional prompt-template revision.
    #[serde(default)]
    pub template_version: Option<String>,
}

impl ContextSpecRef {
    /// Build a reference without a template revision.
    pub fn new(id: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
            template_version: None,
        }
    }
}

/// Token, latency and cost accounting for one turn.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Usage {
    #[serde(default)]
    pub tokens_in: u32,
    #[serde(default)]
    pub tokens_out: u32,
    #[serde(default)]
    pub latency_ms: u64,
    /// Estimated USD cost, when the model has pricing metadata.
    #[serde(default)]
    pub cost_estimate: Option<f64>,
}

/// A normalized error, suitable for both provider and tool failures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorInfo {
    /// Stable machine-readable code, for example `provider_timeout`.
    pub code: String,
    /// Human-readable message. Secrets must already be redacted.
    pub message: String,
    /// Whether retrying the same operation may succeed.
    #[serde(default)]
    pub retryable: bool,
    /// Optional provider-specific error code.
    #[serde(default)]
    pub provider_code: Option<String>,
}

/// The historical name for a turn-level error.
pub type TurnError = ErrorInfo;

/// A tool invocation proposed by the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Provider call id, echoed by the matching [`ToolResult`].
    pub id: String,
    /// Registered tool name.
    pub name: String,
    /// Arguments as JSON. Validation happens in [`crate::tools`].
    pub arguments: Value,
}

impl ToolCall {
    /// Build a tool call with JSON arguments.
    pub fn new(id: impl Into<String>, name: impl Into<String>, arguments: Value) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            arguments,
        }
    }
}

/// The canonical outcome of a tool invocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    /// The [`ToolCall::id`] this result answers.
    pub call_id: String,
    /// The tool name, repeated for observable transcripts.
    pub tool: String,
    /// Whether the tool did what was asked.
    pub ok: bool,
    /// Stable result code, see [`result_code`].
    pub code: String,
    /// Short summary shown in a tool card.
    pub summary: String,
    /// Structured payload; shape depends on the tool.
    #[serde(default)]
    pub data: Option<Value>,
    /// The new board/study revision when the call wrote content.
    #[serde(default)]
    pub revision: Option<u64>,
    /// Audit row id when the call was recorded.
    #[serde(default)]
    pub audit_id: Option<String>,
    /// Failure details when `ok` is false.
    #[serde(default)]
    pub error: Option<ErrorInfo>,
}

/// Result codes shared by tools and the event stream.
pub mod result_code {
    pub const APPLIED: &str = "applied";
    pub const APPROVAL_REQUIRED: &str = "approval_required";
    pub const ILLEGAL: &str = "illegal";
    pub const CONFLICT: &str = "conflict";
    pub const COPY_REQUIRED: &str = "copy_required";
    pub const DENIED: &str = "denied";
    pub const NOT_FOUND: &str = "not_found";
    pub const INVALID_ARGUMENTS: &str = "invalid_arguments";
    pub const INTERNAL: &str = "internal";
    /// The tool exists but the service behind it is not configured here.
    pub const UNAVAILABLE: &str = "unavailable";
}

/// A named assistant conversation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiSession {
    pub id: String,
    pub title: String,
    pub mode: SessionMode,
    /// The board the session started from, when it has one.
    #[serde(default)]
    pub home_board_id: Option<String>,
    /// The board tools currently target by default.
    #[serde(default)]
    pub target_board_id: Option<String>,
    pub model: ModelRef,
    pub context_spec: ContextSpecRef,
    #[serde(default)]
    pub permissions: PermissionProfile,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub archived: bool,
}

/// One append-only entry in a session transcript.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiTurn {
    pub id: String,
    pub session_id: String,
    /// Monotonic sequence number within the session.
    pub seq: u64,
    pub role: TurnRole,
    pub content: String,
    /// Optional model reasoning, kept separately from user-visible content.
    #[serde(default)]
    pub reasoning: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default)]
    pub tool_results: Vec<ToolResult>,
    /// The exact compiled context JSON sent to the provider.
    #[serde(default)]
    pub context_json: String,
    /// SHA-256 of `context_json`, used for replay comparison.
    #[serde(default)]
    pub context_sha256: String,
    #[serde(default)]
    pub spec_version: String,
    #[serde(default)]
    pub template_version: String,
    #[serde(default)]
    pub tool_schema_version: String,
    /// Engine binary/build fingerprint when analysis contributed to context.
    #[serde(default)]
    pub engine_fingerprint: Option<String>,
    pub model: ModelRef,
    #[serde(default)]
    pub usage: Usage,
    /// Terminal turn error, if any.
    #[serde(default)]
    pub error: Option<ErrorInfo>,
    #[serde(default)]
    pub created_at: i64,
}

/// Lifecycle of an approval job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    #[default]
    Pending,
    Approved,
    Denied,
    Expired,
}

/// A request that a person approve a tool's changeset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: String,
    /// Whether the request came from the app UI or an MCP connection.
    pub origin: Origin,
    #[serde(default)]
    pub session_id: Option<String>,
    /// The kind of object being changed, for example `board` or `study`.
    #[serde(default)]
    pub target_kind: Option<String>,
    #[serde(default)]
    pub target_id: Option<String>,
    /// The action or tool being approved.
    pub action: String,
    pub tool: String,
    /// One-line description shown in the approval queue.
    pub changeset_summary: String,
    /// Structured diff. Its shape is owned by the approval service.
    #[serde(default)]
    pub changeset: Option<Value>,
    #[serde(default)]
    pub status: ApprovalStatus,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub resolved_at: Option<i64>,
    /// When an untouched request stops being actionable, in Unix milliseconds.
    #[serde(default)]
    pub expires_at: Option<i64>,
}

/// Why a turn stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    ToolCalls,
    MaxTokens,
    Cancelled,
    Error,
}

/// A normalized, replayable streaming event payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AiEventPayload {
    TurnStarted {
        turn_id: String,
        seq: u64,
    },
    ContextReady {
        sha256: String,
        tokens: u32,
        spec_version: String,
    },
    ReasoningDelta {
        text: String,
    },
    ContentDelta {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: Value,
    },
    ToolResult {
        id: String,
        ok: bool,
        code: String,
        summary: String,
    },
    BoardAction {
        board_id: String,
        revision: u64,
        action: String,
    },
    ApprovalRequired {
        approval_id: String,
        changeset_summary: String,
    },
    Usage {
        #[serde(flatten)]
        usage: Usage,
    },
    TurnDone {
        turn_id: String,
        stop_reason: StopReason,
    },
    Error {
        #[serde(flatten)]
        error: ErrorInfo,
    },
}

/// An event on a session or match stream.
///
/// `event_id` increases monotonically per stream; clients reconnect with the
/// last id they saw. The payload is flattened so the wire shape is
/// `{"event_id":1,"type":"content_delta","text":"..."}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiEvent {
    pub event_id: u64,
    #[serde(flatten)]
    pub payload: AiEventPayload,
}

impl AiEvent {
    /// Build an event with the next stream id.
    pub fn new(event_id: u64, payload: AiEventPayload) -> Self {
        Self { event_id, payload }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn model() -> ModelRef {
        ModelRef::new("openai", "gpt-4.1")
    }

    #[test]
    fn session_round_trips() {
        let session = AiSession {
            id: "s1".into(),
            title: "Analysis".into(),
            mode: SessionMode::Chat,
            home_board_id: Some("b1".into()),
            target_board_id: Some("b2".into()),
            model: model(),
            context_spec: ContextSpecRef::new("chat-v1", "1"),
            permissions: PermissionProfile::default(),
            created_at: 10,
            updated_at: 20,
            archived: false,
        };
        let encoded = serde_json::to_string(&session).unwrap();
        let decoded: AiSession = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, session);
        assert!(encoded.contains("\"mode\":\"chat\""));
    }

    #[test]
    fn turn_round_trips() {
        let turn = AiTurn {
            id: "t1".into(),
            session_id: "s1".into(),
            seq: 1,
            role: TurnRole::Assistant,
            content: "Nf3".into(),
            reasoning: Some("developing".into()),
            tool_calls: vec![ToolCall::new("c1", "play_move", json!({"move": "Nf3"}))],
            tool_results: vec![ToolResult {
                call_id: "c1".into(),
                tool: "play_move".into(),
                ok: true,
                code: result_code::APPLIED.into(),
                summary: "played Nf3".into(),
                data: None,
                revision: Some(2),
                audit_id: Some("a1".into()),
                error: None,
            }],
            context_json: "{}".into(),
            context_sha256: "abc".into(),
            spec_version: "1".into(),
            template_version: "1".into(),
            tool_schema_version: "1.0.0".into(),
            engine_fingerprint: None,
            model: model(),
            usage: Usage {
                tokens_in: 12,
                tokens_out: 4,
                latency_ms: 30,
                cost_estimate: Some(0.001),
            },
            error: None,
            created_at: 30,
        };
        let encoded = serde_json::to_string(&turn).unwrap();
        let decoded: AiTurn = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, turn);
        assert_eq!(decoded.usage.tokens_in, 12);
    }

    #[test]
    fn event_wire_shape_and_round_trip() {
        let event = AiEvent::new(
            7,
            AiEventPayload::ContentDelta {
                text: "hello".into(),
            },
        );
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["event_id"], 7);
        assert_eq!(value["type"], "content_delta");
        assert_eq!(value["text"], "hello");

        let decoded: AiEvent = serde_json::from_value(value).unwrap();
        assert_eq!(decoded, event);
    }

    #[test]
    fn error_and_usage_events_round_trip() {
        let usage = AiEvent::new(
            1,
            AiEventPayload::Usage {
                usage: Usage {
                    tokens_in: 3,
                    tokens_out: 5,
                    latency_ms: 9,
                    cost_estimate: None,
                },
            },
        );
        let error = AiEvent::new(
            2,
            AiEventPayload::Error {
                error: ErrorInfo {
                    code: "provider_timeout".into(),
                    message: "timed out".into(),
                    retryable: true,
                    provider_code: Some("ETIMEDOUT".into()),
                },
            },
        );
        for event in [usage, error] {
            let value = serde_json::to_value(&event).unwrap();
            assert_eq!(
                value["type"],
                if event.event_id == 1 {
                    "usage"
                } else {
                    "error"
                }
            );
            let encoded = serde_json::to_string(&event).unwrap();
            let decoded: AiEvent = serde_json::from_str(&encoded).unwrap();
            assert_eq!(decoded, event);
        }
        let usage_event = AiEvent::new(
            3,
            AiEventPayload::Usage {
                usage: Usage {
                    tokens_in: 3,
                    tokens_out: 5,
                    latency_ms: 9,
                    cost_estimate: None,
                },
            },
        );
        let value = serde_json::to_value(&usage_event).unwrap();
        assert_eq!(value["tokens_in"], 3);
        assert!(value.get("usage").is_none());
    }

    #[test]
    fn provider_and_approval_serialize_stably() {
        assert_eq!(
            serde_json::to_value(ProviderKind::OpenAiCompatible).unwrap(),
            json!("openai_compatible")
        );
        let approval = ApprovalRequest {
            id: "ap1".into(),
            origin: Origin::InApp,
            session_id: Some("s1".into()),
            target_kind: Some("board".into()),
            target_id: Some("b1".into()),
            action: "play_move".into(),
            tool: "play_move".into(),
            changeset_summary: "play Nf3".into(),
            changeset: None,
            status: ApprovalStatus::Pending,
            created_at: 1,
            resolved_at: None,
            expires_at: Some(900_001),
        };
        let value = serde_json::to_value(&approval).unwrap();
        assert_eq!(value["origin"], "in_app");
        assert_eq!(value["status"], "pending");
    }
}
