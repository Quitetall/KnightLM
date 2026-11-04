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
use crate::policy::{Origin, PermissionProfile, PolicyDecision, decide};
use crate::protocol::{SessionMode, ToolCall, result_code};
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

/// One resource the server offers.
#[derive(Debug, Clone, PartialEq)]
pub struct ResourceEntry {
    /// `rbc://board/<id>` and friends.
    pub uri: String,
    pub name: String,
    pub description: String,
    /// The revision this snapshot was taken at, when the thing has one.
    pub revision: Option<i64>,
}

/// The read-only snapshot side of the surface.
///
/// Kept as a trait so the protocol stays testable without a database. Every
/// implementation must return snapshots: a resource read is a picture of the
/// thing at a revision, not a live handle on it.
pub trait Resources {
    /// Every resource the server exposes.
    fn list(&self) -> Vec<ResourceEntry>;
    /// One resource's content, or `None` when the uri names nothing.
    fn read(&self, uri: &str) -> Option<ResourceContent>;
}

/// A resource's content, shaped the way MCP wants it.
#[derive(Debug, Clone, PartialEq)]
pub struct ResourceContent {
    pub uri: String,
    /// `application/json` for the structured ones, `application/x-chess-pgn`
    /// for a PGN snapshot.
    pub mime_type: String,
    pub text: String,
    pub revision: Option<i64>,
}

/// A server with no resources, for tests that only exercise tools.
pub struct NoResources;

impl Resources for NoResources {
    fn list(&self) -> Vec<ResourceEntry> {
        Vec::new()
    }
    fn read(&self, _uri: &str) -> Option<ResourceContent> {
        None
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
    resources: &dyn Resources,
    profile: &PermissionProfile,
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
                "resources": {"subscribe": false, "listChanged": false},
                "prompts": {"listChanged": false},
            },
            "serverInfo": {"name": info.name, "version": info.version},
        }))),
        // Notifications carry no result.
        "notifications/initialized" | "notifications/cancelled" => Ok(None),
        "ping" => Ok(Some(json!({}))),
        "tools/list" => Ok(Some(tools_list(registry))),
        "tools/call" => tools_call(&params, registry, host, profile),
        "resources/list" => Ok(Some(resources_list(resources))),
        "resources/read" => resources_read(&params, resources),
        "prompts/list" => Ok(Some(json!({"prompts": prompts()}))),
        "prompts/get" => prompts_get(&params),
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

fn tools_call(
    params: &Value,
    registry: &ToolRegistry,
    host: &mut dyn ToolHost,
    profile: &PermissionProfile,
) -> Dispatched {
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

    // The policy is applied here, not by the caller and not by the host: an MCP
    // call is not a privileged path. MCP has no session mode, which the policy
    // already knows, so the mode passed here only ever matters to in-app calls.
    // `decide` takes the one scope a write needs, so ask the profile for each
    // scope the tool's effect maps to and pass along whether any is granted.
    let granted = [
        crate::policy::PermissionScope::BoardWrite,
        crate::policy::PermissionScope::StudyContent,
        crate::policy::PermissionScope::Destructive,
    ]
    .into_iter()
    .find(|scope| profile.grants(Origin::Mcp, *scope));
    let result = match decide(spec, SessionMode::Chat, Origin::Mcp, granted) {
        PolicyDecision::Deny => crate::result::refused(
            &call,
            result_code::DENIED,
            format!("{name} is not available over MCP"),
        ),
        PolicyDecision::RequiresApproval => {
            let summary =
                format!("{name} changes stored content and needs approval before it runs");
            let mut refused =
                crate::result::refused(&call, result_code::APPROVAL_REQUIRED, summary.clone());
            let approval_id = host.request_approval(&call, summary.clone());
            refused.data = Some(match &approval_id {
                Some(id) => serde_json::json!({
                    "status": "approval_required",
                    "approval_id": id,
                    "changeset_summary": summary,
                    "poll": "approval_status",
                }),
                None => serde_json::json!({
                    "status": "approval_required",
                    "changeset_summary": summary,
                }),
            });
            refused
        }
        PolicyDecision::Allow => host.call(&call),
    };
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

fn resources_list(resources: &dyn Resources) -> Value {
    let entries: Vec<Value> = resources
        .list()
        .into_iter()
        .map(|entry| {
            let mut value = json!({
                "uri": entry.uri,
                "name": entry.name,
                "description": entry.description,
                "mimeType": "application/json",
            });
            if let Some(revision) = entry.revision {
                // The revision travels with the resource, so a client can tell
                // one snapshot from the next.
                value["revision"] = json!(revision);
            }
            value
        })
        .collect();
    json!({ "resources": entries })
}

fn resources_read(params: &Value, resources: &dyn Resources) -> Dispatched {
    let Some(uri) = params.get("uri").and_then(|uri| uri.as_str()) else {
        return Err(RpcError::invalid_params("resources/read needs a uri"));
    };
    let Some(content) = resources.read(uri) else {
        return Err(RpcError::invalid_params(format!("no resource {uri}")));
    };
    let mut resource = json!({
        "uri": content.uri,
        "mimeType": content.mime_type,
        "text": content.text,
    });
    if let Some(revision) = content.revision {
        resource["revision"] = json!(revision);
    }
    Ok(Some(json!({ "contents": [resource] })))
}

/// The prompts a client may ask for. Small, fixed templates: a prompt that
/// varied with server state would not be a prompt.
fn prompts() -> Vec<Value> {
    vec![
        json!({
            "name": "analyze_position",
            "description": "Ask for a structured analysis of a board.",
            "arguments": [
                {"name": "board_id", "description": "The board to analyse.", "required": true}
            ],
        }),
        json!({
            "name": "review_game",
            "description": "Ask for the mistakes and themes in a game.",
            "arguments": [
                {"name": "game_id", "description": "The game to review.", "required": true}
            ],
        }),
    ]
}

fn prompts_get(params: &Value) -> Dispatched {
    let Some(name) = params.get("name").and_then(|name| name.as_str()) else {
        return Err(RpcError::invalid_params("prompts/get needs a name"));
    };
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let (id_name, text) = match name {
        "analyze_position" => (
            "board_id",
            "Analyse this position. Give: the material balance, the pawn structure, \
             the three most forcing candidate moves with a line for each, and what \
             each side is trying to do.",
        ),
        "review_game" => (
            "game_id",
            "Review this game. Name the turning points, the mistakes with the better \
             move and why, and the two or three themes worth practising.",
        ),
        other => return Err(RpcError::invalid_params(format!("unknown prompt {other}"))),
    };
    let Some(value) = args.get(id_name).and_then(|v| v.as_str()) else {
        return Err(RpcError::invalid_params(format!(
            "{name} needs a {id_name}"
        )));
    };
    Ok(Some(json!({
        "description": format!("{name} for {value}"),
        "messages": [{
            "role": "user",
            "content": {"type": "text", "text": format!("{text}\n\n({id_name}: {value})")},
        }],
    })))
}
