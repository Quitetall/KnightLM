//! The MCP surface: JSON-RPC over whatever transport carries it.
//!
//! This module is the protocol, deliberately free of stdio and HTTP so it can be
//! tested without either: hand it a request and it hands back a response. The
//! app owns the transports (`rusty-brick-chess mcp` on stdio, `POST /mcp` over
//! HTTP) and the bearer token.
//!
//! Two rules from the design shape everything here:
//!
//! - **One registry, one queue, one audit log across transports.** The same
//!   tools and the same hosts serve MCP and the in-app assistant, so a call
//!   cannot behave differently depending on how it arrived.
//! - **MCP has no session mode.** Its calls are not mode-gated, which is why the
//!   policy treats `Origin::Mcp` separately; and it can reach no AI session, no
//!   provider key and no setting.

use serde_json::{Value, json};

use crate::agent::ToolHost;
use crate::protocol::ToolCall;
use crate::tools::ToolRegistry;

/// The protocol version this server speaks.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// The server's own identity.
#[derive(Debug, Clone)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

impl Default for ServerInfo {
    fn default() -> Self {
        Self {
            name: "rusty-brick-chess".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        }
    }
}

/// A JSON-RPC error, shaped the way MCP expects.
#[derive(Debug, Clone, PartialEq)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

impl RpcError {
    /// The request was not valid JSON-RPC.
    pub fn parse(message: impl Into<String>) -> Self {
        Self {
            code: -32700,
            message: message.into(),
        }
    }

    /// The request was JSON but not a valid request object.
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            code: -32600,
            message: message.into(),
        }
    }

    /// No such method.
    pub fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("unknown method {method}"),
        }
    }

    /// Bad method parameters.
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
        }
    }
}

/// What a dispatch produced: a result, an error, or nothing (a notification).
pub type Dispatched = Result<Option<Value>, RpcError>;

/// Serve one JSON-RPC message.
///
/// Returns `None` for a notification (a request without an `id`), which must not
/// be answered.
pub fn dispatch(
    request: &Value,
    registry: &ToolRegistry,
    host: &mut dyn ToolHost,
    info: &ServerInfo,
) -> Dispatched {
    let Some(object) = request.as_object() else {
        return Err(RpcError::invalid_request("a request must be an object"));
    };
    let Some(method) = object.get("method").and_then(|m| m.as_str()) else {
        return Err(RpcError::invalid_request("a request needs a method"));
    };
    let id = object.get("id").cloned();
    let params = object.get("params").cloned().unwrap_or(json!({}));

    let result = match method {
        "initialize" => Ok(Some(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {
                "tools": {"listChanged": false},
            },
            "serverInfo": {"name": info.name, "version": info.version},
        }))),
        // Notifications carry no result.
        "notifications/initialized" | "notifications/cancelled" => Ok(None),
        "ping" => Ok(Some(json!({}))),
        "tools/list" => Ok(Some(tools_list(registry))),
        "tools/call" => tools_call(&params, registry, host),
        other => Err(RpcError::method_not_found(other)),
    }?;

    // A notification is never answered, whatever it produced.
    if id.is_none() {
        return Ok(None);
    }
    Ok(Some(result.unwrap_or(Value::Null)))
}

/// The response envelope for a dispatched message.
pub fn response(id: Option<&Value>, dispatched: Dispatched) -> Option<Value> {
    let id = id?.clone();
    Some(match dispatched {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        Err(error) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": error.code, "message": error.message},
        }),
    })
}

fn tools_list(registry: &ToolRegistry) -> Value {
    let tools: Vec<Value> = registry
        .iter()
        .map(|spec| {
            json!({
                "name": spec.name,
                "description": spec.description,
                "inputSchema": spec.parameters,
            })
        })
        .collect();
    json!({ "tools": tools })
}

fn tools_call(params: &Value, registry: &ToolRegistry, host: &mut dyn ToolHost) -> Dispatched {
    let Some(name) = params.get("name").and_then(|n| n.as_str()) else {
        return Err(RpcError::invalid_params("tools/call needs a name"));
    };
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let Some(spec) = registry.get(name) else {
        return Err(RpcError::invalid_params(format!("unknown tool {name}")));
    };
    crate::tools::validate_arguments(spec, &arguments)
        .map_err(|error| RpcError::invalid_params(format!("{error}")))?;

    let call = ToolCall::new(format!("mcp-{}", uuid_like()), name, arguments);
    let result = host.call(&call);
    // MCP wants content blocks; the tool result is one text block plus its
    // structured form, so a client can read either.
    let text = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
    Ok(Some(json!({
        "content": [{"type": "text", "text": text}],
        "structuredContent": result,
        "isError": !result.ok,
    })))
}

/// A cheap unique suffix for a call id; the transport does not need a UUID.
fn uuid_like() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    format!("{now}-{}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// The message size a transport should accept.
///
/// A tool call is small; a paste of a game is not unbounded either. Anything
/// larger is refused by the transport before it reaches this module.
pub const MAX_MESSAGE_BYTES: usize = 1 << 20;
