//! The MCP transports: a bearer-token HTTP endpoint and a stdio server.
//!
//! The protocol lives in `rbc_ai::mcp`; this module owns the two ways a request
//! can arrive, the token that HTTP requires, and the limits a transport has to
//! impose. Both transports use the same tool registry, host and approval queue,
//! which is the design's "one registry, one queue, one audit log".

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde_json::{Value, json};

use crate::api::{ApiError, AppState};

/// The setting the token lives under. Generated on first use.
const TOKEN_KEY: &str = "mcp_token";

/// The design's per-connection budget: generous for a working agent, a wall for
/// one stuck in a loop.
const REQUESTS_PER_MINUTE: u32 = 240;

/// The process-wide limiter, keyed by connection.
fn rate_limit(state: &AppState) -> &'static RateLimiter {
    use std::sync::OnceLock;
    let _ = state;
    static LIMITER: OnceLock<RateLimiter> = OnceLock::new();
    LIMITER.get_or_init(|| RateLimiter::new(REQUESTS_PER_MINUTE, std::time::Duration::from_secs(60)))
}

fn token_of(state: &AppState) -> Result<String, ApiError> {
    Ok(state.db.setting(TOKEN_KEY)?.unwrap_or_default())
}

/// The token, generating one the first time it is asked for.
fn ensure_token(state: &AppState) -> Result<String, ApiError> {
    let existing = token_of(state)?;
    if !existing.trim().is_empty() {
        return Ok(existing);
    }
    // The token only has to be unguessable on loopback; a monotonic millisecond
    // stamp is not enough on its own, so it is salted with the database path.
    let token = new_token();
    state.db.set_setting(TOKEN_KEY, &token)?;
    Ok(token)
}

/// A fresh token: time plus a per-process counter, so two rotations in the same
/// millisecond still differ.
fn new_token() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "mcp-{}-{}-{}",
        crate::ai_host::now_ms(),
        counter,
        std::process::id()
    )
}

/// Compare the presented bearer token with the stored one.
///
/// Length-independent: every byte is compared and the differences accumulated,
/// so a near miss is not a timing signal about how much matched.
fn token_matches(presented: Option<&str>, expected: &str) -> bool {
    let Some(value) = presented else {
        return false;
    };
    let value = value.strip_prefix("Bearer ").unwrap_or(value);
    if value.len() != expected.len() {
        return false;
    }
    value
        .bytes()
        .zip(expected.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

/// A per-connection rate limit: a fixed window, because a local agent either
/// bursts briefly or is stuck in a loop, and a window is enough to tell them
/// apart.
pub struct RateLimiter {
    window: std::time::Duration,
    limit: u32,
    seen: std::sync::Mutex<std::collections::HashMap<String, (std::time::Instant, u32)>>,
}

impl RateLimiter {
    /// `limit` requests per `window`, per connection.
    pub fn new(limit: u32, window: std::time::Duration) -> Self {
        Self {
            window,
            limit,
            seen: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Whether this connection may make another call now.
    pub fn allow(&self, connection: &str) -> bool {
        let now = std::time::Instant::now();
        let mut seen = self.seen.lock().expect("rate limiter");
        // Forget connections that have been quiet for a window, so the map does
        // not grow with every client that ever connected.
        seen.retain(|_, (started, _)| now.duration_since(*started) < self.window);
        let entry = seen.entry(connection.to_string()).or_insert((now, 0));
        if now.duration_since(entry.0) >= self.window {
            *entry = (now, 0);
        }
        if entry.1 >= self.limit {
            return false;
        }
        entry.1 += 1;
        true
    }

    /// How many connections are inside the current window.
    pub fn connections(&self) -> usize {
        self.seen.lock().expect("rate limiter").len()
    }
}

/// `POST /mcp` — one JSON-RPC message in, one response out.
///
/// Streamable HTTP without the optional SSE stream: every call answers on the
/// same response, which is what an agent needs and what a `tools/call` produces.
pub async fn mcp_http(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<impl IntoResponse, ApiError> {
    let expected = ensure_token(&state)?;
    let presented = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok());
    if !token_matches(presented, &expected) {
        // Not 401 with a body: a caller without the token learns nothing else.
        return Err(ApiError::Unauthorized);
    }
    if body.len() > rbc_ai::mcp::MAX_MESSAGE_BYTES {
        return Err(ApiError::BadRequest("message too large".into()));
    }

    // The connection is the peer address: enough to separate two local agents,
    // and the only identity a tokenless stdio client would not have anyway.
    let connection = headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("loopback")
        .to_string();
    if !rate_limit(&state).allow(&connection) {
        return Err(ApiError::TooManyRequests);
    }

    let request: Value = serde_json::from_slice(&body)
        .map_err(|error| ApiError::BadRequest(format!("invalid JSON: {error}")))?;
    let registry = rbc_ai::tools::ToolRegistry::builtin();
    let info = rbc_ai::mcp::ServerInfo::default();
    let mut host = crate::ai_host::SessionHost::new(
        &state.db,
        "mcp",
        None,
        "mcp",
        rbc_ai::policy::Origin::Mcp,
    );
    let id = request.get("id").cloned();
    let dispatched = rbc_ai::mcp::dispatch(&request, &registry, &mut host, &info);
    match rbc_ai::mcp::response(id.as_ref(), dispatched) {
        Some(reply) => Ok((StatusCode::OK, Json(reply))),
        // A notification: accepted, nothing to say.
        None => Ok((StatusCode::ACCEPTED, Json(json!(null)))),
    }
}

/// `GET /api/mcp/config` — what a client needs to connect, never more.
pub async fn mcp_config(
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let token = ensure_token(&state)?;
    let addr = std::env::var("RBC_ADDR").unwrap_or_else(|_| "127.0.0.1:8090".into());
    Ok(Json(json!({
        "http": format!("http://{addr}/mcp"),
        "stdio": "rusty-brick-chess mcp",
        "token": token,
        "protocol": rbc_ai::mcp::PROTOCOL_VERSION,
        "max_message_bytes": rbc_ai::mcp::MAX_MESSAGE_BYTES,
        "requests_per_minute": REQUESTS_PER_MINUTE,
        "connections": rate_limit(&state).connections(),
    })))
}

/// `POST /api/mcp/rotate` — a new token, invalidating the old one.
pub async fn mcp_rotate(
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let token = new_token();
    state.db.set_setting(TOKEN_KEY, &token)?;
    Ok(Json(json!({ "token": token })))
}

/// The stdio server: one JSON-RPC message per line, answered on stdout.
///
/// stdio inherits process trust, so there is no token here; the parent process
/// already runs as this user with this database.
pub fn serve_stdio(db: &rbc_db::Db) -> anyhow::Result<()> {
    use std::io::{BufRead, Write};
    let registry = rbc_ai::tools::ToolRegistry::builtin();
    let info = rbc_ai::mcp::ServerInfo::default();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if line.len() > rbc_ai::mcp::MAX_MESSAGE_BYTES {
            let error = json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": {"code": -32600, "message": "message too large"},
            });
            writeln!(stdout, "{error}")?;
            stdout.flush()?;
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(error) => {
                let reply = json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {"code": -32700, "message": format!("invalid JSON: {error}")},
                });
                writeln!(stdout, "{reply}")?;
                stdout.flush()?;
                continue;
            }
        };
        let mut host = crate::ai_host::SessionHost::new(
            db,
            "mcp",
            None,
            "mcp",
            rbc_ai::policy::Origin::Mcp,
        );
        let id = request.get("id").cloned();
        let dispatched = rbc_ai::mcp::dispatch(&request, &registry, &mut host, &info);
        if let Some(reply) = rbc_ai::mcp::response(id.as_ref(), dispatched) {
            writeln!(stdout, "{reply}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}
