//! The provider registry's one non-negotiable invariant: a secret never lands in
//! the database, in a response, or in an audit row.
//!
//! These drive the real router in-process. An earlier version spawned the server
//! and spoke HTTP to it, which is closer to production but made the suite a
//! hostage to the machine: five children each opening SQLite ended up in
//! uninterruptible disk wait while another project's build saturated the volume,
//! and the tests timed out for reasons that had nothing to do with the code. The
//! router under `oneshot` exercises the same handlers, extractors and policies
//! without a socket.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

const SECRET: &str = "sk-not-a-real-key-but-shaped-like-one";
const KEY_REF: &str = "RBC_TEST_PROVIDER_KEY";
const OTHER_KEY_REF: &str = "RBC_TEST_PROVIDER_KEY_FOR_DB";

fn state() -> rusty_brick_chess::api::AppState {
    let db = rbc_db::Db::open_memory().expect("memory database");
    rusty_brick_chess::api::AppState {
        db,
        db_dir: std::env::temp_dir(),
        data_dir: std::env::temp_dir(),
        engine_path: None,
        model_url: None,
        default_depth: 12,
        console: Default::default(),
        turn_cancels: Default::default(),
    }
}

async fn call(
    state: rusty_brick_chess::api::AppState,
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
    let response = rusty_brick_chess::router(state)
        .oneshot(request)
        .await
        .expect("response");
    let status = response.status();
    let bytes = response.into_body().collect().await.expect("body").to_bytes();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

async fn add(state: rusty_brick_chess::api::AppState, json: &str) -> (StatusCode, String) {
    call(state, "POST", "/api/providers", Some(json)).await
}

#[tokio::test]
async fn a_key_reference_is_stored_but_the_key_it_names_never_is() {
    // SAFETY: set and removed within one test, read only by the resolver.
    unsafe { std::env::set_var(KEY_REF, SECRET) };
    let state = state();
    let (status, _) = add(
        state.clone(),
        r#"{"label":"Local","base_url":"http://127.0.0.1:9/v1","key_ref":"RBC_TEST_PROVIDER_KEY"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = call(state, "GET", "/api/providers", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(KEY_REF), "the reference is returned: {body}");
    assert!(!body.contains(SECRET), "the response carries the secret: {body}");
    assert!(
        body.contains("\"key_resolves\":true"),
        "and it reports that the reference resolves: {body}"
    );
    unsafe { std::env::remove_var(KEY_REF) };
}

#[tokio::test]
async fn an_unresolved_reference_is_reported_as_unresolved_not_as_an_error() {
    let state = state();
    let (status, _) = add(
        state.clone(),
        r#"{"label":"Cloud","base_url":"https://api.example.com/v1","key_ref":"RBC_NOT_SET_ANYWHERE"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = call(state, "GET", "/api/providers", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("\"key_resolves\":false"), "{body}");
    assert!(body.contains("unresolved"), "{body}");
}

#[tokio::test]
async fn the_secret_reaches_no_row_of_the_database() {
    unsafe { std::env::set_var(OTHER_KEY_REF, SECRET) };
    let state = state();
    add(
        state.clone(),
        r#"{"label":"Local","base_url":"http://127.0.0.1:9/v1","key_ref":"RBC_TEST_PROVIDER_KEY_FOR_DB"}"#,
    )
    .await;
    let rows = state.db.list_providers().expect("providers");
    let serialised = serde_json::to_string(&rows).expect("json");
    assert!(serialised.contains(OTHER_KEY_REF));
    assert!(!serialised.contains(SECRET), "a provider row holds the secret");
    let audit = serde_json::to_string(&state.db.audit_log(100).expect("audit")).expect("json");
    assert!(!audit.contains(SECRET), "the audit log holds the secret");
    unsafe { std::env::remove_var(OTHER_KEY_REF) };
}

#[tokio::test]
async fn a_base_url_that_is_not_http_is_refused() {
    let (status, body) = add(state(), r#"{"label":"Bad","base_url":"file:///etc/passwd"}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.contains("http://"), "and it says what is allowed: {body}");
}

#[tokio::test]
async fn a_label_is_required_and_a_duplicate_is_refused() {
    let state = state();
    let (status, _) = add(state.clone(), r#"{"label":"  ","base_url":"http://127.0.0.1:9/v1"}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (first, _) = add(state.clone(), r#"{"label":"One","base_url":"http://127.0.0.1:9/v1"}"#).await;
    assert_eq!(first, StatusCode::OK);
    let (second, _) = add(state, r#"{"label":"One","base_url":"http://127.0.0.1:9/v2"}"#).await;
    assert_ne!(second, StatusCode::OK, "two providers cannot share a label");
}

#[tokio::test]
async fn a_provider_that_is_not_there_is_a_404_not_a_panic() {
    let (status, _) = call(state(), "POST", "/api/providers/nope/test", Some("{}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_local_scan_answers_even_when_nothing_is_running() {
    let (status, body) = call(state(), "GET", "/api/providers/scan", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // Every candidate is listed whether or not it answered, so a person sees what
    // was tried rather than an empty list when a server is merely asleep.
    assert!(body.contains("Ollama"), "{body}");
}

#[tokio::test]
async fn deleting_a_provider_removes_it_and_answers_once() {
    let state = state();
    let (status, body) = add(state.clone(), r#"{"label":"Gone","base_url":"http://127.0.0.1:9/v1"}"#).await;
    assert_eq!(status, StatusCode::OK);
    let id = serde_json::from_str::<serde_json::Value>(&body)
        .expect("json")
        .get("id")
        .and_then(|id| id.as_str())
        .expect("id")
        .to_string();
    let (first, _) = call(state.clone(), "DELETE", &format!("/api/providers/{id}"), None).await;
    assert_eq!(first, StatusCode::OK);
    let (second, _) = call(state, "DELETE", &format!("/api/providers/{id}"), None).await;
    assert_eq!(second, StatusCode::NOT_FOUND, "the second delete has nothing to do");
}
