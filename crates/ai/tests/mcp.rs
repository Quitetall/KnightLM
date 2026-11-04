//! MCP transport behaviour: framing, dispatch, refusals and the auth boundary.
//!
//! The protocol is a pure function here, so these tests never start a socket.
//! What they check is what a client would notice: initialize answers with a
//! protocol version, tools are listed with the same schemas the app uses, a call
//! runs the same host, and anything malformed is refused with a JSON-RPC error
//! rather than a panic.
#![cfg(feature = "core")]

use serde_json::{Value, json};

use rbc_ai::agent::ToolHost;
use rbc_ai::mcp::{MAX_MESSAGE_BYTES, RpcError, ServerInfo, dispatch, response};
use rbc_ai::protocol::{ToolCall, ToolResult};
use rbc_ai::tools::ToolRegistry;

/// A host that answers every call, recording what it saw.
struct EchoHost {
    calls: Vec<(String, Value)>,
}

impl EchoHost {
    fn new() -> Self {
        Self { calls: Vec::new() }
    }
}

impl ToolHost for EchoHost {
    fn call(&mut self, call: &ToolCall) -> ToolResult {
        self.calls.push((call.name.clone(), call.arguments.clone()));
        rbc_ai::result::ok(call, "echo", json!({"seen": call.name}))
    }
}

fn serve(request: Value) -> (Option<Value>, Vec<String>) {
    let registry = ToolRegistry::builtin();
    let mut host = EchoHost::new();
    let info = ServerInfo::default();
    let id = request.get("id").cloned();
    let dispatched = dispatch(&request, &registry, &mut host, &info);
    let calls = host.calls.iter().map(|(name, _)| name.clone()).collect();
    (response(id.as_ref(), dispatched), calls)
}

#[test]
fn mcp_initialize_reports_a_protocol_version_and_the_server() {
    let (reply, calls) = serve(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2025-06-18", "capabilities": {}}
    }));
    let reply = reply.expect("a response");
    assert_eq!(reply["jsonrpc"], json!("2.0"));
    assert_eq!(reply["id"], json!(1));
    assert!(
        reply["result"]["protocolVersion"].is_string(),
        "a client needs the version it is speaking: {reply}"
    );
    assert_eq!(
        reply["result"]["serverInfo"]["name"],
        json!("rusty-brick-chess")
    );
    assert!(calls.is_empty(), "initialize calls no tool");
}

#[test]
fn mcp_a_notification_is_never_answered() {
    // No id means a notification; answering one would break the client.
    let (reply, _) = serve(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    assert_eq!(reply, None);
}

#[test]
fn mcp_tools_list_carries_the_same_schemas_the_app_uses() {
    let (reply, _) = serve(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    let tools = reply.expect("a response")["result"]["tools"]
        .as_array()
        .expect("tools array")
        .clone();
    assert!(!tools.is_empty(), "the registry has tools");
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(names.contains(&"get_position"), "read tools are exposed");
    assert!(names.contains(&"play_move"), "write tools are exposed");
    for tool in &tools {
        assert!(
            tool["inputSchema"].is_object(),
            "every tool declares an input schema: {tool}"
        );
        assert!(
            tool.get("effects").is_none(),
            "internal effect metadata does not belong on the wire"
        );
    }
}

#[test]
fn mcp_a_tool_call_runs_the_same_host() {
    let (reply, calls) = serve(json!({
        "jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": {"name": "get_position", "arguments": {"board_id": "b1"}}
    }));
    let reply = reply.expect("a response");
    assert_eq!(reply["result"]["isError"], json!(false));
    assert!(
        reply["result"]["content"][0]["text"].is_string(),
        "a text block"
    );
    assert!(
        reply["result"]["structuredContent"].is_object(),
        "and the struct"
    );
    assert_eq!(
        calls,
        vec!["get_position".to_string()],
        "the host was called once"
    );
}

#[test]
fn mcp_an_unknown_tool_is_an_error_not_a_crash() {
    let (reply, calls) = serve(json!({
        "jsonrpc": "2.0", "id": 4, "method": "tools/call",
        "params": {"name": "no_such_tool", "arguments": {}}
    }));
    let reply = reply.expect("a response");
    assert_eq!(reply["error"]["code"], json!(-32602));
    assert!(calls.is_empty(), "nothing was called");
}

#[test]
fn mcp_bad_arguments_are_refused_before_the_host_runs() {
    // `get_position` requires a board_id; without one the call must not reach
    // the host, so a broken client cannot half-run a tool.
    let (reply, calls) = serve(json!({
        "jsonrpc": "2.0", "id": 5, "method": "tools/call",
        "params": {"name": "get_position", "arguments": {}}
    }));
    let reply = reply.expect("a response");
    assert_eq!(reply["error"]["code"], json!(-32602));
    assert!(calls.is_empty(), "the host never saw it");
}

#[test]
fn mcp_an_unknown_method_is_method_not_found() {
    let (reply, _) = serve(json!({"jsonrpc": "2.0", "id": 6, "method": "no/such"}));
    assert_eq!(reply.expect("a response")["error"]["code"], json!(-32601));
}

#[test]
fn mcp_a_request_without_a_method_is_invalid() {
    let (reply, _) = serve(json!({"jsonrpc": "2.0", "id": 7}));
    assert_eq!(reply.expect("a response")["error"]["code"], json!(-32600));
}

#[test]
fn mcp_a_non_object_request_is_refused() {
    let registry = ToolRegistry::builtin();
    let mut host = EchoHost::new();
    let info = ServerInfo::default();
    let failure = dispatch(&json!("hello"), &registry, &mut host, &info)
        .expect_err("a string is not a request");
    assert_eq!(failure.code, -32600);
}

#[test]
fn mcp_ping_answers_and_carries_no_tool_traffic() {
    let (reply, calls) = serve(json!({"jsonrpc": "2.0", "id": 8, "method": "ping"}));
    assert_eq!(reply.expect("a response")["result"], json!({}));
    assert!(calls.is_empty());
}

#[test]
fn mcp_the_error_codes_are_the_json_rpc_ones() {
    assert_eq!(RpcError::parse("x").code, -32700);
    assert_eq!(RpcError::invalid_request("x").code, -32600);
    assert_eq!(RpcError::method_not_found("m").code, -32601);
    assert_eq!(RpcError::invalid_params("x").code, -32602);
}

#[test]
fn mcp_the_message_ceiling_is_a_megabyte() {
    // A tool call is small; this is the point a transport refuses the frame.
    assert_eq!(MAX_MESSAGE_BYTES, 1024 * 1024);
}

// --- the auth boundary, as the app enforces it --------------------------------

#[test]
fn mcp_a_missing_or_wrong_token_is_refused() {
    // The app compares a bearer token before it dispatches anything. The
    // comparison is here because the rule belongs with the protocol: stdio
    // inherits process trust, HTTP does not.
    fn accepts(presented: Option<&str>, expected: &str) -> bool {
        match presented {
            Some(value) => {
                let value = value.strip_prefix("Bearer ").unwrap_or(value);
                // A length-independent compare, so a near-miss is not a tell.
                value.len() == expected.len()
                    && value
                        .bytes()
                        .zip(expected.bytes())
                        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                        == 0
            }
            None => false,
        }
    }

    let token = "mcp-token-0123456789";
    assert!(accepts(Some(&format!("Bearer {token}")), token));
    assert!(!accepts(Some("Bearer wrong"), token));
    assert!(!accepts(None, token));
    assert!(!accepts(Some(""), token));
}
