//! The turn loop, exercised without a network or a database.
//!
//! A scripted provider stands in for the model and a recording host stands in
//! for the board repository, so each behaviour the loop promises — context
//! hash, tool rounds, retries, refusal, approval, cancellation, usage and error
//! persistence — is checked on its own.
#![cfg(all(feature = "provider", feature = "host"))]

use std::collections::VecDeque;
use std::sync::Mutex;

use rbc_ai::agent::{AgentLoop, ToolHost, TurnContext, TurnLimits, TurnPlan, loop_code};
use rbc_ai::policy::{PermissionProfile, PermissionScope};
use rbc_ai::protocol::{
    AiEventPayload, ErrorInfo, ModelRef, SessionMode, StopReason, ToolCall, ToolResult, TurnRole,
    Usage, result_code,
};
use rbc_ai::provider::{
    Cancellation, ChatMessage, CompletionRequest, CompletionResponse, Provider, ProviderEvent,
};
use rbc_ai::tools::ToolRegistry;

// --- a scripted model ------------------------------------------------------

/// A provider that answers from a list and records what it was asked.
struct ScriptedProvider {
    responses: Mutex<VecDeque<Result<CompletionResponse, ErrorInfo>>>,
    requests: Mutex<Vec<CompletionRequest>>,
}

impl ScriptedProvider {
    fn new(responses: Vec<Result<CompletionResponse, ErrorInfo>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<CompletionRequest> {
        self.requests.lock().expect("requests lock").clone()
    }
}

impl Provider for ScriptedProvider {
    fn name(&self) -> &str {
        "scripted"
    }

    fn complete(
        &self,
        request: &CompletionRequest,
        _cancel: &Cancellation,
        on_event: &mut dyn FnMut(ProviderEvent),
    ) -> Result<CompletionResponse, ErrorInfo> {
        self.requests
            .lock()
            .expect("requests lock")
            .push(request.clone());
        let next = self.responses.lock().expect("responses lock").pop_front();
        match next {
            Some(Ok(response)) => {
                if !response.content.is_empty() {
                    on_event(ProviderEvent::ContentDelta(response.content.clone()));
                }
                if let Some(reasoning) = &response.reasoning {
                    on_event(ProviderEvent::ReasoningDelta(reasoning.clone()));
                }
                Ok(response)
            }
            Some(Err(error)) => Err(error),
            None => Err(ErrorInfo {
                code: "scripted_exhausted".into(),
                message: "the script ran out of responses".into(),
                retryable: false,
                provider_code: None,
            }),
        }
    }
}

fn text(tokens_in: u32, tokens_out: u32, content: &str) -> Result<CompletionResponse, ErrorInfo> {
    Ok(CompletionResponse {
        content: content.to_string(),
        reasoning: None,
        tool_calls: Vec::new(),
        usage: Usage {
            tokens_in,
            tokens_out,
            latency_ms: 1,
            cost_estimate: None,
        },
        stop_reason: StopReason::EndTurn,
        provider_request_id: None,
    })
}

fn calls(
    tokens_in: u32,
    tokens_out: u32,
    tool_calls: Vec<ToolCall>,
) -> Result<CompletionResponse, ErrorInfo> {
    Ok(CompletionResponse {
        content: String::new(),
        reasoning: None,
        tool_calls,
        usage: Usage {
            tokens_in,
            tokens_out,
            latency_ms: 1,
            cost_estimate: None,
        },
        stop_reason: StopReason::ToolCalls,
        provider_request_id: None,
    })
}

// --- a recording host ------------------------------------------------------

/// A host that records calls and answers from a queue.
struct FakeHost {
    calls: Vec<ToolCall>,
    queued: VecDeque<ToolResult>,
    /// Runs before a call is answered, so a test can cancel mid-turn.
    before_call: BeforeCall,
}

/// Runs before a call is answered, so a test can cancel mid-turn.
type BeforeCall = Option<Box<dyn FnMut(&ToolCall)>>;

impl FakeHost {
    fn new() -> Self {
        Self {
            calls: Vec::new(),
            queued: VecDeque::new(),
            before_call: None,
        }
    }

    fn cancelling_on_first_call(cancel: Cancellation) -> Self {
        let mut seen = false;
        Self {
            before_call: Some(Box::new(move |_call| {
                if !seen {
                    seen = true;
                    cancel.cancel();
                }
            })),
            ..Self::new()
        }
    }
}

impl ToolHost for FakeHost {
    fn call(&mut self, call: &ToolCall) -> ToolResult {
        self.calls.push(call.clone());
        if let Some(before) = self.before_call.as_mut() {
            before(call);
        }
        self.queued.pop_front().unwrap_or_else(|| applied(call, 1))
    }
}

fn applied(call: &ToolCall, revision: u64) -> ToolResult {
    ToolResult {
        call_id: call.id.clone(),
        tool: call.name.clone(),
        ok: true,
        code: result_code::APPLIED.into(),
        summary: format!("{} applied", call.name),
        data: Some(serde_json::json!({"board_id": "board-1"})),
        revision: Some(revision),
        audit_id: None,
        error: None,
    }
}

fn refused(call: &ToolCall, code: &str, summary: &str) -> ToolResult {
    ToolResult {
        call_id: call.id.clone(),
        tool: call.name.clone(),
        ok: false,
        code: code.into(),
        summary: summary.into(),
        data: None,
        revision: None,
        audit_id: None,
        error: None,
    }
}

// --- the turn under test ---------------------------------------------------

const CONTEXT_SHA: &str = "sha256:context-under-test";

fn context() -> TurnContext {
    TurnContext {
        system: "You are the KnightLM chess assistant.".into(),
        context_json: "{\"section\":\"position\"}".into(),
        context_sha256: CONTEXT_SHA.into(),
        spec_version: "chat-v1".into(),
        template_version: "1".into(),
        tool_schema_version: "1".into(),
        engine_fingerprint: None,
        tokens_estimate: 42,
    }
}

fn plan() -> TurnPlan {
    TurnPlan::chat(
        "session-1",
        "turn-1",
        ModelRef::new("scripted", "model"),
        context(),
        "what is the best move?",
    )
}

fn move_call(id: &str) -> ToolCall {
    ToolCall::new(
        id,
        "play_move",
        serde_json::json!({"board_id": "board-1", "move": "e4", "expected_revision": 1}),
    )
}

fn read_call(id: &str) -> ToolCall {
    ToolCall::new(id, "get_board", serde_json::json!({"board_id": "board-1"}))
}

fn run(
    provider: &ScriptedProvider,
    host: &mut FakeHost,
    plan: &TurnPlan,
    cancel: &Cancellation,
) -> (rbc_ai::agent::AgentOutcome, Vec<AiEventPayload>) {
    let registry = ToolRegistry::builtin();
    let mut events = Vec::new();
    let outcome = AgentLoop::new(provider, &registry, host)
        .run(plan, cancel, &mut |event| events.push(event));
    (outcome, events)
}

fn has<E>(events: &[AiEventPayload], predicate: E) -> bool
where
    E: Fn(&AiEventPayload) -> bool,
{
    events.iter().any(predicate)
}

// --- context and events ----------------------------------------------------

#[test]
fn agent_loop_announces_context_hash_and_finishes_cleanly() {
    let provider = ScriptedProvider::new(vec![text(20, 8, "Play e4.")]);
    let mut host = FakeHost::new();
    let plan = plan();
    let (outcome, events) = run(&provider, &mut host, &plan, &Cancellation::new());

    assert_eq!(outcome.reply(), "Play e4.");
    assert_eq!(outcome.stop_reason, StopReason::EndTurn);
    assert_eq!(outcome.rounds, 1);
    assert_eq!(outcome.usage.tokens_in, 20);
    assert_eq!(outcome.usage.tokens_out, 8);
    assert!(outcome.error.is_none());

    assert!(has(&events, |event| matches!(
        event,
        AiEventPayload::ContextReady { sha256, tokens: 42, .. } if sha256 == CONTEXT_SHA
    )));
    assert!(has(&events, |event| matches!(
        event,
        AiEventPayload::ContentDelta { text } if text == "Play e4."
    )));
    assert!(has(&events, |event| matches!(
        event,
        AiEventPayload::Usage { usage } if usage.tokens_in == 20
    )));
    assert!(matches!(
        events.last(),
        Some(AiEventPayload::TurnDone { turn_id, stop_reason: StopReason::EndTurn }) if turn_id == "turn-1"
    ));

    // The stored turn carries the context hash that was announced.
    let stored = outcome.drafts[0].to_turn(&plan);
    assert_eq!(stored.context_sha256, CONTEXT_SHA);
    assert_eq!(stored.spec_version, "chat-v1");
    assert_eq!(stored.role, TurnRole::Assistant);
}

#[test]
fn agent_loop_uses_the_compiled_context_as_the_only_system_message() {
    let provider = ScriptedProvider::new(vec![text(1, 1, "ok")]);
    let mut host = FakeHost::new();
    let mut plan = plan();
    plan.history = vec![
        ChatMessage::system("a stale system prompt"),
        ChatMessage::user("earlier question"),
        ChatMessage::assistant("earlier answer"),
    ];
    let _ = run(&provider, &mut host, &plan, &Cancellation::new());

    let request = &provider.requests()[0];
    let system: Vec<&ChatMessage> = request
        .messages
        .iter()
        .filter(|message| message.role == TurnRole::System)
        .collect();
    assert_eq!(system.len(), 1, "one system message, the compiled context");
    assert_eq!(
        system[0].content.as_deref(),
        Some("You are the KnightLM chess assistant.")
    );
    let roles: Vec<TurnRole> = request
        .messages
        .iter()
        .map(|message| message.role)
        .collect();
    assert_eq!(
        roles,
        vec![
            TurnRole::System,
            TurnRole::User,
            TurnRole::Assistant,
            TurnRole::User
        ]
    );
    assert_eq!(
        request
            .messages
            .last()
            .expect("user turn")
            .content
            .as_deref(),
        Some("what is the best move?")
    );
}

// --- tools -----------------------------------------------------------------

#[test]
fn agent_loop_runs_a_tool_and_feeds_its_result_back() {
    let provider = ScriptedProvider::new(vec![
        calls(10, 2, vec![read_call("call_1")]),
        text(12, 6, "The board is at move 12."),
    ]);
    let mut host = FakeHost::new();
    let plan = plan();
    let (outcome, events) = run(&provider, &mut host, &plan, &Cancellation::new());

    assert_eq!(host.calls.len(), 1);
    assert_eq!(host.calls[0].name, "get_board");
    assert_eq!(outcome.tool_results.len(), 1);
    assert!(outcome.tool_results[0].ok);
    assert_eq!(outcome.rounds, 2);
    assert_eq!(outcome.reply(), "The board is at move 12.");

    assert!(has(&events, |event| matches!(
        event,
        AiEventPayload::ToolCall { name, .. } if name == "get_board"
    )));
    assert!(has(&events, |event| matches!(
        event,
        AiEventPayload::ToolResult { ok: true, .. }
    )));
    assert!(has(&events, |event| matches!(
        event,
        AiEventPayload::BoardAction { board_id, revision: 1, .. } if board_id == "board-1"
    )));

    // The second request carries the assistant's call and the tool's answer.
    let second = &provider.requests()[1];
    let assistant = second
        .messages
        .iter()
        .find(|message| !message.tool_calls.is_empty())
        .expect("assistant tool call");
    assert_eq!(assistant.tool_calls[0].id, "call_1");
    let tool = second
        .messages
        .iter()
        .find(|message| message.role == TurnRole::Tool)
        .expect("tool message");
    assert_eq!(tool.tool_call_id.as_deref(), Some("call_1"));
    assert!(
        tool.content
            .as_deref()
            .unwrap_or_default()
            .contains("applied")
    );

    // Usage is the sum of both rounds.
    assert_eq!(outcome.usage.tokens_in, 22);
    assert_eq!(outcome.usage.tokens_out, 8);
}

#[test]
fn agent_loop_stops_after_the_last_allowed_round() {
    let provider = ScriptedProvider::new(vec![
        calls(1, 1, vec![read_call("call_1")]),
        calls(1, 1, vec![read_call("call_2")]),
        calls(1, 1, vec![read_call("call_3")]),
    ]);
    let mut host = FakeHost::new();
    let mut plan = plan();
    plan.limits = TurnLimits {
        max_rounds: 2,
        max_tool_retries: 1,
    };
    let (outcome, events) = run(&provider, &mut host, &plan, &Cancellation::new());

    assert_eq!(outcome.rounds, 2);
    assert_eq!(host.calls.len(), 2);
    assert_eq!(outcome.stop_reason, StopReason::Error);
    let error = outcome.error.as_ref().expect("rounds exhausted");
    assert_eq!(error.code, loop_code::ROUNDS_EXHAUSTED);
    assert!(error.retryable);
    assert!(has(&events, |event| matches!(
        event,
        AiEventPayload::Error { error } if error.code == loop_code::ROUNDS_EXHAUSTED
    )));
}

#[test]
fn agent_loop_does_not_offer_tools_the_mode_denies() {
    let provider = ScriptedProvider::new(vec![text(1, 1, "ok")]);
    let mut host = FakeHost::new();
    let mut plan = plan();
    plan.mode = SessionMode::Play;
    let registry = ToolRegistry::builtin();
    let offered: Vec<String> = AgentLoop::new(&provider, &registry, &mut host)
        .offered_tools(&plan)
        .into_iter()
        .map(|tool| tool.name)
        .collect();

    assert!(offered.contains(&"play_move".to_string()));
    assert!(
        !offered.contains(&"create_study".to_string()),
        "a study write is not offered in a play session"
    );
}

#[test]
fn agent_loop_refuses_a_denied_tool_without_calling_the_host() {
    // Play mode denies session writes; a model may still ask for one.
    let provider = ScriptedProvider::new(vec![
        calls(
            1,
            1,
            vec![ToolCall::new(
                "call_1",
                "create_study",
                serde_json::json!({"title": "Sicilian"}),
            )],
        ),
        text(1, 1, "I cannot create a study here."),
    ]);
    let mut host = FakeHost::new();
    let mut plan = plan();
    plan.mode = SessionMode::Play;
    let (outcome, _events) = run(&provider, &mut host, &plan, &Cancellation::new());

    assert!(
        host.calls.is_empty(),
        "a denied tool never reaches the host"
    );
    assert_eq!(outcome.tool_results[0].code, result_code::DENIED);
    assert_eq!(outcome.stop_reason, StopReason::EndTurn);
}

#[test]
fn agent_loop_asks_for_approval_instead_of_writing() {
    // Chat mode with no grants: a board write needs a person.
    let provider = ScriptedProvider::new(vec![
        calls(1, 1, vec![move_call("call_1")]),
        text(1, 1, "Waiting for your approval."),
    ]);
    let mut host = FakeHost::new();
    let plan = plan();
    let (outcome, events) = run(&provider, &mut host, &plan, &Cancellation::new());

    assert!(host.calls.is_empty(), "nothing is written before approval");
    assert_eq!(outcome.tool_results[0].code, result_code::APPROVAL_REQUIRED);
    assert!(has(&events, |event| matches!(
        event,
        AiEventPayload::ApprovalRequired { approval_id, .. } if approval_id == "call_1"
    )));
    assert_eq!(outcome.stop_reason, StopReason::EndTurn);
}

#[test]
fn agent_loop_runs_a_board_write_when_the_scope_is_granted() {
    let provider = ScriptedProvider::new(vec![
        calls(1, 1, vec![move_call("call_1")]),
        text(1, 1, "Played e4."),
    ]);
    let mut host = FakeHost::new();
    let mut plan = plan();
    plan.permissions = PermissionProfile {
        in_app: vec![PermissionScope::BoardWrite],
        mcp: Vec::new(),
    };
    let (outcome, _events) = run(&provider, &mut host, &plan, &Cancellation::new());

    assert_eq!(host.calls.len(), 1);
    assert!(outcome.tool_results[0].ok);
    assert_eq!(outcome.tool_results[0].revision, Some(1));
}

// --- retries ---------------------------------------------------------------

#[test]
fn agent_loop_retries_an_illegal_move_then_succeeds() {
    let provider = ScriptedProvider::new(vec![
        calls(1, 1, vec![move_call("call_1")]),
        calls(1, 1, vec![move_call("call_2")]),
        text(1, 1, "Played e4."),
    ]);
    let mut host = FakeHost::new();
    // The first attempt is illegal; the host would accept a corrected one.
    host.queued = VecDeque::from(vec![
        refused(
            &move_call("call_1"),
            result_code::ILLEGAL,
            "e4 is not legal",
        ),
        applied(&move_call("call_2"), 2),
    ]);
    let mut plan = plan();
    plan.permissions = PermissionProfile {
        in_app: vec![PermissionScope::BoardWrite],
        mcp: Vec::new(),
    };

    let (outcome, _events) = run(&provider, &mut host, &plan, &Cancellation::new());

    assert_eq!(outcome.rounds, 3, "two attempts and then the answer");
    assert_eq!(host.calls.len(), 2);
    assert_eq!(outcome.tool_results[0].code, result_code::ILLEGAL);
    assert!(outcome.tool_results[1].ok);
    assert_eq!(outcome.stop_reason, StopReason::EndTurn);
    assert!(outcome.error.is_none());
}

#[test]
fn agent_loop_stops_when_a_tool_keeps_failing() {
    let provider = ScriptedProvider::new(vec![
        calls(1, 1, vec![move_call("call_1")]),
        calls(1, 1, vec![move_call("call_2")]),
        calls(1, 1, vec![move_call("call_3")]),
    ]);
    let mut host = FakeHost::new();
    host.queued = VecDeque::from(vec![
        refused(&move_call("call_1"), result_code::ILLEGAL, "still illegal"),
        refused(&move_call("call_2"), result_code::ILLEGAL, "still illegal"),
        refused(&move_call("call_3"), result_code::ILLEGAL, "still illegal"),
    ]);
    let mut plan = plan();
    plan.permissions = PermissionProfile {
        in_app: vec![PermissionScope::BoardWrite],
        mcp: Vec::new(),
    };
    plan.limits = TurnLimits {
        max_rounds: 5,
        max_tool_retries: 1,
    };

    let (outcome, _events) = run(&provider, &mut host, &plan, &Cancellation::new());

    assert_eq!(host.calls.len(), 2, "the third attempt is never made");
    assert_eq!(outcome.stop_reason, StopReason::Error);
    assert_eq!(
        outcome.error.as_ref().expect("error").code,
        loop_code::TOOL_RETRIES_EXHAUSTED
    );
}

#[test]
fn agent_loop_answers_when_the_model_asks_for_a_tool_that_does_not_exist() {
    let provider = ScriptedProvider::new(vec![
        calls(
            1,
            1,
            vec![ToolCall::new(
                "call_1",
                "summon_dragon",
                serde_json::json!({}),
            )],
        ),
        text(1, 1, "That tool does not exist."),
    ]);
    let mut host = FakeHost::new();
    let (outcome, _events) = run(&provider, &mut host, &plan(), &Cancellation::new());

    assert!(host.calls.is_empty());
    assert_eq!(outcome.tool_results[0].code, result_code::NOT_FOUND);
    assert_eq!(outcome.stop_reason, StopReason::EndTurn);
}

// --- cancellation and failures ---------------------------------------------

#[test]
fn agent_loop_stops_further_tool_writes_when_cancelled() {
    let cancel = Cancellation::new();
    let provider = ScriptedProvider::new(vec![calls(
        1,
        1,
        vec![move_call("call_1"), read_call("call_2")],
    )]);
    let mut host = FakeHost::cancelling_on_first_call(cancel.clone());
    let mut plan = plan();
    plan.permissions = PermissionProfile {
        in_app: vec![PermissionScope::BoardWrite],
        mcp: Vec::new(),
    };

    let (outcome, _events) = run(&provider, &mut host, &plan, &cancel);

    assert_eq!(host.calls.len(), 1, "the second call is never executed");
    assert_eq!(outcome.stop_reason, StopReason::Cancelled);
    assert_eq!(outcome.rounds, 1);
}

#[test]
fn agent_loop_makes_no_request_when_cancelled_before_it_starts() {
    let cancel = Cancellation::new();
    cancel.cancel();
    let provider = ScriptedProvider::new(vec![text(1, 1, "never sent")]);
    let mut host = FakeHost::new();
    let (outcome, _events) = run(&provider, &mut host, &plan(), &cancel);

    assert!(provider.requests().is_empty());
    assert_eq!(outcome.stop_reason, StopReason::Cancelled);
    assert!(outcome.drafts.is_empty());
}

#[test]
fn agent_loop_ends_the_turn_when_the_provider_fails() {
    let provider = ScriptedProvider::new(vec![Err(ErrorInfo {
        code: rbc_ai::provider::error_code::SERVER.into(),
        message: "the model provider reported a server error".into(),
        retryable: true,
        provider_code: Some("http_503".into()),
    })]);
    let mut host = FakeHost::new();
    let plan = plan();
    let (outcome, events) = run(&provider, &mut host, &plan, &Cancellation::new());

    assert_eq!(outcome.stop_reason, StopReason::Error);
    let error = outcome.error.as_ref().expect("error recorded");
    assert_eq!(error.code, rbc_ai::provider::error_code::SERVER);
    assert!(has(&events, |event| matches!(
        event,
        AiEventPayload::Error { error } if error.retryable
    )));
    // The failed turn still records what happened.
    let draft = outcome.drafts.last().expect("a draft for the failure");
    assert_eq!(draft.error.as_ref().expect("draft error").code, error.code);
    assert!(draft.content.is_empty());
}

// --- persistence -----------------------------------------------------------

#[test]
fn agent_loop_persists_usage_context_and_errors() {
    let store = rbc_ai::store::Store::in_memory().expect("in-memory store");
    let session = store
        .create_session(
            "Turn loop",
            SessionMode::Chat,
            None,
            None,
            rbc_ai::store::SessionTalk {
                model: &ModelRef::new("scripted", "model"),
                context_spec: &rbc_ai::protocol::ContextSpecRef::new("chat-v1", "1"),
                permissions: &PermissionProfile::default(),
            },
        )
        .expect("session");

    // A good turn.
    let provider = ScriptedProvider::new(vec![text(20, 8, "Play e4.")]);
    let mut host = FakeHost::new();
    let plan = TurnPlan::chat(
        session.id.clone(),
        "turn-1",
        ModelRef::new("scripted", "model"),
        context(),
        "what is the best move?",
    );
    let (outcome, _events) = run(&provider, &mut host, &plan, &Cancellation::new());

    let user = store
        .append_turn(
            &session.id,
            &rbc_ai::protocol::AiTurn {
                role: TurnRole::User,
                content: plan.user_message.clone(),
                context_sha256: CONTEXT_SHA.into(),
                model: plan.model.clone(),
                ..empty_turn(&session.id)
            },
        )
        .expect("user turn");
    assert_eq!(user.seq, 1);

    let assistant = store
        .append_turn(&session.id, &outcome.drafts[0].to_turn(&plan))
        .expect("assistant turn");
    assert_eq!(assistant.seq, 2);
    assert_eq!(assistant.usage.tokens_in, 20);
    assert_eq!(assistant.usage.tokens_out, 8);
    assert_eq!(assistant.context_sha256, CONTEXT_SHA);
    assert!(assistant.tool_calls.is_empty());

    // A failing turn against the same session.
    let failing = ScriptedProvider::new(vec![Err(ErrorInfo {
        code: rbc_ai::provider::error_code::AUTH.into(),
        message: "the model provider rejected the credentials".into(),
        retryable: false,
        provider_code: None,
    })]);
    let mut host = FakeHost::new();
    let failed_plan = TurnPlan::chat(
        session.id.clone(),
        "turn-2",
        ModelRef::new("scripted", "model"),
        context(),
        "try again",
    );
    let (failed, _events) = run(&failing, &mut host, &failed_plan, &Cancellation::new());
    let stored = store
        .append_turn(&session.id, &failed.drafts[0].to_turn(&failed_plan))
        .expect("failed turn");
    assert_eq!(
        stored.error.as_ref().expect("error row").code,
        rbc_ai::provider::error_code::AUTH
    );

    // Everything reads back from disk in order.
    let turns = store.turns(&session.id).expect("turns");
    assert_eq!(turns.len(), 3);
    assert_eq!(turns[1].content, "Play e4.");
    assert_eq!(turns[1].context_sha256, CONTEXT_SHA);
    assert!(turns[2].error.is_some());
}

/// A turn row with every field defaulted, for building test rows.
fn empty_turn(session: &str) -> rbc_ai::protocol::AiTurn {
    rbc_ai::protocol::AiTurn {
        id: String::new(),
        session_id: session.to_string(),
        seq: 0,
        role: TurnRole::User,
        content: String::new(),
        reasoning: None,
        tool_calls: Vec::new(),
        tool_results: Vec::new(),
        context_json: String::new(),
        context_sha256: String::new(),
        spec_version: String::new(),
        template_version: String::new(),
        tool_schema_version: String::new(),
        engine_fingerprint: None,
        model: ModelRef::new("test", "test"),
        usage: Usage::default(),
        error: None,
        created_at: 0,
    }
}
