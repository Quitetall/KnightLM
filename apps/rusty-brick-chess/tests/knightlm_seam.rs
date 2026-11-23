//! The app against a real KnightLM server over the seam contract (docs/model/seam.md).
//!
//! The server is `python -m model.serve.fake_server`: the real serving app (`model/serve/server.py`)
//! over a predictor that always answers its first legal move (no model, no torch). It speaks the
//! same `/v1/chat/completions` contract a trained checkpoint does, including its 400 and 422
//! refusals of a request that is not the contract's. The router runs in-process, as in
//! `model_wiring.rs`.
//!
//! Needs Python with fastapi, uvicorn and httpx: `KNIGHTLM_FAKE_SERVER_PYTHON` names it, else
//! `python3`. Without them the tests skip, unless `RBC_REQUIRE_FAKE_SERVER=1` (CI sets it).

use std::collections::VecDeque;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

/// A running fake server; killed on drop.
struct FakeServer {
    child: Child,
    base: String,
}

impl Drop for FakeServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Start the fake server, or `None` when Python lacks its dependencies and skipping is allowed.
fn fake_server(illegal: bool) -> Option<FakeServer> {
    let python = std::env::var("KNIGHTLM_FAKE_SERVER_PYTHON").unwrap_or_else(|_| "python3".into());
    let required = std::env::var("RBC_REQUIRE_FAKE_SERVER").is_ok_and(|v| v == "1");
    let ready = Command::new(&python)
        .args(["-c", "import fastapi, uvicorn, httpx"])
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !ready {
        assert!(
            !required,
            "RBC_REQUIRE_FAKE_SERVER=1 but `{python}` lacks fastapi, uvicorn or httpx"
        );
        eprintln!("skipped: `{python}` lacks fastapi, uvicorn or httpx");
        return None;
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let port = free_port();
    let mut command = Command::new(&python);
    command
        .args(["-m", "model.serve.fake_server", "--port", &port.to_string()])
        .current_dir(&root)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if illegal {
        command.arg("--illegal");
    }
    let child = command.spawn().expect("start the fake KnightLM server");
    let server = FakeServer {
        child,
        base: format!("http://127.0.0.1:{port}/v1"),
    };
    for _ in 0..150 {
        if ureq::get(format!("http://127.0.0.1:{port}/v1/health"))
            .call()
            .is_ok()
        {
            return Some(server);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    panic!("the fake KnightLM server did not start");
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
) -> (StatusCode, Value) {
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
        serde_json::from_str(&text).unwrap_or(Value::String(text)),
    )
}

/// Register the server through the KnightLM preset (only the base URL is ours).
async fn register_knightlm(state: &rusty_brick_chess::api::AppState, base: &str) -> String {
    let (status, row) = call(
        state,
        "POST",
        "/api/providers",
        Some(json!({"preset": "knightlm", "base_url": base})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{row}");
    assert_eq!(row["label"], "KnightLM server");
    assert_eq!(row["kind"], "knightlm", "the preset stores the kind");
    row["id"].as_str().unwrap().to_string()
}

/// A human-versus-model match with the model as black, after the human's 1. e4.
async fn new_match(state: &rusty_brick_chess::api::AppState, provider: &str) -> String {
    let (status, body) = call(
        state,
        "POST",
        "/api/ai/matches",
        Some(json!({"config": {
            "start_fen": "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "white": {"kind": "human", "name": "You"},
            "black": {"kind": "model", "provider": provider, "model": "knightlm"},
            "max_plies": 300, "rated": false
        }})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let id = body["id"].as_str().unwrap().to_string();
    human_moves(state, &id, "e2e4").await;
    id
}

async fn human_moves(state: &rusty_brick_chess::api::AppState, id: &str, mv: &str) {
    let (status, body) = call(
        state,
        "POST",
        &format!("/api/ai/matches/{id}/move"),
        Some(json!({"mv": mv})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

async fn plies(state: &rusty_brick_chess::api::AppState, id: &str) -> usize {
    let (_, game) = call(state, "GET", &format!("/api/ai/matches/{id}"), None).await;
    game["moves"].as_array().map_or(0, Vec::len)
}

async fn model_move(
    state: &rusty_brick_chess::api::AppState,
    id: &str,
    options: Value,
) -> (StatusCode, Value) {
    call(
        state,
        "POST",
        &format!("/api/ai/matches/{id}/model-move"),
        Some(options),
    )
    .await
}

fn assert_legal_black_reply(turn: &Value) -> String {
    let uci = turn["move"]["uci"].as_str().expect("a move").to_string();
    let legal = turn["match"]["legal_moves"].as_array().unwrap();
    assert_eq!(
        turn["match"]["turn"], "white",
        "the move was played: {turn}"
    );
    assert!(!legal.is_empty());
    uci
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_knightlm_server_plays_a_game_with_chat_off_and_on() {
    let Some(server) = fake_server(false) else {
        return;
    };
    let state = state();
    let id = register_knightlm(&state, &server.base).await;

    // The connection test recognises the server, and a row stays a KnightLM row.
    let (_, probe) = call(
        &state,
        "POST",
        &format!("/api/providers/{id}/test"),
        Some(json!({"model": "knightlm"})),
    )
    .await;
    assert_eq!(probe["ok"], true, "{probe}");
    assert_eq!(probe["knightlm"], true, "{probe}");

    let game = new_match(&state, &format!("provider:{id}")).await;

    // Chat off: the contract's play request, the submit_move call gated against the position.
    let (status, turn) = model_move(&state, &game, json!({"chat": false})).await;
    assert_eq!(status, StatusCode::OK, "{turn}");
    assert_eq!(turn["chat_on"], false);
    assert_legal_black_reply(&turn);
    assert_eq!(turn["tools"]["fallback"], false, "{turn}");
    assert_eq!(turn["tools"]["rejected"], 0);
    assert_eq!(plies(&state, &game).await, 2);

    // Chat on: a coach turn. The server read the `Provisional:` line, the reply comes back with
    // the board effect of its coach tool call.
    human_moves(&state, &game, "g1f3").await;
    let history = json!([
        {"role": "user", "text": "Why not Nf3?"},
        {"role": "assistant", "text": "It blocks the f-pawn."}
    ]);
    let (status, turn) = model_move(
        &state,
        &game,
        json!({"chat": true, "summary": "We discussed the Ruy Lopez.", "history": history}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{turn}");
    assert_eq!(turn["chat_on"], true);
    assert_legal_black_reply(&turn);
    let tools = &turn["tools"];
    assert_eq!(tools["fallback"], false, "{turn}");
    assert!(
        tools["reply"].as_str().unwrap().contains("Fake coach"),
        "{turn}"
    );
    assert_eq!(
        tools["marks"],
        json!([{"kind": "square", "square": "e4", "color": "blue"}])
    );
    assert_eq!(tools["ui"][0]["effect"], "add_mark");
    // The reply joined the rolled chat the client keeps.
    assert_eq!(turn["chat"]["recent"].as_array().unwrap().len(), 3);
    assert_eq!(plies(&state, &game).await, 4);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_server_added_without_the_preset_is_recognised_by_the_connection_test() {
    let Some(server) = fake_server(false) else {
        return;
    };
    let state = state();
    let (status, row) = call(
        &state,
        "POST",
        "/api/providers",
        Some(json!({"label": "My server", "base_url": server.base})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{row}");
    assert_eq!(row["kind"], "openai_compatible");
    let id = row["id"].as_str().unwrap();
    let (_, probe) = call(&state, "POST", &format!("/api/providers/{id}/test"), None).await;
    assert_eq!(probe["knightlm"], true, "{probe}");
    let (_, listed) = call(&state, "GET", "/api/providers", None).await;
    assert_eq!(listed["providers"][0]["kind"], "knightlm", "{listed}");

    let game = new_match(&state, &format!("provider:{id}")).await;
    let (status, turn) = model_move(&state, &game, json!({"chat": false})).await;
    assert_eq!(status, StatusCode::OK, "{turn}");
    assert_legal_black_reply(&turn);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_illegal_answer_is_refused_and_the_match_is_untouched() {
    let Some(server) = fake_server(true) else {
        return;
    };
    let state = state();
    let id = register_knightlm(&state, &server.base).await;
    let game = new_match(&state, &format!("provider:{id}")).await;
    for options in [json!({"chat": false}), json!({"chat": true})] {
        let (status, body) = model_move(&state, &game, options).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(
            plies(&state, &game).await,
            1,
            "no illegal move was recorded"
        );
    }
}

// --- a generic OpenAI-style provider keeps the generic prompts ----------------------------

#[derive(Default)]
struct Generic {
    replies: VecDeque<String>,
    seen: Vec<Value>,
}

type Shared = Arc<Mutex<Generic>>;

async fn generic_models() -> Response {
    axum::Json(json!({"object": "list", "data": [{"id": "gpt-ish", "owned_by": "openai"}]}))
        .into_response()
}

async fn generic_chat(
    State(shared): State<Shared>,
    axum::Json(body): axum::Json<Value>,
) -> Response {
    let text = {
        let mut shared = shared.lock().unwrap();
        shared.seen.push(body);
        shared.replies.pop_front().expect("scripted reply")
    };
    axum::Json(json!({
        "id": "c1", "object": "chat.completion", "model": "gpt-ish",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": text}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
    }))
    .into_response()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_generic_openai_provider_keeps_the_generic_prompts() {
    let play = rbc_ai::model_turn::format_play_completion("mirror", "e7e5");
    let shared: Shared = Arc::new(Mutex::new(Generic {
        replies: [
            play,
            "Provisional: e7e5\nI mirror the centre pawn.".to_string(),
        ]
        .into(),
        seen: Vec::new(),
    }));
    let app = axum::Router::new()
        .route("/v1/models", get(generic_models))
        .route("/v1/chat/completions", post(generic_chat))
        .with_state(shared.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let state = state();
    let (_, row) = call(
        &state,
        "POST",
        "/api/providers",
        Some(json!({"label": "Generic", "base_url": base})),
    )
    .await;
    let id = row["id"].as_str().unwrap();
    let (_, probe) = call(&state, "POST", &format!("/api/providers/{id}/test"), None).await;
    assert_eq!(probe["ok"], true, "{probe}");
    assert_eq!(probe["knightlm"], false, "{probe}");
    let (_, listed) = call(&state, "GET", "/api/providers", None).await;
    assert_eq!(listed["providers"][0]["kind"], "openai_compatible");

    let seat = format!("provider:{id}");
    let off = new_match(&state, &seat).await;
    let (status, turn) = model_move(&state, &off, json!({"chat": false})).await;
    assert_eq!(status, StatusCode::OK, "{turn}");
    assert_eq!(turn["move"]["uci"], "e7e5");
    let on = new_match(&state, &seat).await;
    let (status, turn) = model_move(&state, &on, json!({"chat": true})).await;
    assert_eq!(status, StatusCode::OK, "{turn}");
    assert_eq!(turn["tools"]["reply"], "I mirror the centre pawn.");

    let seen = shared.lock().unwrap().seen.clone();
    assert_eq!(seen.len(), 2);
    for body in &seen {
        assert!(
            body.get("knightlm").is_none(),
            "no extension for a generic provider: {body}"
        );
    }
    let play_prompt = seen[0]["messages"][0]["content"].as_str().unwrap();
    assert!(play_prompt.starts_with(rbc_ai::model_turn::PLAY_INSTRUCTION));
    assert!(
        !play_prompt.contains("\"legal_moves\""),
        "the generic path never carried the list"
    );
    let coach_prompt = seen[1]["messages"][0]["content"].as_str().unwrap();
    assert!(coach_prompt.starts_with(rbc_ai::model_turn::COACH_INSTRUCTION));
}
