//! Per-launch access token for the desktop shell.
//!
//! The desktop app binds a random loopback port, but any local process can
//! connect to a loopback port. The shell therefore mints a random token per
//! launch and the server refuses every request that does not carry it.
//!
//! The window opens `/?rbc_token=<token>` once; the server answers with a
//! `HttpOnly; SameSite=Strict` cookie and redirects to the clean URL, so the
//! page, its fetches and its event streams are all authorised without any
//! frontend change. Programmatic callers (the shell itself) send the
//! `x-rbc-token` header. Browser mode passes no token and is unaffected.

use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::Response,
};

pub const TOKEN_HEADER: &str = "x-rbc-token";
pub const TOKEN_PARAM: &str = "rbc_token";
pub const TOKEN_COOKIE: &str = "rbc_token";

/// A fresh 256-bit token as 64 hex characters, from the OS random source.
pub fn new_token() -> std::io::Result<String> {
    use std::io::Read;
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .or_else(|_| fallback_random(&mut bytes))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(windows)]
fn fallback_random(bytes: &mut [u8; 32]) -> std::io::Result<()> {
    // std's HashMap seeds come from the OS CSPRNG (RtlGenRandom/BCryptGenRandom).
    use std::hash::{BuildHasher, Hasher};
    for chunk in bytes.chunks_mut(8) {
        let value = std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish();
        chunk.copy_from_slice(&value.to_le_bytes()[..chunk.len()]);
    }
    Ok(())
}

#[cfg(not(windows))]
fn fallback_random(_: &mut [u8; 32]) -> std::io::Result<()> {
    Err(std::io::Error::other("no OS random source available"))
}

fn same(a: &str, b: &str) -> bool {
    // Constant time over the token length; both are hex of fixed length.
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn cookie_token(request: &Request) -> Option<&str> {
    request
        .headers()
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == TOKEN_COOKIE)
        .map(|(_, value)| value)
}

fn query_token(query: &str) -> Option<&str> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(name, _)| *name == TOKEN_PARAM)
        .map(|(_, value)| value)
}

fn refuse() -> Response {
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"error":"missing or invalid launch token"}"#))
        .expect("static response")
}

/// Middleware: only requests carrying the launch token get through.
pub async fn require_token(
    State(token): State<std::sync::Arc<String>>,
    request: Request,
    next: Next,
) -> Response {
    let header_ok = request
        .headers()
        .get(TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|given| same(given, &token));
    if header_ok || cookie_token(&request).is_some_and(|given| same(given, &token)) {
        return next.run(request).await;
    }
    // First navigation of the window: trade the query token for a cookie.
    let query = request.uri().query().unwrap_or("");
    if request.method() == axum::http::Method::GET
        && query_token(query).is_some_and(|given| same(given, &token))
    {
        let rest: Vec<&str> = query
            .split('&')
            .filter(|pair| !pair.starts_with(&format!("{TOKEN_PARAM}=")))
            .collect();
        let mut location = request.uri().path().to_string();
        if !rest.is_empty() {
            location.push('?');
            location.push_str(&rest.join("&"));
        }
        let cookie = format!(
            "{TOKEN_COOKIE}={}; Path=/; HttpOnly; SameSite=Strict",
            token
        );
        if let (Ok(location), Ok(cookie)) = (
            HeaderValue::from_str(&location),
            HeaderValue::from_str(&cookie),
        ) {
            return Response::builder()
                .status(StatusCode::SEE_OTHER)
                .header(header::LOCATION, location)
                .header(header::SET_COOKIE, cookie)
                .body(Body::empty())
                .expect("redirect response");
        }
    }
    refuse()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, routing::get};
    use tower::ServiceExt;

    fn app(token: &str) -> Router {
        let token = std::sync::Arc::new(token.to_string());
        Router::new()
            .route("/api/health", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(token, require_token))
    }

    async fn status(app: Router, request: Request<Body>) -> (StatusCode, axum::http::HeaderMap) {
        let response = app.oneshot(request).await.unwrap();
        (response.status(), response.headers().clone())
    }

    fn get_req(uri: &str) -> axum::http::request::Builder {
        Request::builder().uri(uri)
    }

    #[test]
    fn tokens_are_random_hex() {
        let a = new_token().unwrap();
        let b = new_token().unwrap();
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }

    #[tokio::test]
    async fn refuses_without_token_and_with_wrong_token() {
        let t = "a".repeat(64);
        let (s, _) = status(app(&t), get_req("/api/health").body(Body::empty()).unwrap()).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        let wrong = "b".repeat(64);
        for request in [
            get_req("/api/health")
                .header(TOKEN_HEADER, &wrong)
                .body(Body::empty())
                .unwrap(),
            get_req("/api/health")
                .header("cookie", format!("rbc_token={wrong}"))
                .body(Body::empty())
                .unwrap(),
            get_req(&format!("/api/health?rbc_token={wrong}"))
                .body(Body::empty())
                .unwrap(),
            get_req("/api/health")
                .header(TOKEN_HEADER, "")
                .body(Body::empty())
                .unwrap(),
        ] {
            let (s, _) = status(app(&t), request).await;
            assert_eq!(s, StatusCode::UNAUTHORIZED);
        }
    }

    #[tokio::test]
    async fn accepts_header_and_cookie() {
        let t = "a".repeat(64);
        let (s, _) = status(
            app(&t),
            get_req("/api/health")
                .header(TOKEN_HEADER, &t)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        let (s, _) = status(
            app(&t),
            get_req("/api/health")
                .header("cookie", format!("theme=dark; rbc_token={t}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
    }

    #[tokio::test]
    async fn query_token_becomes_a_cookie_and_is_stripped() {
        let t = "a".repeat(64);
        let (s, h) = status(
            app(&t),
            get_req(&format!("/?x=1&rbc_token={t}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(s, StatusCode::SEE_OTHER);
        assert_eq!(h[header::LOCATION], "/?x=1");
        let cookie = h[header::SET_COOKIE].to_str().unwrap();
        assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"));
    }
}
