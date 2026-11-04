//! One way to build a [`ToolResult`].
//!
//! Every host — board writes, read tools, and the server's own adapters — builds
//! results here, so a tool card renders the same fields whatever produced it:
//! a stem, a code from [`crate::protocol::result_code`], a short summary and,
//! when the tool wrote something, the new revision.

use serde_json::Value;

use crate::protocol::{ErrorInfo, ToolCall, ToolResult, result_code};

/// A call that did what was asked.
pub fn ok(call: &ToolCall, summary: impl Into<String>, data: Value) -> ToolResult {
    ToolResult {
        call_id: call.id.clone(),
        tool: call.name.clone(),
        ok: true,
        code: result_code::APPLIED.to_string(),
        summary: summary.into(),
        data: (!data.is_null()).then_some(data),
        revision: None,
        audit_id: None,
        error: None,
    }
}

/// A call that wrote: `ok`, carrying the new revision.
pub fn applied(
    call: &ToolCall,
    revision: u64,
    summary: impl Into<String>,
    data: Value,
) -> ToolResult {
    let mut result = ok(call, summary, data);
    result.revision = Some(revision);
    result
}

/// A call that refused, for a reason the model can read and act on.
pub fn refused(call: &ToolCall, code: &str, summary: impl Into<String>) -> ToolResult {
    refused_with(call, code, summary, Value::Null)
}

/// A refusal that carries data, for example the revision that caused a
/// conflict.
pub fn refused_with(
    call: &ToolCall,
    code: &str,
    summary: impl Into<String>,
    data: Value,
) -> ToolResult {
    ToolResult {
        call_id: call.id.clone(),
        tool: call.name.clone(),
        ok: false,
        code: code.to_string(),
        summary: summary.into(),
        data: (!data.is_null()).then_some(data),
        revision: None,
        audit_id: None,
        error: None,
    }
}

/// A failure the tool could not describe any other way.
pub fn internal(call: &ToolCall, error: ErrorInfo) -> ToolResult {
    refused_with(call, &error.code, error.message.clone(), Value::Null).with_error(error)
}

/// A service this build cannot reach, with what would make it available.
pub fn unavailable(call: &ToolCall, what: &str) -> ToolResult {
    refused(
        call,
        result_code::UNAVAILABLE,
        format!("{what} is not configured on this server"),
    )
}

impl ToolResult {
    /// Attach the detail a failure carries into the transcript.
    fn with_error(mut self, error: ErrorInfo) -> Self {
        self.error = Some(error);
        self
    }
}
