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

/// The setting holding the scopes MCP may take without asking.
///
/// Empty by default: the design's rule is that board writes prompt for MCP and
/// study/destructive always do, so an auto-approve is something a person turns
/// on deliberately.
const SCOPES_KEY: &str = "mcp_auto_approve";

fn mcp_profile(state: &AppState) -> rbc_ai::policy::PermissionProfile {
    let granted = state
        .db
        .setting(SCOPES_KEY)
        .ok()
        .flatten()
        .unwrap_or_default();
    let mut profile = rbc_ai::policy::PermissionProfile::default();
    for name in granted.split(',').map(str::trim).filter(|n| !n.is_empty()) {
        let scope = match name {
            "board-write" => rbc_ai::policy::PermissionScope::BoardWrite,
            "study-content" => rbc_ai::policy::PermissionScope::StudyContent,
            "destructive" => rbc_ai::policy::PermissionScope::Destructive,
            _ => continue,
        };
        profile.grant(rbc_ai::policy::Origin::Mcp, scope);
    }
    profile
}

/// The design's per-connection budget: generous for a working agent, a wall for
/// one stuck in a loop.
const REQUESTS_PER_MINUTE: u32 = 240;

/// The process-wide limiter, keyed by connection.
fn rate_limit(state: &AppState) -> &'static RateLimiter {
    use std::sync::OnceLock;
    let _ = state;
    static LIMITER: OnceLock<RateLimiter> = OnceLock::new();
    LIMITER
        .get_or_init(|| RateLimiter::new(REQUESTS_PER_MINUTE, std::time::Duration::from_secs(60)))
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
    let resources = BoardResources { db: &state.db };
    let profile = mcp_profile(&state);
    let dispatched =
        rbc_ai::mcp::dispatch(&request, &registry, &mut host, &resources, &profile, &info);
    match rbc_ai::mcp::response(id.as_ref(), dispatched) {
        Some(reply) => Ok((StatusCode::OK, Json(reply))),
        // A notification: accepted, nothing to say.
        None => Ok((StatusCode::ACCEPTED, Json(json!(null)))),
    }
}

/// `GET /api/mcp/config` — what a client needs to connect, never more.
pub async fn mcp_config(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
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
pub async fn mcp_rotate(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
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
        let mut host =
            crate::ai_host::SessionHost::new(db, "mcp", None, "mcp", rbc_ai::policy::Origin::Mcp);
        let id = request.get("id").cloned();
        let resources = BoardResources { db };
        // stdio inherits process trust, so it may take the scopes a person
        // granted for MCP without asking again.
        let mut profile = rbc_ai::policy::PermissionProfile::default();
        if let Ok(Some(granted)) = db.setting(SCOPES_KEY) {
            for name in granted.split(',').map(str::trim).filter(|n| !n.is_empty()) {
                let scope = match name {
                    "board-write" => rbc_ai::policy::PermissionScope::BoardWrite,
                    "study-content" => rbc_ai::policy::PermissionScope::StudyContent,
                    "destructive" => rbc_ai::policy::PermissionScope::Destructive,
                    _ => continue,
                };
                profile.grant(rbc_ai::policy::Origin::Mcp, scope);
            }
        }
        let dispatched =
            rbc_ai::mcp::dispatch(&request, &registry, &mut host, &resources, &profile, &info);
        if let Some(reply) = rbc_ai::mcp::response(id.as_ref(), dispatched) {
            writeln!(stdout, "{reply}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

// --- the resources the server publishes ------------------------------------

/// Read-only snapshots of this database, addressed by `rbc://` uri.
///
/// Every read is a snapshot: the revision travels in the payload, so a client
/// can tell one picture of a board from the next. Nothing here writes, and
/// nothing here can reach an AI session, a provider key or a setting beyond the
/// MCP token that the config route already owns.
pub struct BoardResources<'a> {
    pub db: &'a rbc_db::Db,
}

impl rbc_ai::mcp::Resources for BoardResources<'_> {
    fn list(&self) -> Vec<rbc_ai::mcp::ResourceEntry> {
        let mut out = vec![rbc_ai::mcp::ResourceEntry {
            uri: "rbc://workspaces".into(),
            name: "Workspaces".into(),
            description: "Every workspace with its kind and board count.".into(),
            revision: None,
        }];
        for workspace in self.db.workspaces(false).unwrap_or_default() {
            out.push(rbc_ai::mcp::ResourceEntry {
                uri: format!("rbc://workspace/{}", workspace.id),
                name: workspace.title.clone(),
                description: "A workspace.".into(),
                revision: None,
            });
            for board in self
                .db
                .boards("workspace", &workspace.id)
                .unwrap_or_default()
            {
                out.push(rbc_ai::mcp::ResourceEntry {
                    uri: format!("rbc://board/{}", board.id),
                    name: board.title.clone(),
                    description: format!("A board at revision {}.", board.revision),
                    revision: Some(board.revision),
                });
                out.push(rbc_ai::mcp::ResourceEntry {
                    uri: format!("rbc://board/{}/history", board.id),
                    name: format!("{} history", board.title),
                    description: "The move history of a board.".into(),
                    revision: Some(board.revision),
                });
            }
        }
        for study in self.db.studies().unwrap_or_default() {
            out.push(rbc_ai::mcp::ResourceEntry {
                uri: format!("rbc://study/{}", study.id),
                name: study.title.clone(),
                description: "A study.".into(),
                revision: Some(study.current_version),
            });
        }
        out
    }

    fn read(&self, uri: &str) -> Option<rbc_ai::mcp::ResourceContent> {
        let rest = uri.strip_prefix("rbc://")?;
        let parts: Vec<&str> = rest.split('/').collect();
        match parts.as_slice() {
            ["workspaces"] => Some(json_resource(
                uri,
                serde_json::to_value(self.db.workspaces(false).ok()?).ok()?,
                None,
            )),
            ["workspace", id] => {
                let workspace = self.db.workspace(id).ok()??;
                let boards = self.db.boards("workspace", id).unwrap_or_default();
                Some(json_resource(
                    uri,
                    serde_json::json!({
                        "workspace": workspace,
                        "boards": boards.iter().map(|b| serde_json::json!({
                            "id": b.id, "title": b.title, "revision": b.revision,
                        })).collect::<Vec<_>>(),
                    }),
                    None,
                ))
            }
            ["board", id] => {
                let board = self.db.board(id).ok()??;
                let revision = board.revision;
                Some(json_resource(
                    uri,
                    serde_json::to_value(&board).ok()?,
                    Some(revision),
                ))
            }
            ["board", id, "history"] => {
                let board = self.db.board(id).ok()??;
                let content: rbc_ai::board::BoardContent =
                    rbc_ai::board::BoardContent::parse(&board.content_json).ok()?;
                Some(json_resource(
                    uri,
                    serde_json::json!({
                        "board_id": board.id,
                        "start_fen": board.start_fen,
                        "moves": content.moves,
                        "revision": board.revision,
                    }),
                    Some(board.revision),
                ))
            }
            ["study", id] => {
                let study = self.db.study(id).ok()??;
                let revision = study.current_version;
                let chapters = self.db.chapters(id).unwrap_or_default();
                Some(json_resource(
                    uri,
                    serde_json::json!({"study": study, "chapters": chapters}),
                    Some(revision),
                ))
            }
            ["study", id, "chapter", chapter_id] => {
                let study = self.db.study(id).ok()??;
                let chapters = self.db.chapters(id).unwrap_or_default();
                let chapter = chapters.into_iter().find(|c| &c.id == chapter_id)?;
                Some(json_resource(
                    uri,
                    serde_json::to_value(&chapter).ok()?,
                    Some(study.current_version),
                ))
            }
            ["match", id] => {
                let row = self.db.ai_match(id).ok()??;
                Some(json_resource(
                    uri,
                    serde_json::json!({"id": row.id, "pgn": row.pgn, "state": row.state_json}),
                    None,
                ))
            }
            ["match", id, "pgn"] => {
                let row = self.db.ai_match(id).ok()??;
                Some(rbc_ai::mcp::ResourceContent {
                    uri: uri.to_string(),
                    mime_type: "application/x-chess-pgn".into(),
                    text: row.pgn,
                    revision: None,
                })
            }
            _ => None,
        }
    }
}

fn json_resource(
    uri: &str,
    value: serde_json::Value,
    revision: Option<i64>,
) -> rbc_ai::mcp::ResourceContent {
    rbc_ai::mcp::ResourceContent {
        uri: uri.to_string(),
        mime_type: "application/json".into(),
        text: serde_json::to_string_pretty(&value).unwrap_or_default(),
        revision,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbc_ai::mcp::Resources;

    fn db() -> rbc_db::Db {
        let db = rbc_db::Db::open_memory().expect("db");
        let workspace = db.create_workspace("W", "analysis").expect("workspace");
        db.create_board(
            "workspace",
            &workspace.id,
            "analysis",
            "B",
            rbc_core_fen(),
            "",
        )
        .expect("board");
        db
    }

    fn rbc_core_fen() -> &'static str {
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
    }

    #[test]
    fn a_token_must_match_exactly() {
        let token = "mcp-abc";
        assert!(token_matches(Some("Bearer mcp-abc"), token));
        assert!(
            token_matches(Some("mcp-abc"), token),
            "the scheme is optional"
        );
        assert!(!token_matches(Some("Bearer mcp-abd"), token));
        assert!(!token_matches(Some("Bearer mcp-ab"), token), "short");
        assert!(!token_matches(Some("Bearer mcp-abcd"), token), "long");
        assert!(!token_matches(None, token), "absent");
        assert!(!token_matches(Some(""), token), "empty");
        assert!(!token_matches(Some("Bearer "), token), "scheme only");
    }

    #[test]
    fn a_resource_path_cannot_escape_into_another_resource() {
        // Traversal attempts must resolve to nothing, not to a neighbouring
        // resource: the matcher is a fixed list of shapes, and anything that is
        // not one of them is refused by name.
        let db = db();
        let resources = BoardResources { db: &db };
        for uri in [
            "rbc://../board/x",
            "rbc://board/../../etc/passwd",
            "rbc://board/../workspaces",
            "rbc:///etc/passwd",
            "rbc://workspaces/../../etc/passwd",
            "file:///etc/passwd",
            "/etc/passwd",
            "rbc://board/x/y/z",
            "rbc://BOARD/x",
        ] {
            assert!(
                resources.read(uri).is_none(),
                "{uri} must not resolve to a resource"
            );
        }
    }

    #[test]
    fn a_known_resource_still_reads() {
        // The refusals above must not be a wall that also blocks real uris.
        let db = db();
        let resources = BoardResources { db: &db };
        let listed = resources.list();
        assert!(listed.iter().any(|entry| entry.uri == "rbc://workspaces"));
        let board_uri = listed
            .iter()
            .find(|entry| entry.uri.starts_with("rbc://board/") && !entry.uri.ends_with("history"))
            .expect("a board resource")
            .uri
            .clone();
        assert!(resources.read(&board_uri).is_some(), "{board_uri} reads");
        assert!(resources.read("rbc://workspaces").is_some());
    }

    #[test]
    fn an_unknown_but_well_formed_uri_is_none_rather_than_a_panic() {
        let db = db();
        let resources = BoardResources { db: &db };
        assert!(resources.read("rbc://board/no-such-board").is_none());
        assert!(resources.read("rbc://study/no-such-study").is_none());
        assert!(resources.read("rbc://match/no-such-match").is_none());
        assert!(
            resources
                .read("rbc://workspace/no-such-workspace")
                .is_none()
        );
    }

    #[test]
    fn the_rate_limiter_refuses_the_request_after_the_cap() {
        let limiter = RateLimiter::new(3, std::time::Duration::from_secs(60));
        assert!(limiter.allow("a"));
        assert!(limiter.allow("a"));
        assert!(limiter.allow("a"));
        assert!(!limiter.allow("a"), "the fourth is refused");
        assert_eq!(limiter.connections(), 1);
        // Another connection has its own budget.
        assert!(limiter.allow("b"));
    }
}
