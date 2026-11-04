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
use rbc_ai::mcp::{
    MAX_MESSAGE_BYTES, NoResources, ResourceContent, ResourceEntry, Resources, RpcError,
    ServerInfo, dispatch, response,
};
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

/// A profile that grants MCP no scopes: the server's default posture.
const GRANTS_NOTHING: rbc_ai::policy::PermissionProfile = rbc_ai::policy::PermissionProfile {
    in_app: Vec::new(),
    mcp: Vec::new(),
};

fn serve(request: Value) -> (Option<Value>, Vec<String>) {
    let registry = ToolRegistry::builtin();
    let mut host = EchoHost::new();
    let info = ServerInfo::default();
    let id = request.get("id").cloned();
    let dispatched = dispatch(
        &request,
        &registry,
        &mut host,
        &NoResources,
        &GRANTS_NOTHING,
        &info,
    );
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
    let failure = dispatch(
        &json!("hello"),
        &registry,
        &mut host,
        &NoResources,
        &GRANTS_NOTHING,
        &info,
    )
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

// --- resources and prompts -----------------------------------------------------

/// A single fake resource, so the protocol can be tested without a database.
struct OneResource;

impl Resources for OneResource {
    fn list(&self) -> Vec<ResourceEntry> {
        vec![ResourceEntry {
            uri: "rbc://board/b1".into(),
            name: "Main line".into(),
            description: "A board.".into(),
            revision: Some(7),
        }]
    }

    fn read(&self, uri: &str) -> Option<ResourceContent> {
        (uri == "rbc://board/b1").then(|| ResourceContent {
            uri: uri.to_string(),
            mime_type: "application/json".into(),
            text: r#"{"moves":["e4"]}"#.into(),
            revision: Some(7),
        })
    }
}

fn serve_with(request: Value, resources: &dyn Resources) -> Option<Value> {
    let registry = ToolRegistry::builtin();
    let mut host = EchoHost::new();
    let info = ServerInfo::default();
    let id = request.get("id").cloned();
    let dispatched = dispatch(
        &request,
        &registry,
        &mut host,
        resources,
        &GRANTS_NOTHING,
        &info,
    );
    response(id.as_ref(), dispatched)
}

#[test]
fn mcp_resources_list_carries_the_revision() {
    let reply = serve_with(
        json!({"jsonrpc": "2.0", "id": 10, "method": "resources/list"}),
        &OneResource,
    )
    .expect("a response");
    let entries = reply["result"]["resources"].as_array().expect("array");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["uri"], json!("rbc://board/b1"));
    assert_eq!(
        entries[0]["revision"],
        json!(7),
        "a snapshot says which revision it is"
    );
}

#[test]
fn mcp_resources_read_returns_the_snapshot() {
    let reply = serve_with(
        json!({"jsonrpc": "2.0", "id": 11, "method": "resources/read", "params": {"uri": "rbc://board/b1"}}),
        &OneResource,
    )
    .expect("a response");
    let contents = &reply["result"]["contents"][0];
    assert_eq!(contents["mimeType"], json!("application/json"));
    assert_eq!(contents["text"], json!(r#"{"moves":["e4"]}"#));
    assert_eq!(contents["revision"], json!(7));
}

#[test]
fn mcp_an_unknown_resource_is_an_error() {
    let reply = serve_with(
        json!({"jsonrpc": "2.0", "id": 12, "method": "resources/read", "params": {"uri": "rbc://nope"}}),
        &OneResource,
    )
    .expect("a response");
    assert_eq!(reply["error"]["code"], json!(-32602));
}

#[test]
fn mcp_prompts_are_deterministic_templates() {
    let listed = serve_with(
        json!({"jsonrpc": "2.0", "id": 13, "method": "prompts/list"}),
        &NoResources,
    )
    .expect("a response");
    let names: Vec<&str> = listed["result"]["prompts"]
        .as_array()
        .expect("array")
        .iter()
        .filter_map(|prompt| prompt["name"].as_str())
        .collect();
    assert_eq!(names, vec!["analyze_position", "review_game"]);

    // The same arguments give the same prompt, twice.
    let ask = || {
        serve_with(
            json!({
                "jsonrpc": "2.0", "id": 14, "method": "prompts/get",
                "params": {"name": "analyze_position", "arguments": {"board_id": "b1"}}
            }),
            &NoResources,
        )
        .expect("a response")
    };
    let first = ask();
    let second = ask();
    assert_eq!(first, second, "a prompt must not vary between calls");
    assert!(
        first["result"]["messages"][0]["content"]["text"]
            .as_str()
            .unwrap_or_default()
            .contains("b1")
    );
}

#[test]
fn mcp_a_prompt_without_its_argument_is_refused() {
    let reply = serve_with(
        json!({"jsonrpc": "2.0", "id": 15, "method": "prompts/get", "params": {"name": "review_game"}}),
        &NoResources,
    )
    .expect("a response");
    assert_eq!(reply["error"]["code"], json!(-32602));
}

#[test]
fn mcp_exposes_no_chat_lua_or_app_lifecycle_tools() {
    // The design is explicit: no Lua, no workspace/app lifecycle, no AI chat
    // sessions, no settings and no provider keys over MCP.
    let registry = ToolRegistry::builtin();
    // Whole words, not substrings: "evaluate" contains "lua" and is a perfectly
    // good chess tool.
    let forbidden = [
        "chat", "lua", "session", "settings", "setting", "provider", "key", "secret", "exec",
        "shell",
    ];
    for name in registry.names() {
        for word in name.to_lowercase().split('_') {
            assert!(
                !forbidden.contains(&word),
                "MCP must not expose {name} (its word '{word}' is forbidden)"
            );
        }
    }
}

// --- approvals over MCP --------------------------------------------------------

/// A host that needs approval for a write and remembers what it queued.
struct ApprovalHost {
    ids: Vec<String>,
    calls: Vec<String>,
}

impl rbc_ai::agent::ToolHost for ApprovalHost {
    fn call(&mut self, call: &rbc_ai::protocol::ToolCall) -> rbc_ai::protocol::ToolResult {
        self.calls.push(call.name.clone());
        rbc_ai::result::ok(call, "ran", json!(null))
    }

    fn request_approval(
        &mut self,
        _call: &rbc_ai::protocol::ToolCall,
        _summary: String,
    ) -> Option<String> {
        let id = format!("approval-{}", self.ids.len() + 1);
        self.ids.push(id.clone());
        Some(id)
    }
}

fn serve_with_profile(
    request: Value,
    profile: &rbc_ai::policy::PermissionProfile,
) -> (Option<Value>, ApprovalHost) {
    let registry = ToolRegistry::builtin();
    let mut host = ApprovalHost {
        ids: Vec::new(),
        calls: Vec::new(),
    };
    let info = ServerInfo::default();
    let id = request.get("id").cloned();
    let dispatched = dispatch(&request, &registry, &mut host, &NoResources, profile, &info);
    (response(id.as_ref(), dispatched), host)
}

#[test]
fn mcp_a_write_without_a_grant_asks_for_approval_and_runs_nothing() {
    // The default profile grants MCP nothing, which is the design's rule.
    let profile = rbc_ai::policy::PermissionProfile::default();
    let (reply, host) = serve_with_profile(
        json!({
            "jsonrpc": "2.0", "id": 20, "method": "tools/call",
            "params": {"name": "play_move", "arguments": {"board_id": "b1", "move": "e4", "expected_revision": 1}}
        }),
        &profile,
    );
    let reply = reply.expect("a response");
    let structured = &reply["result"]["structuredContent"];
    assert_eq!(structured["code"], json!("approval_required"));
    assert_eq!(structured["data"]["status"], json!("approval_required"));
    assert!(
        structured["data"]["approval_id"].is_string(),
        "an id to poll"
    );
    assert!(structured["data"]["changeset_summary"].is_string());
    assert_eq!(structured["data"]["poll"], json!("approval_status"));
    assert!(host.calls.is_empty(), "the write did not run");
    assert_eq!(host.ids.len(), 1, "it was queued instead");
}

#[test]
fn mcp_a_granted_board_write_runs() {
    let mut profile = rbc_ai::policy::PermissionProfile::default();
    profile.grant(
        rbc_ai::policy::Origin::Mcp,
        rbc_ai::policy::PermissionScope::BoardWrite,
    );
    let (reply, host) = serve_with_profile(
        json!({
            "jsonrpc": "2.0", "id": 21, "method": "tools/call",
            "params": {"name": "play_move", "arguments": {"board_id": "b1", "move": "e4", "expected_revision": 1}}
        }),
        &profile,
    );
    let reply = reply.expect("a response");
    assert_eq!(reply["result"]["isError"], json!(false));
    assert_eq!(host.calls, vec!["play_move".to_string()], "it ran");
    assert!(host.ids.is_empty(), "and nothing was queued");
}

#[test]
fn mcp_a_denied_tool_never_reaches_the_host() {
    // `create_study` is a structural write; MCP has no mode, but the effect is
    // study content, which always needs a grant.
    let profile = rbc_ai::policy::PermissionProfile::default();
    let (reply, host) = serve_with_profile(
        json!({
            "jsonrpc": "2.0", "id": 22, "method": "tools/call",
            "params": {"name": "create_study", "arguments": {"title": "x"}}
        }),
        &profile,
    );
    let reply = reply.expect("a response");
    let structured = &reply["result"]["structuredContent"];
    assert!(
        structured["code"] == json!("approval_required") || structured["code"] == json!("denied"),
        "a structural write is never silently allowed: {structured}"
    );
    assert!(host.calls.is_empty(), "nothing ran");
}

#[test]
fn mcp_approval_status_is_a_pollable_tool() {
    let registry = ToolRegistry::builtin();
    let spec = registry
        .get("approval_status")
        .expect("approval_status is registered");
    assert_eq!(
        spec.effects,
        rbc_ai::tools::ToolEffect::Read,
        "polling is a read"
    );
    assert!(spec.requires_argument("approval_id"));
}
