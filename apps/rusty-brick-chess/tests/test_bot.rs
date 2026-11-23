//! The built-in test bot (`local:rbc-test-bot`): the whole play, chat and watch path with no
//! Python, no model, no GPU and no network. It is a test bot, and says so.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

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

const BOT: &str = "local:rbc-test-bot";

fn bot_seat() -> Value {
    json!({"kind": "model", "provider": BOT, "model": "rbc-test-bot"})
}

async fn new_match(state: &rusty_brick_chess::api::AppState, white: Value, black: Value) -> String {
    let (status, body) = call(
        state,
        "POST",
        "/api/ai/matches",
        Some(json!({"config": {
            "start_fen": "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "white": white, "black": black, "max_plies": 300, "rated": false
        }})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["id"].as_str().unwrap().to_string()
}

async fn plies(state: &rusty_brick_chess::api::AppState, id: &str) -> usize {
    let (_, game) = call(state, "GET", &format!("/api/ai/matches/{id}"), None).await;
    game["moves"].as_array().map_or(0, Vec::len)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_test_bot_is_labelled_and_always_available() {
    let state = state();
    let (status, info) = call(&state, "GET", "/api/ai/test-bot", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(info["provider"], BOT);
    assert_eq!(info["is_model"], false);
    assert_eq!(info["available"], true);
    assert!(info["label"].as_str().unwrap().contains("Test bot"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_test_bot_plays_and_talks_with_chat_off_and_on() {
    let state = state();
    let human = json!({"kind": "human", "name": "You"});
    let game = new_match(&state, human, bot_seat()).await;
    let (status, _) = call(
        &state,
        "POST",
        &format!("/api/ai/matches/{game}/move"),
        Some(json!({"mv": "e2e4"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Chat off: a legal move, labelled as the bot's own (no fallback).
    let (status, turn) = call(
        &state,
        "POST",
        &format!("/api/ai/matches/{game}/model-move"),
        Some(json!({"chat": false})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{turn}");
    assert_eq!(turn["match"]["turn"], "white", "{turn}");
    assert_eq!(turn["tools"]["fallback"], false);
    assert_eq!(plies(&state, &game).await, 2);

    // Chat on: the reply is honest, the board mark is drawn, and the move is legal.
    let (status, _) = call(
        &state,
        "POST",
        &format!("/api/ai/matches/{game}/move"),
        Some(json!({"mv": "g1f3"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, turn) = call(
        &state,
        "POST",
        &format!("/api/ai/matches/{game}/model-move"),
        Some(json!({"chat": true, "history": [{"role": "user", "text": "Are you strong?"}]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{turn}");
    let reply = turn["tools"]["reply"].as_str().unwrap();
    assert!(reply.contains("test bot"), "{reply}");
    assert!(reply.contains("random"), "{reply}");
    assert!(!reply.contains("Provisional"), "{reply}");
    assert_eq!(
        turn["tools"]["marks"].as_array().unwrap().len(),
        1,
        "{turn}"
    );
    assert_eq!(plies(&state, &game).await, 4);

    // Free chat: a reply, and no move.
    let (status, chat) = call(
        &state,
        "POST",
        &format!("/api/ai/matches/{game}/chat"),
        Some(json!({"message": "Why that move?"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{chat}");
    let reply = chat["reply"].as_str().unwrap();
    assert!(
        reply.contains("test bot") && reply.contains("Why that move?"),
        "{reply}"
    );
    assert_eq!(plies(&state, &game).await, 4);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_test_bots_play_a_watched_game_of_legal_moves() {
    let state = state();
    let game = new_match(&state, bot_seat(), bot_seat()).await;
    let (status, run) = call(
        &state,
        "POST",
        &format!("/api/ai/matches/{game}/watch/step"),
        Some(json!({"plies": 12})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{run}");
    assert_eq!(run["steps"].as_array().unwrap().len(), 12, "{run}");
    assert_eq!(plies(&state, &game).await, 12);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_competitive_game_refuses_test_bot_chat() {
    let state = state();
    let (status, body) = call(
        &state,
        "POST",
        "/api/ai/matches",
        Some(json!({"config": {
            "start_fen": "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "white": {"kind": "human", "name": "You"}, "black": bot_seat(),
            "max_plies": 300, "rated": true
        }})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let game = body["id"].as_str().unwrap();
    let (status, _) = call(
        &state,
        "POST",
        &format!("/api/ai/matches/{game}/chat"),
        Some(json!({"message": "hi"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
