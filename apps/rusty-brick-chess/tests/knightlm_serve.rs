//! A match seat can be the served KnightLM checkpoint (`local:knightlm-serve`).
//!
//! The model server is replaced by a loopback stub speaking the same
//! `POST /v1/move` contract, so this checks what the app sends (the canonical
//! packet, constrained mode), what it does with the answer, and that an illegal
//! answer never reaches the board. The router runs in-process, as in
//! `providers.rs`. One test function: the endpoint comes from an environment
//! variable, which is process-global.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
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
        tournament_actions: Default::default(),
    }
}

async fn call(
    state: &rusty_brick_chess::api::AppState,
    path: &str,
    body: &str,
) -> (StatusCode, serde_json::Value) {
    let request = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("request");
    let response = rusty_brick_chess::router(state.clone())
        .oneshot(request)
        .await
        .expect("response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| serde_json::Value::String(String::from_utf8_lossy(&bytes).into()));
    (status, value)
}

/// Serve `replies` in order on a loopback port; return the URL and the bodies seen.
fn stub(replies: Vec<String>) -> (String, Arc<Mutex<Vec<serde_json::Value>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://{}/v1/move", listener.local_addr().expect("addr"));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for reply in replies {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buffer = Vec::new();
            let mut chunk = [0u8; 4096];
            let (head_end, length) = loop {
                let n = stream.read(&mut chunk).expect("read");
                buffer.extend_from_slice(&chunk[..n]);
                if let Some(end) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&buffer[..end]).to_lowercase();
                    let length = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    break (end + 4, length);
                }
            };
            while buffer.len() < head_end + length {
                let n = stream.read(&mut chunk).expect("read body");
                buffer.extend_from_slice(&chunk[..n]);
            }
            let body: serde_json::Value =
                serde_json::from_slice(&buffer[head_end..head_end + length]).expect("json body");
            log.lock().expect("log").push(body);
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                reply.len(),
                reply
            );
        }
    });
    (url, seen)
}

const NEW_MATCH: &str = r#"{"config":{
    "start_fen":"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "white":{"kind":"human","name":"me"},
    "black":{"kind":"model","provider":"local:knightlm-serve","model":"knightlm-serve"},
    "max_plies":200}}"#;

#[tokio::test]
async fn the_served_checkpoint_plays_a_seat_and_an_illegal_answer_never_reaches_the_board() {
    let state = state();
    let (url, seen) = stub(vec![
        r#"{"move":"e7e5","legal":true,"raw_reply":"e7e5","input_tokens":1,"output_tokens":1,"latency_ms":1.0}"#.into(),
        r#"{"move":"e2e4","legal":false,"raw_reply":"e2e4"}"#.into(),
    ]);
    // SAFETY: the only test in this binary; read by the endpoint resolver.
    unsafe { std::env::set_var("RBC_KNIGHTLM_SERVE_URL", &url) };

    let (status, created) = call(&state, "/api/ai/matches", NEW_MATCH).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let id = created["id"].as_str().expect("match id").to_owned();
    let (status, after) = call(
        &state,
        &format!("/api/ai/matches/{id}/move"),
        r#"{"mv":"e2e4"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{after}");

    // The model answers a legal move: it is applied.
    let (status, played) = call(&state, &format!("/api/ai/matches/{id}/model-move"), "{}").await;
    assert_eq!(status, StatusCode::OK, "{played}");
    assert_eq!(played["move"]["uci"], "e7e5", "{played}");
    assert_eq!(played["match"]["moves"].as_array().map(Vec::len), Some(2));

    // What the server was sent: the canonical packet with the legal list, constrained.
    let request = seen.lock().expect("log")[0].clone();
    assert_eq!(request["mode"], "constrained");
    assert_eq!(
        request["packet_format_version"],
        request["packet"]["schema_version"]
    );
    let choices = request["packet"]["legal_moves"].to_string();
    assert!(
        choices.contains("e7e5") && choices.contains("g8f6"),
        "{choices}"
    );

    // Now it is white's turn again; play, then the stub answers an illegal move
    // (it is white's, not black's): the app refuses it and records nothing.
    let (status, _) = call(
        &state,
        &format!("/api/ai/matches/{id}/move"),
        r#"{"mv":"g1f3"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, refused) = call(&state, &format!("/api/ai/matches/{id}/model-move"), "{}").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    let (_, game) = {
        let request = Request::builder()
            .uri(format!("/api/ai/matches/{id}"))
            .body(Body::empty())
            .expect("request");
        let response = rusty_brick_chess::router(state.clone())
            .oneshot(request)
            .await
            .expect("response");
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        (
            StatusCode::OK,
            serde_json::from_slice::<serde_json::Value>(&bytes).expect("json"),
        )
    };
    assert_eq!(game["moves"].as_array().map(Vec::len), Some(3), "{game}");
    unsafe { std::env::remove_var("RBC_KNIGHTLM_SERVE_URL") };
}
