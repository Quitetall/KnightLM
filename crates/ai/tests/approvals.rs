//! The approval queue: what a person is asked, and what happens if they never
//! answer, answer twice, or try to answer something that has already expired.
//!
//! The bypass tests at the end drive the real agent loop: a write the policy
//! does not auto-approve must become a request and must never reach the board.
#![cfg(all(feature = "host", feature = "provider"))]

use std::cell::RefCell;
use std::sync::Mutex;

use rbc_ai::agent::{AgentLoop, ToolHost, TurnContext, TurnLimits, TurnPlan};
use rbc_ai::approval::{ApprovalError, ApprovalStore, Changeset};
use rbc_ai::board::{BoardContent, BoardWriteHost};
use rbc_ai::policy::{Origin, PermissionProfile, PermissionScope};
use rbc_ai::protocol::{
    AiEventPayload, ApprovalStatus, ErrorInfo, ModelRef, SessionMode, StopReason, ToolCall,
    ToolResult, Usage, result_code,
};
use rbc_ai::provider::{
    Cancellation, CompletionRequest, CompletionResponse, Provider, ProviderEvent,
};
use rbc_ai::tools::ToolRegistry;
use rbc_db::{Board, Db};

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

fn setup() -> (Db, Board) {
    let db = Db::open_memory().expect("database");
    let workspace = db.create_workspace("W", "analysis").expect("workspace");
    let board = db
        .create_board("workspace", &workspace.id, "analysis", "Board 1", START, "")
        .expect("board");
    (db, board)
}

fn call(name: &str, arguments: serde_json::Value) -> ToolCall {
    ToolCall::new("call_1", name, arguments)
}

fn move_call(board: &Board, text: &str) -> ToolCall {
    call(
        "play_move",
        serde_json::json!({
            "board_id": board.id,
            "move": text,
            "expected_revision": board.revision,
        }),
    )
}

fn content(moves: &[&str]) -> BoardContent {
    BoardContent {
        moves: moves.iter().map(|mv| mv.to_string()).collect(),
        ..BoardContent::default()
    }
}

// --- the changeset a person reads -----------------------------------------

#[test]
fn approvals_changeset_shows_the_before_and_after_board() {
    let changeset = Changeset::for_board(
        "board-1",
        "Main",
        4,
        "play_move",
        content(&["e4"]),
        content(&["e4", "e5"]),
    );
    let round_trip: Changeset =
        serde_json::from_str(&changeset.to_json()).expect("changeset json");
    assert_eq!(round_trip.summary, "play_move: moves 1 -> 2 (e5)");
    let board = round_trip.board.expect("board change");
    assert_eq!(board.before.moves, vec!["e4"]);
    assert_eq!(board.after.moves, vec!["e4", "e5"]);
    assert_eq!(board.expected_revision, 4);
}

// --- the lifecycle ---------------------------------------------------------

#[test]
fn approvals_request_starts_pending_and_is_pollable() {
    let (db, board) = setup();
    let store = ApprovalStore::new(&db);
    let changeset = Changeset::for_board(
        board.id.clone(),
        board.title.clone(),
        board.revision,
        "play_move",
        content(&[]),
        content(&["e4"]),
    );

    let request = store
        .request(Origin::InApp, Some("session-1"), &move_call(&board, "e4"), &changeset, 1_000)
        .expect("queued");

    assert_eq!(request.status, ApprovalStatus::Pending);
    assert_eq!(request.tool, "play_move");
    assert_eq!(request.target_kind.as_deref(), Some("board"));
    assert_eq!(request.target_id.as_deref(), Some(board.id.as_str()));
    assert_eq!(request.session_id.as_deref(), Some("session-1"));
    assert_eq!(request.expires_at, Some(1_000 + rbc_ai::approval::DEFAULT_TTL_MS));
    // An MCP client polls by id.
    assert_eq!(store.status(&request.id).expect("status"), Some(ApprovalStatus::Pending));
    assert_eq!(store.get(&request.id).expect("get").expect("row").id, request.id);
}

#[test]
fn approvals_approve_and_deny_move_only_a_pending_request() {
    let (db, board) = setup();
    let store = ApprovalStore::new(&db);
    let changeset = Changeset::for_board(board.id.clone(), board.title.clone(), 1, "play_move", content(&[]), content(&["e4"]));

    let approved = store
        .request(Origin::InApp, None, &move_call(&board, "e4"), &changeset, 1_000)
        .expect("queued");
    let resolved = store.approve(&approved.id, 2_000).expect("approved");
    assert_eq!(resolved.status, ApprovalStatus::Approved);
    assert_eq!(resolved.resolved_at, Some(2_000));
    assert!(store.pending(2_000).expect("pending").is_empty());

    let denied = store
        .request(Origin::InApp, None, &move_call(&board, "e4"), &changeset, 3_000)
        .expect("queued");
    let resolved = store.deny(&denied.id, 4_000).expect("denied");
    assert_eq!(resolved.status, ApprovalStatus::Denied);
}

#[test]
fn approvals_cannot_be_answered_twice() {
    let (db, board) = setup();
    let store = ApprovalStore::new(&db);
    let changeset = Changeset::for_board(board.id.clone(), board.title.clone(), 1, "play_move", content(&[]), content(&["e4"]));
    let request = store
        .request(Origin::InApp, None, &move_call(&board, "e4"), &changeset, 1_000)
        .expect("queued");

    store.approve(&request.id, 2_000).expect("first answer wins");
    let error = store.deny(&request.id, 3_000).expect_err("second answer refused");
    assert_eq!(error, ApprovalError::AlreadyResolved(ApprovalStatus::Approved));
    assert_eq!(
        store.status(&request.id).expect("status"),
        Some(ApprovalStatus::Approved),
        "the first answer stands"
    );
}

#[test]
fn approvals_expire_at_their_deadline() {
    let (db, board) = setup();
    let store = ApprovalStore::with_ttl(&db, 1_000);
    let changeset = Changeset::for_board(board.id.clone(), board.title.clone(), 1, "play_move", content(&[]), content(&["e4"]));
    let request = store
        .request(Origin::InApp, None, &move_call(&board, "e4"), &changeset, 10_000)
        .expect("queued");

    // A second after the deadline, approving is refused and the row is expired.
    let error = store
        .approve(&request.id, 11_000)
        .expect_err("too late");
    assert_eq!(error, ApprovalError::Expired);
    assert_eq!(
        store.status(&request.id).expect("status"),
        Some(ApprovalStatus::Expired)
    );
}

#[test]
fn approvals_sweep_expires_what_nobody_answered() {
    let (db, board) = setup();
    let store = ApprovalStore::with_ttl(&db, 1_000);
    let changeset = Changeset::for_board(board.id.clone(), board.title.clone(), 1, "play_move", content(&[]), content(&["e4"]));
    let stale = store
        .request(Origin::InApp, None, &move_call(&board, "e4"), &changeset, 1_000)
        .expect("queued");
    let fresh = store
        .request(Origin::InApp, None, &move_call(&board, "e4"), &changeset, 1_500)
        .expect("queued");

    // The stale request's deadline is 2_000, the fresh one's is 2_500.
    let pending = store.pending(2_100).expect("pending");
    assert_eq!(pending.len(), 1, "only the fresh one is still actionable");
    assert_eq!(pending[0].id, fresh.id);
    assert_eq!(
        store.status(&stale.id).expect("status"),
        Some(ApprovalStatus::Expired)
    );
}

#[test]
fn approvals_pending_is_oldest_first_and_ignores_answered_requests() {
    let (db, board) = setup();
    let store = ApprovalStore::new(&db);
    let changeset = Changeset::for_board(board.id.clone(), board.title.clone(), 1, "play_move", content(&[]), content(&["e4"]));
    let first = store
        .request(Origin::InApp, None, &move_call(&board, "e4"), &changeset, 1_000)
        .expect("queued");
    let second = store
        .request(Origin::InApp, None, &move_call(&board, "d4"), &changeset, 2_000)
        .expect("queued");
    store.approve(&second.id, 3_000).expect("approved");

    let pending = store.pending(4_000).expect("pending");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, first.id);
}

#[test]
fn approvals_unknown_id_is_reported_by_name() {
    let (db, _board) = setup();
    let store = ApprovalStore::new(&db);
    assert!(store.get("nope").expect("get").is_none());
    assert!(store.status("nope").expect("status").is_none());
    assert_eq!(
        store.approve("nope", 1).expect_err("missing"),
        ApprovalError::NotFound("nope".to_string())
    );
}

#[test]
fn approvals_changeset_survives_storage_unchanged() {
    let (db, board) = setup();
    let store = ApprovalStore::new(&db);
    let mut after = content(&["e4"]);
    after.notes_md = "plan".into();
    let changeset = Changeset::for_board(board.id.clone(), board.title.clone(), 1, "write_note", content(&[]), after);
    let request = store
        .request(Origin::Mcp, None, &call("write_note", serde_json::json!({"board_id": board.id})), &changeset, 1_000)
        .expect("queued");

    let stored = rbc_ai::approval::changeset_of(&request).expect("changeset");
    assert_eq!(stored, changeset);
    assert_eq!(request.origin, Origin::Mcp, "MCP requests keep their origin");
}

// --- bypass ----------------------------------------------------------------

/// A host that queues approvals exactly as the server does.
struct QueueHost<'a> {
    approvals: ApprovalStore<'a>,
    boards: BoardWriteHost<'a>,
    origin: Origin,
    session: String,
    executed: RefCell<Vec<String>>,
}

impl<'a> QueueHost<'a> {
    fn new(db: &'a Db, origin: Origin) -> Self {
        Self {
            approvals: ApprovalStore::new(db),
            boards: BoardWriteHost::new(db, "test"),
            origin,
            session: "session-1".into(),
            executed: RefCell::new(Vec::new()),
        }
    }
}

impl ToolHost for QueueHost<'_> {
    fn call(&mut self, call: &ToolCall) -> ToolResult {
        self.executed.borrow_mut().push(call.name.clone());
        self.boards.call(call)
    }

    fn request_approval(&mut self, call: &ToolCall, summary: String) -> Option<String> {
        let board_id = call.arguments.get("board_id")?.as_str()?.to_string();
        let mut changeset = Changeset::for_board(
            board_id.clone(),
            "Board 1",
            1,
            call.name.clone(),
            BoardContent::default(),
            BoardContent::default(),
        );
        // The queue row reads better with the request in it than with the
        // placeholder "no visible change".
        changeset.summary = summary.clone();
        changeset.notes = vec![format!("requested: {} {}", call.name, call.arguments)];
        self.approvals
            .request(self.origin, Some(&self.session), call, &changeset, 1_000)
            .ok()
            .map(|request| request.id)
    }
}

/// A provider that asks for one tool call and then answers.
struct ScriptedProvider {
    responses: Mutex<Vec<Result<CompletionResponse, ErrorInfo>>>,
}

impl ScriptedProvider {
    fn new(responses: Vec<Result<CompletionResponse, ErrorInfo>>) -> Self {
        Self {
            responses: Mutex::new(responses),
        }
    }
}

impl Provider for ScriptedProvider {
    fn name(&self) -> &str {
        "scripted"
    }

    fn complete(
        &self,
        _request: &CompletionRequest,
        _cancel: &Cancellation,
        on_event: &mut dyn FnMut(ProviderEvent),
    ) -> Result<CompletionResponse, ErrorInfo> {
        let next = self.responses.lock().expect("responses lock").remove(0);
        match next {
            Ok(response) => {
                if !response.content.is_empty() {
                    on_event(ProviderEvent::ContentDelta(response.content.clone()));
                }
                Ok(response)
            }
            Err(error) => Err(error),
        }
    }
}

fn tool_response(calls: Vec<ToolCall>) -> Result<CompletionResponse, ErrorInfo> {
    Ok(CompletionResponse {
        content: String::new(),
        reasoning: None,
        tool_calls: calls,
        usage: Usage::default(),
        stop_reason: StopReason::ToolCalls,
        provider_request_id: None,
    })
}

fn text_response(text: &str) -> Result<CompletionResponse, ErrorInfo> {
    Ok(CompletionResponse {
        content: text.to_string(),
        reasoning: None,
        tool_calls: Vec::new(),
        usage: Usage::default(),
        stop_reason: StopReason::EndTurn,
        provider_request_id: None,
    })
}

fn plan(mode: SessionMode, permissions: PermissionProfile) -> TurnPlan {
    let mut plan = TurnPlan::chat(
        "session-1",
        "turn-1",
        ModelRef::new("scripted", "m"),
        TurnContext {
            system: "ask before writing".into(),
            context_json: "{}".into(),
            context_sha256: "sha256:test".into(),
            spec_version: "chat-v1@1".into(),
            template_version: "1".into(),
            tool_schema_version: "1".into(),
            engine_fingerprint: None,
            tokens_estimate: 1,
        },
        "play e4",
    );
    plan.mode = mode;
    plan.permissions = permissions;
    plan.limits = TurnLimits {
        max_rounds: 3,
        max_tool_retries: 1,
    };
    plan
}

#[test]
fn approvals_ungranted_write_becomes_a_request_and_never_runs() {
    let (db, board) = setup();
    let provider = ScriptedProvider::new(vec![
        tool_response(vec![move_call(&board, "e4")]),
        text_response("waiting for your approval"),
    ]);
    let mut host = QueueHost::new(&db, Origin::InApp);
    let registry = ToolRegistry::builtin();
    let mut events = Vec::new();
    let outcome = AgentLoop::new(&provider, &registry, &mut host).run(
        &plan(SessionMode::Chat, PermissionProfile::default()),
        &Cancellation::new(),
        &mut |event| events.push(event),
    );

    assert!(host.executed.borrow().is_empty(), "the board was never written");
    assert_eq!(outcome.tool_results[0].code, result_code::APPROVAL_REQUIRED);
    let approval_id = outcome.tool_results[0]
        .data
        .as_ref()
        .and_then(|data| data.get("approval_id"))
        .and_then(|id| id.as_str())
        .expect("the result names the queued request")
        .to_string();

    let pending = ApprovalStore::new(&db).pending(2_000).expect("pending");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, approval_id);
    assert_eq!(pending[0].tool, "play_move");
    assert_eq!(pending[0].status, ApprovalStatus::Pending);

    // The stream points at the same request.
    assert!(events.iter().any(|event| matches!(
        event,
        AiEventPayload::ApprovalRequired { approval_id: id, .. } if *id == approval_id
    )));

    // The board still has no move on it.
    let stored = db.board(&board.id).expect("board").expect("row");
    assert_eq!(stored.content_json, "");
}

#[test]
fn approvals_granted_scope_writes_without_asking() {
    let (db, board) = setup();
    let provider = ScriptedProvider::new(vec![
        tool_response(vec![move_call(&board, "e4")]),
        text_response("played e4"),
    ]);
    let mut host = QueueHost::new(&db, Origin::InApp);
    let registry = ToolRegistry::builtin();
    let outcome = AgentLoop::new(&provider, &registry, &mut host).run(
        &plan(
            SessionMode::Chat,
            PermissionProfile {
                in_app: vec![PermissionScope::BoardWrite],
                mcp: Vec::new(),
            },
        ),
        &Cancellation::new(),
        &mut |_event| {},
    );

    assert_eq!(host.executed.borrow().as_slice(), ["play_move"]);
    assert!(outcome.tool_results[0].ok);
    assert!(
        ApprovalStore::new(&db).pending(2_000).expect("pending").is_empty(),
        "an auto-approved write creates no request"
    );
    let stored = db.board(&board.id).expect("board").expect("row");
    assert_eq!(stored.content_json, r#"{"moves":["e4"]}"#);
}

#[test]
fn approvals_an_mcp_call_cannot_borrow_the_in_app_grant() {
    let (db, board) = setup();
    let provider = ScriptedProvider::new(vec![
        tool_response(vec![move_call(&board, "e4")]),
        text_response("waiting for approval"),
    ]);
    let mut host = QueueHost::new(&db, Origin::Mcp);
    let registry = ToolRegistry::builtin();
    let mut mcp_plan = plan(
        SessionMode::Chat,
        PermissionProfile {
            // Granted for the app only; MCP has no grant of its own.
            in_app: vec![PermissionScope::BoardWrite],
            mcp: Vec::new(),
        },
    );
    mcp_plan.origin = Origin::Mcp;
    let outcome = AgentLoop::new(&provider, &registry, &mut host).run(
        &mcp_plan,
        &Cancellation::new(),
        &mut |_event| {},
    );

    assert!(host.executed.borrow().is_empty());
    assert_eq!(outcome.tool_results[0].code, result_code::APPROVAL_REQUIRED);
    assert_eq!(ApprovalStore::new(&db).pending(2_000).expect("pending").len(), 1);
}
