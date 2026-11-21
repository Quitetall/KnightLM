//! RBC reaches a model only through an OpenAI-compatible API. These tests stand
//! up a scripted fake of that API on a loopback port and drive the real router:
//! provider settings and the typed connection test, per-seat selection, and the
//! chat toggle (coach turn with a rolling summary, versus the competition play
//! harness), including the structured marks a coach turn returns for the board.
//! No KnightLM code and no model weights are involved.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

const KEY_NAME: &str = "RBC_TEST_FAKE_MODEL_KEY";
// Set once per test and never removed: tests share the process environment, and the
// value is the same everywhere.
const KEY_VALUE: &str = "fake-model-secret-value";

#[derive(Default)]
struct Fake {
    replies: VecDeque<String>,
    seen: Vec<Value>,
}

type Shared = Arc<Mutex<Fake>>;

fn authorised(headers: &HeaderMap) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        == Some(&format!("Bearer {KEY_VALUE}"))
}

async fn fake_models(headers: HeaderMap) -> Response {
    if !authorised(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            json!({"error": {"message": "bad key"}}).to_string(),
        )
            .into_response();
    }
    axum::Json(json!({"object": "list", "data": [{"id": "fake-model"}]})).into_response()
}

async fn fake_chat(
    State(fake): State<Shared>,
    headers: HeaderMap,
    axum::Json(body): axum::Json<Value>,
) -> Response {
    if !authorised(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let text = {
        let mut fake = fake.lock().unwrap();
        fake.seen.push(body.clone());
        fake.replies
            .pop_front()
            .expect("the fake model ran out of scripted replies")
    };
    if body["stream"] == json!(true) {
        let chunk = |delta: Value, finish: Value| {
            format!(
                "data: {}\n\n",
                json!({"id": "c1", "object": "chat.completion.chunk", "model": "fake-model",
                       "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]})
            )
        };
        let payload = format!(
            "{}{}data: [DONE]\n\n",
            chunk(json!({"role": "assistant", "content": text}), Value::Null),
            chunk(json!({}), json!("stop")),
        );
        return Response::builder()
            .header(header::CONTENT_TYPE, "text/event-stream")
            .body(Body::from(payload))
            .unwrap();
    }
    axum::Json(json!({
        "id": "c1", "object": "chat.completion", "model": "fake-model",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": text}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
    }))
    .into_response()
}

async fn serve_fake(replies: &[String]) -> (String, Shared) {
    let fake: Shared = Arc::new(Mutex::new(Fake {
        replies: replies.iter().cloned().collect(),
        seen: Vec::new(),
    }));
    let app = axum::Router::new()
        .route("/v1/models", get(fake_models))
        .route("/v1/chat/completions", post(fake_chat))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (base, fake)
}

fn state() -> rusty_brick_chess::api::AppState {
    rusty_brick_chess::api::AppState {
        db: rbc_db::Db::open_memory().expect("memory database"),
        db_dir: std::env::temp_dir(),
        data_dir: std::env::temp_dir(),
        engine_path: None,
        model_url: None,
        default_depth: 12,
        console: Default::default(),
        turn_cancels: Default::default(),
        lamu_child: Default::default(),
    }
}

async fn call(
    state: &rusty_brick_chess::api::AppState,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value, String) {
    let mut request = Request::builder().method(method).uri(path);
    let request = match body {
        Some(json) => {
            request = request.header("content-type", "application/json");
            request.body(Body::from(json.to_string())).unwrap()
        }
        None => request.body(Body::empty()).unwrap(),
    };
    let response = rusty_brick_chess::router(state.clone())
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).to_string();
    (
        status,
        serde_json::from_str(&text).unwrap_or(Value::Null),
        text,
    )
}

async fn register(
    state: &rusty_brick_chess::api::AppState,
    label: &str,
    base: &str,
    key_ref: Option<&str>,
) -> String {
    let (status, body, text) = call(
        state,
        "POST",
        "/api/providers",
        Some(json!({"label": label, "base_url": base, "key_ref": key_ref})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{text}");
    body["id"].as_str().unwrap().to_string()
}

async fn new_match(
    state: &rusty_brick_chess::api::AppState,
    seat_provider: &str,
    rated: bool,
) -> String {
    let (status, body, text) = call(
        state,
        "POST",
        "/api/ai/matches",
        Some(json!({"config": {
            "start_fen": "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "white": {"kind": "human", "name": "You"},
            "black": {"kind": "model", "provider": seat_provider, "model": "fake-model"},
            "max_plies": 300, "rated": rated
        }})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let id = body["id"].as_str().unwrap().to_string();
    let (status, _, text) = call(
        state,
        "POST",
        &format!("/api/ai/matches/{id}/move"),
        Some(json!({"mv": "e2e4"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{text}");
    id
}

fn coach_reply() -> String {
    "Provisional: e7e5\n\
     <tool_call>{\"name\":\"mark_arrow\",\"arguments\":{\"from\":\"e7\",\"to\":\"e5\",\"color\":\"red\"}}</tool_call>\n\
     <tool_call>{\"name\":\"mark_square\",\"arguments\":{\"square\":\"e4\",\"color\":\"blue\"}}</tool_call>\n\
     <tool_call>{\"name\":\"show_game_move\",\"arguments\":{\"ply\":1}}</tool_call>\n\
     I mirror the centre pawn."
        .to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_connection_test_returns_typed_errors() {
    unsafe { std::env::set_var(KEY_NAME, KEY_VALUE) };
    let (base, _) = serve_fake(&[]).await;
    let state = state();
    let good = register(&state, "Fake", &base, Some(KEY_NAME)).await;

    let (_, ok, _) = call(
        &state,
        "POST",
        &format!("/api/providers/{good}/test"),
        Some(json!({"model": "fake-model"})),
    )
    .await;
    assert_eq!(ok["ok"], true, "{ok}");
    assert_eq!(ok["error"], Value::Null);
    assert_eq!(ok["models"], 1);

    let (_, missing, _) = call(
        &state,
        "POST",
        &format!("/api/providers/{good}/test"),
        Some(json!({"model": "ghost"})),
    )
    .await;
    assert_eq!(missing["error"]["code"], "model_missing", "{missing}");

    let no_key = register(&state, "No key", &base, None).await;
    let (_, denied, _) = call(
        &state,
        "POST",
        &format!("/api/providers/{no_key}/test"),
        Some(json!({})),
    )
    .await;
    assert_eq!(denied["error"]["code"], "unauthorized", "{denied}");

    let unset = register(&state, "Unset", &base, Some("RBC_TEST_FAKE_NEVER_SET")).await;
    let (_, unresolved, _) = call(
        &state,
        "POST",
        &format!("/api/providers/{unset}/test"),
        Some(json!({})),
    )
    .await;
    assert_eq!(
        unresolved["error"]["code"], "key_unresolved",
        "{unresolved}"
    );

    let (_, gone, _) = call(
        &state,
        "POST",
        "/api/providers/probe",
        Some(json!({"base_url": "http://127.0.0.1:1/v1"})),
    )
    .await;
    assert_eq!(gone["error"]["code"], "unreachable", "{gone}");

    let (_, wrong_path, _) = call(
        &state,
        "POST",
        "/api/providers/probe",
        Some(json!({"base_url": format!("{base}/nope"), "key_ref": KEY_NAME})),
    )
    .await;
    assert_eq!(wrong_path["error"]["code"], "not_found", "{wrong_path}");

    let (_, bad_url, _) = call(
        &state,
        "POST",
        "/api/providers/probe",
        Some(json!({"base_url": "ftp://x"})),
    )
    .await;
    assert_eq!(bad_url["error"]["code"], "invalid_url");
}

#[tokio::test]
async fn a_key_value_in_place_of_a_reference_is_refused_and_never_echoed() {
    let state = state();
    let pasted = "sk-this-is-a-pasted-key-0123456789";
    let (status, _, text) = call(
        &state,
        "POST",
        "/api/providers",
        Some(json!({"label": "Oops", "base_url": "https://api.example.com/v1", "key_ref": pasted})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!text.contains(pasted), "{text}");
    assert!(text.contains("environment variable name"), "{text}");
    let (_, listed, _) = call(&state, "GET", "/api/providers", None).await;
    assert_eq!(listed["providers"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn the_knightlm_preset_fills_a_provider_row_with_the_local_chat_base() {
    let state = state();
    let (_, presets, _) = call(&state, "GET", "/api/providers/presets", None).await;
    assert_eq!(presets["presets"][0]["id"], "knightlm");
    let (status, row, text) = call(
        &state,
        "POST",
        "/api/providers",
        Some(json!({"preset": "knightlm"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{text}");
    assert_eq!(row["label"], "KnightLM server");
    assert!(row["base_url"].as_str().unwrap().ends_with("/v1"), "{row}");
    assert_eq!(row["key_ref"], Value::Null);
    let (status, _, _) = call(
        &state,
        "POST",
        "/api/providers",
        Some(json!({"preset": "nope"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn seats_are_chosen_per_side_and_a_dangling_provider_is_refused() {
    let state = state();
    let (base, _) = serve_fake(&[]).await;
    let id = register(&state, "Fake", &base, None).await;
    let seat = json!({"provider": format!("provider:{id}"), "model": "fake-model"});
    let (status, saved, text) = call(
        &state,
        "PUT",
        "/api/model-seats",
        Some(json!({"black": seat, "coach": seat, "chat": true})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{text}");
    assert_eq!(saved["white"], Value::Null);
    let (_, got, _) = call(&state, "GET", "/api/model-seats", None).await;
    assert_eq!(got["coach"]["model"], "fake-model");
    assert_eq!(got["chat"], true);
    let (status, _, _) = call(
        &state,
        "PUT",
        "/api/model-seats",
        Some(json!({"white": {"provider": "provider:missing", "model": "m"}})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chat_on_runs_a_coach_turn_and_chat_off_runs_the_play_harness() {
    unsafe { std::env::set_var(KEY_NAME, KEY_VALUE) };
    let play = rbc_ai::model_turn::format_play_completion("mirror", "e7e5");
    let (base, fake) = serve_fake(&[coach_reply(), play.clone(), play]).await;
    let state = state();
    let id = register(&state, "Fake", &base, Some(KEY_NAME)).await;
    let seat = format!("provider:{id}");

    // Chat on: a coach turn with the rolling summary and the recent messages.
    let game = new_match(&state, &seat, false).await;
    let history: Vec<Value> = (0..8)
        .map(|n| json!({"role": if n % 2 == 0 {"user"} else {"assistant"}, "text": format!("message {n}")}))
        .collect();
    let (status, turn, text) = call(
        &state,
        "POST",
        &format!("/api/ai/matches/{game}/model-move"),
        Some(json!({"chat": true, "summary": "earlier: opening talk", "history": history})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{text}");
    assert_eq!(turn["chat_on"], true);
    assert_eq!(turn["move"]["uci"], "e7e5", "{turn}");
    let tools = &turn["tools"];
    assert_eq!(tools["fallback"], false);
    assert_eq!(tools["reply"], "I mirror the centre pawn.");
    // The structured effects the board draws, in call order.
    let effects: Vec<&str> = tools["ui"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["effect"].as_str().unwrap())
        .collect();
    assert_eq!(effects, ["add_mark", "add_mark", "show_ply"], "{tools}");
    assert_eq!(
        tools["ui"][0]["mark"],
        json!({"kind": "arrow", "from": "e7", "to": "e5", "color": "red"})
    );
    assert_eq!(
        tools["ui"][1]["mark"],
        json!({"kind": "square", "square": "e4", "color": "blue"})
    );
    assert_eq!(tools["ui"][2]["ply"], 1);
    assert_eq!(tools["marks"].as_array().unwrap().len(), 2);
    // The chat rolled: the oldest messages folded into the summary.
    assert_eq!(turn["chat"]["recent"].as_array().unwrap().len(), 6);
    assert!(
        turn["chat"]["summary"]
            .as_str()
            .unwrap()
            .contains("opening talk"),
        "{turn}"
    );
    // What the model saw: a coach prompt, the summary, and the key as a bearer.
    {
        let fake = fake.lock().unwrap();
        let prompt = fake.seen[0]["messages"][0]["content"].as_str().unwrap();
        assert!(
            prompt.contains("Conversation summary: earlier: opening talk"),
            "{prompt}"
        );
        assert!(prompt.contains("message 7"), "{prompt}");
        assert!(prompt.contains("Position packet"), "{prompt}");
        assert_eq!(fake.seen[0]["model"], "fake-model");
    }

    // Chat off: the competition harness, no reply, no marks.
    let game = new_match(&state, &seat, false).await;
    let (status, turn, text) = call(
        &state,
        "POST",
        &format!("/api/ai/matches/{game}/model-move"),
        Some(json!({"chat": false})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{text}");
    assert_eq!(turn["chat_on"], false);
    assert_eq!(turn["move"]["uci"], "e7e5");
    assert_eq!(turn["tools"]["reply"], Value::Null);
    assert_eq!(turn["tools"]["marks"].as_array().unwrap().len(), 0);
    {
        let fake = fake.lock().unwrap();
        let prompt = fake.seen[1]["messages"][0]["content"].as_str().unwrap();
        assert!(prompt.contains("competition mode"), "{prompt}");
    }

    // A rated game never chats, whatever the toggle says.
    let rated = new_match(&state, &seat, true).await;
    let (_, turn, text) = call(
        &state,
        "POST",
        &format!("/api/ai/matches/{rated}/model-move"),
        Some(json!({"chat": true})),
    )
    .await;
    assert_eq!(turn["chat_on"], false, "{text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_coach_seat_answers_chat_messages_and_the_chat_rolls() {
    let (base, fake) = serve_fake(&["Develop a knight.".to_string()]).await;
    let state = state();
    let id = register(&state, "Fake", &base, None).await;
    // This fake wants a key; register one that resolves.
    unsafe { std::env::set_var(KEY_NAME, KEY_VALUE) };
    let keyed = register(&state, "Keyed", &base, Some(KEY_NAME)).await;
    let _ = id;
    let coach = json!({"provider": format!("provider:{keyed}"), "model": "fake-model"});
    let game = new_match(&state, &format!("provider:{keyed}"), false).await;
    let (status, reply, text) = call(
        &state,
        "POST",
        &format!("/api/ai/matches/{game}/chat"),
        Some(
            json!({"message": "what now?", "coach": coach, "summary": "we talked", "history": []}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{text}");
    assert_eq!(reply["reply"], "Develop a knight.");
    assert_eq!(reply["chat"]["recent"].as_array().unwrap().len(), 2);
    assert_eq!(reply["chat"]["summary"], "we talked");
    let sent = fake.lock().unwrap().seen[0]["messages"][1]["content"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        sent.contains("Conversation summary: we talked") && sent.contains("what now?"),
        "{sent}"
    );
}
