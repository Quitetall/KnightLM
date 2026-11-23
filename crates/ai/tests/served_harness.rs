//! The seam contract from the app's side (docs/model/seam.md): what RBC sends a KnightLM
//! server and what it does with the answer. A scripted provider stands in for the server and
//! records the requests; the prompt text is compared with the contract goldens, which
//! `model/prompt.py` reproduces byte for byte.
#![cfg(feature = "provider")]

use std::sync::Mutex;

use rbc_ai::game::{Match, MatchConfig, Player};
use rbc_ai::model_turn::ChatLine;
use rbc_ai::play::{CoachContext, MoveHarness, PlayPolicy, request_move_with_harness};
use rbc_ai::protocol::{ErrorInfo, ModelRef, StopReason, ToolCall};
use rbc_ai::provider::{
    Cancellation, CompletionRequest, CompletionResponse, Provider, ProviderEvent,
};
use serde_json::{Value, json};

struct Server {
    replies: Mutex<Vec<Result<CompletionResponse, ErrorInfo>>>,
    seen: Mutex<Vec<CompletionRequest>>,
}

impl Server {
    fn new(replies: Vec<Result<CompletionResponse, ErrorInfo>>) -> Self {
        Self {
            replies: Mutex::new(replies),
            seen: Mutex::new(Vec::new()),
        }
    }
    fn requests(&self) -> Vec<CompletionRequest> {
        self.seen.lock().unwrap().clone()
    }
}

impl Provider for Server {
    fn name(&self) -> &str {
        "served"
    }
    fn complete(
        &self,
        request: &CompletionRequest,
        _cancel: &Cancellation,
        _on_event: &mut dyn FnMut(ProviderEvent),
    ) -> Result<CompletionResponse, ErrorInfo> {
        self.seen.lock().unwrap().push(request.clone());
        let mut replies = self.replies.lock().unwrap();
        assert!(!replies.is_empty(), "the app asked for more than scripted");
        replies.remove(0)
    }
}

fn play_reply(mv: &str, path: &str, legal: bool) -> Result<CompletionResponse, ErrorInfo> {
    let calls = if mv.is_empty() {
        Vec::new()
    } else {
        vec![ToolCall {
            id: "call_1".into(),
            name: "submit_move".into(),
            arguments: json!({"reasoning": "because", "move": mv}),
        }]
    };
    Ok(CompletionResponse {
        content: if mv.is_empty() {
            "I would play something".into()
        } else {
            String::new()
        },
        reasoning: None,
        tool_calls: calls,
        usage: Default::default(),
        stop_reason: StopReason::ToolCalls,
        provider_request_id: None,
        knightlm: Some(json!({
            "path": path, "code": "ok", "raw_code": "ok", "legal": legal,
            "manifest_hash": "abc"
        })),
    })
}

fn coach_reply(
    content: &str,
    mv: Option<&str>,
    fallback: bool,
    calls: Vec<ToolCall>,
) -> Result<CompletionResponse, ErrorInfo> {
    Ok(CompletionResponse {
        content: content.into(),
        reasoning: None,
        tool_calls: calls,
        usage: Default::default(),
        stop_reason: StopReason::EndTurn,
        provider_request_id: None,
        knightlm: Some(json!({
            "path": if fallback { "constrained_move" } else { "model" },
            "code": "ok", "raw_code": "ok", "legal": mv.is_some(), "manifest_hash": "abc",
            "coach": {"move": mv, "provisional": mv.iter().collect::<Vec<_>>(),
                      "rejected": 0, "fallback": fallback, "refused_calls": []}
        })),
    })
}

fn game() -> Match {
    Match::new(MatchConfig {
        white: Player::Model {
            provider: "provider:p1".into(),
            model: "knightlm".into(),
        },
        black: Player::Human {
            name: "Alex".into(),
        },
        ..MatchConfig::default()
    })
    .unwrap()
}

fn model() -> ModelRef {
    ModelRef::new("provider:p1", "knightlm")
}

fn run(
    game: &mut Match,
    server: &Server,
    harness: &MoveHarness,
) -> Result<rbc_ai::play::HarnessMove, ErrorInfo> {
    request_move_with_harness(
        game,
        server,
        &model(),
        &PlayPolicy::casual(),
        harness,
        &Cancellation::new(),
    )
}

fn golden(key: &str) -> String {
    let goldens: Value = serde_json::from_str(knightlm_contract::MODEL_PROMPTS_JSON).unwrap();
    goldens["prefixes"][key].as_str().unwrap().to_owned()
}

fn last_message(request: &CompletionRequest) -> String {
    request.messages.last().unwrap().content.clone().unwrap()
}

/// The packet the contract carries: the canonical `ContextPacket`, WITH `legal_moves`.
fn packet_of(text: &str, marker: &str) -> Value {
    serde_json::from_str(text.strip_prefix(marker).unwrap_or(text)).unwrap()
}

#[test]
fn the_play_request_is_the_contract_prompt_with_legal_moves() {
    let mut game = game();
    let server = Server::new(vec![play_reply("e2e4", "model", true)]);
    let moved = run(&mut game, &server, &MoveHarness::ServedPlay).unwrap();
    assert_eq!(moved.played.mv.uci, "e2e4");
    assert!(!moved.fallback);
    assert_eq!(moved.rejected, 0);

    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    // `PACKET_MARKER` of model/prompt.py is the play prefix plus the packet label.
    let marker = format!("{}Position packet:\n", golden("play"));
    let text = last_message(request);
    assert!(text.starts_with(&marker), "the play prompt of the contract");
    let packet = packet_of(&text, &marker);
    let legal: Vec<String> = serde_json::from_value(packet["legal_moves"].clone())
        .expect("the packet carries legal_moves, which the server's commit gate needs");
    assert_eq!(legal.len(), 20);
    assert_eq!(
        packet["schema_version"],
        json!(knightlm_contract::PACKET_FORMAT_VERSION)
    );
    // The legacy generic prompt is gone from this path.
    assert!(!text.contains("Choose a legal chess move"));
    assert!(request.grammar.is_none());
    assert_eq!(
        request.knightlm,
        Some(json!({"turn": "play", "mode": "raw"}))
    );
    assert!(!request.stream);
}

#[test]
fn an_illegal_move_is_not_trusted_and_the_constrained_path_is_labelled() {
    let mut game = game();
    let server = Server::new(vec![
        play_reply("e2e5", "model", false),
        play_reply("d2d4", "constrained_move", true),
    ]);
    let moved = run(&mut game, &server, &MoveHarness::ServedPlay).unwrap();
    assert_eq!(moved.played.mv.uci, "d2d4");
    assert!(moved.fallback, "a constrained fallback is an intervention");
    assert_eq!(moved.rejected, 1);
    assert_eq!(moved.played.attempts, 2);
    let modes: Vec<Value> = server
        .requests()
        .iter()
        .map(|r| r.knightlm.clone().unwrap()["mode"].clone())
        .collect();
    assert_eq!(modes, [json!("raw"), json!("constrained")]);
}

#[test]
fn a_raw_refusal_without_a_call_falls_back_the_same_way() {
    let mut game = game();
    let server = Server::new(vec![
        play_reply("", "none", false),
        play_reply("g1f3", "constrained_forced_reasoning", true),
    ]);
    let moved = run(&mut game, &server, &MoveHarness::ServedPlay).unwrap();
    assert_eq!(moved.played.mv.uci, "g1f3");
    assert!(moved.fallback);
}

#[test]
fn a_move_outside_the_legal_list_never_reaches_the_board() {
    let mut game = game();
    // The server claims it is legal both times; the app checks the real position itself.
    let server = Server::new(vec![
        play_reply("e2e5", "model", true),
        play_reply("e2e5", "constrained_move", true),
    ]);
    let failure = run(&mut game, &server, &MoveHarness::ServedPlay).unwrap_err();
    assert!(failure.message.contains("outside the legal move list"));
    assert!(game.moves().is_empty(), "the match is untouched");
}

#[test]
fn a_server_error_is_surfaced_and_the_match_is_untouched() {
    let mut game = game();
    let server = Server::new(vec![Err(ErrorInfo {
        code: "provider_bad_request".into(),
        message: "no_packet".into(),
        retryable: false,
        provider_code: None,
    })]);
    let failure = run(&mut game, &server, &MoveHarness::ServedPlay).unwrap_err();
    assert_eq!(failure.message, "no_packet");
    assert!(game.moves().is_empty());
}

fn mark_call() -> ToolCall {
    ToolCall {
        id: "call_9".into(),
        name: "mark_square".into(),
        arguments: json!({"square": "e4", "color": "blue"}),
    }
}

#[test]
fn the_coach_request_carries_the_chat_and_the_packet_and_no_prompt() {
    let mut game = game();
    let server = Server::new(vec![coach_reply(
        "The centre first.",
        Some("e2e4"),
        false,
        vec![mark_call()],
    )]);
    let context = CoachContext {
        summary: Some("We discussed the Ruy Lopez.".into()),
        recent: vec![
            ChatLine::new("user", "Why not Nf3?"),
            ChatLine::new("assistant", "It blocks the f-pawn."),
        ],
    };
    let moved = run(&mut game, &server, &MoveHarness::ServedCoach(context)).unwrap();
    assert_eq!(moved.played.mv.uci, "e2e4");
    assert!(!moved.fallback);
    assert_eq!(moved.reply.as_deref(), Some("The centre first."));
    assert_eq!(moved.coach_calls, 1);
    assert_eq!(moved.marks.len(), 1);
    assert_eq!(moved.ui.len(), 1);

    let request = &server.requests()[0];
    assert_eq!(
        request.knightlm,
        Some(json!({"turn": "coach", "mode": "raw",
                    "summary": "We discussed the Ruy Lopez."}))
    );
    // Earlier chat as messages; the packet last. The server renders `Mode: coach`, both
    // schemas and the `Provisional:` rule itself (model/serve/coach.py), so the app sends
    // none of that text: anything before the packet would be read as something the person typed.
    let roles: Vec<String> = request
        .messages
        .iter()
        .map(|m| format!("{:?}", m.role))
        .collect();
    assert_eq!(roles, ["User", "Assistant", "User"]);
    let text = last_message(request);
    assert!(!text.contains("Mode:") && !text.contains("Tools:"));
    let packet: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(packet["legal_moves"].as_array().unwrap().len(), 20);
}

#[test]
fn a_coach_fallback_withholds_the_reply_and_the_board_effects() {
    let mut game = game();
    let server = Server::new(vec![
        coach_reply("nothing legal here", None, true, vec![]),
        coach_reply("", Some("d2d4"), true, vec![mark_call()]),
    ]);
    let moved = run(
        &mut game,
        &server,
        &MoveHarness::ServedCoach(CoachContext::default()),
    )
    .unwrap();
    assert_eq!(moved.played.mv.uci, "d2d4");
    assert!(moved.fallback);
    assert_eq!(moved.reply, None);
    assert!(moved.ui.is_empty() && moved.marks.is_empty());
    let modes: Vec<Value> = server
        .requests()
        .iter()
        .map(|r| r.knightlm.clone().unwrap()["mode"].clone())
        .collect();
    assert_eq!(modes, [json!("raw"), json!("constrained")]);
}

#[test]
fn an_illegal_coach_move_is_never_applied() {
    let mut game = game();
    let server = Server::new(vec![
        coach_reply("hm", Some("a1a1"), false, vec![]),
        coach_reply("", Some("a1a1"), true, vec![]),
    ]);
    let failure = run(
        &mut game,
        &server,
        &MoveHarness::ServedCoach(CoachContext::default()),
    )
    .unwrap_err();
    assert!(failure.message.contains("outside the legal move list"));
    assert!(game.moves().is_empty());
}

#[test]
fn the_golden_play_prefix_is_what_the_default_tools_harness_would_send_too() {
    // The contract has one play prefix. The served path and the text-tool harness share it,
    // so a change to the golden moves both.
    assert_eq!(
        golden("play"),
        rbc_ai::model_turn::mode_prefix(rbc_ai::tools::ModelMode::Play)
    );
}
