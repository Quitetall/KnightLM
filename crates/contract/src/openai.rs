//! OpenAI chat-completions wire types, the subset the seam uses.
//!
//! Types that are ours (the request a client sends to a KnightLM server and the `knightlm`
//! extension) refuse unknown fields. Responses from a server we do not own may carry extra
//! fields (`system_fingerprint`, `service_tier`, ...), so [`ChatCompletion`] and its parts
//! ignore them.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `POST /v1/chat/completions` request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<RequestMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolSpec>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
    /// KnightLM extension request field: `{"mode": "raw" | "constrained"}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knightlm: Option<KnightlmRequestExt>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestMessage {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolSpec {
    #[serde(rename = "type")]
    pub kind: String,
    pub function: ToolFunctionSpec,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolFunctionSpec {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamOptions {
    pub include_usage: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnightlmRequestExt {
    pub mode: String,
}

/// `chat.completion` response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatCompletion {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<Choice>,
    #[serde(default)]
    pub usage: Option<Usage>,
    /// Present when the server is KnightLM: how the answer was produced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knightlm: Option<KnightlmExt>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    pub index: u32,
    pub message: ResponseMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseMessage {
    pub role: String,
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    /// A JSON document in a string, as OpenAI sends it.
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

/// The `knightlm` response extension. `path` is `model`, `constrained_move`,
/// `constrained_forced_reasoning` or `none`; `code` is `ok` or a refusal code; `raw_code` is the
/// code of the model's own completion before any constrained choice; `legal` is false for a refusal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnightlmExt {
    pub path: String,
    pub code: String,
    pub raw_code: String,
    pub legal: bool,
    pub manifest_hash: String,
}

/// The OpenAI error object: `{"error": {...}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: ErrorBody,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub message: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub param: Option<String>,
    #[serde(default)]
    pub code: Option<String>,
}

impl ChatCompletion {
    /// The `submit_move` arguments `(reasoning, move)` of the first tool call, if there is one.
    pub fn submit_move(&self) -> Option<(String, String)> {
        let call = self.choices.first()?.message.tool_calls.as_ref()?.first()?;
        if call.function.name != "submit_move" {
            return None;
        }
        let args: Value = serde_json::from_str(&call.function.arguments).ok()?;
        Some((
            args.get("reasoning")?.as_str()?.to_string(),
            args.get("move")?.as_str()?.to_string(),
        ))
    }
}
