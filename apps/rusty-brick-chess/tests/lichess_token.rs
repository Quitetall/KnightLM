//! The Lichess token is a reference, never a stored or returned value.
//! Drives the real router in-process (no socket, no network).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

const SECRET: &str = "lip_not-a-real-token-for-tests-0123";

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
    method: &str,
    path: &str,
    body: Option<&str>,
) -> (StatusCode, String) {
    let mut request = Request::builder().method(method).uri(path);
    let request = match body {
        Some(json) => {
            request = request.header("content-type", "application/json");
            request.body(Body::from(json.to_string())).expect("request")
        }
        None => request.body(Body::empty()).expect("request"),
    };
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
    (status, String::from_utf8_lossy(&bytes).to_string())
}

#[tokio::test]
async fn a_pasted_token_is_refused_and_never_stored_or_echoed() {
    let state = state();
    let body = format!(r#"{{"provider":"lichess","username":"someone","token_ref":"{SECRET}"}}"#);
    let (status, text) = call(&state, "POST", "/api/accounts/link", Some(&body)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{text}");
    assert!(!text.contains(SECRET), "{text}");
    let stored = format!("{:?}", state.db.settings().expect("settings"));
    assert!(!stored.contains(SECRET), "{stored}");
}

#[tokio::test]
async fn an_unresolvable_reference_is_refused_without_storing_anything() {
    let state = state();
    let body =
        r#"{"provider":"lichess","username":"someone","token_ref":"RBC_TEST_LICHESS_API_UNSET"}"#;
    let (status, text) = call(&state, "POST", "/api/accounts/link", Some(body)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{text}");
    assert!(text.contains("does not resolve"), "{text}");
    assert!(state.db.settings().expect("settings").is_empty());
}

#[tokio::test]
async fn no_response_carries_a_legacy_or_referenced_token() {
    let state = state();
    state
        .db
        .set_setting("lichess_user", "someone")
        .expect("user");
    state
        .db
        .set_setting("lichess_token", SECRET)
        .expect("legacy");
    for path in ["/api/accounts", "/api/settings"] {
        let (status, text) = call(&state, "GET", path, None).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert!(!text.contains(SECRET), "{path}: {text}");
    }
    let (_, accounts) = call(&state, "GET", "/api/accounts", None).await;
    assert!(accounts.contains(r#""has_token":true"#), "{accounts}");

    // Unlinking removes the row itself, not a blank tombstone.
    let (status, text) = call(
        &state,
        "POST",
        "/api/accounts/unlink",
        Some(r#"{"provider":"lichess"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!text.contains(SECRET));
    assert!(
        !state
            .db
            .settings()
            .expect("settings")
            .contains_key("lichess_token")
    );
}
