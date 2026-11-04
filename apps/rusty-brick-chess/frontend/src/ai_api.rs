//! Assistant API client: sessions, turns, sends and context compilation.
//!
//! Shapes mirror `rbc_db::AiSession` / `rbc_db::AiTurn` and the `send` route in
//! `apps/rusty-brick-chess/src/domain_api.rs`.

use gloo_net::http::Request;
use serde::Deserialize;

/// A named assistant conversation (`GET /api/ai/sessions`).
#[derive(Debug, Clone, Deserialize)]
pub struct AiSessionInfo {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub home_board_id: Option<String>,
    #[serde(default)]
    pub target_board_id: Option<String>,
    #[serde(default)]
    pub model_json: String,
    #[serde(default)]
    pub context_spec_json: String,
    #[serde(default)]
    pub permissions_json: String,
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub updated: i64,
    #[serde(default)]
    pub archived: bool,
}

/// One append-only transcript entry (`GET /api/ai/sessions/{id}/turns`).
#[derive(Debug, Clone, Deserialize)]
pub struct AiTurnInfo {
    pub id: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub seq: i64,
    pub role: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub reasoning: Option<String>,
    #[serde(default)]
    pub tool_calls_json: String,
    #[serde(default)]
    pub tool_results_json: String,
    #[serde(default)]
    pub context_json: String,
    #[serde(default)]
    pub context_sha256: String,
    #[serde(default)]
    pub spec_version: String,
    #[serde(default)]
    pub template_version: String,
    #[serde(default)]
    pub tool_schema_version: String,
    #[serde(default)]
    pub engine_fingerprint: Option<String>,
    #[serde(default)]
    pub model_json: String,
    #[serde(default)]
    pub tokens_in: i64,
    #[serde(default)]
    pub tokens_out: i64,
    #[serde(default)]
    pub latency_ms: i64,
    #[serde(default)]
    pub error_json: Option<String>,
    #[serde(default)]
    pub created: i64,
}

/// A section of the compiled context (`rbc_context::Section`).
#[derive(Debug, Clone, Deserialize)]
pub struct AiContextSection {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub priority: u8,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
    #[serde(default)]
    pub tokens: u32,
    #[serde(default)]
    pub included: bool,
    #[serde(default)]
    pub truncated: bool,
}

/// The context summary returned by `send`.
#[derive(Debug, Clone, Deserialize)]
pub struct AiSendContext {
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub spec_id: String,
    #[serde(default)]
    pub spec_version: u32,
    #[serde(default)]
    pub tokens: u32,
    #[serde(default)]
    pub sections: Vec<AiContextSection>,
}

/// `POST /api/ai/sessions/{id}/send`.
///
/// The route answers with `{reply, actions, tools, user_turn,
/// assistant_turn, context}` on success and `{user_turn, assistant_turn,
/// error}` when the provider fails, so both halves are optional here.
#[derive(Debug, Clone, Deserialize)]
pub struct AiSendResponse {
    #[serde(default)]
    pub reply: Option<String>,
    #[serde(default)]
    pub actions: Vec<serde_json::Value>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub user_turn: Option<AiTurnInfo>,
    #[serde(default)]
    pub assistant_turn: Option<AiTurnInfo>,
    #[serde(default)]
    pub context: Option<AiSendContext>,
    #[serde(default)]
    pub error: Option<String>,
}

/// `POST /api/ai/context/compile`.
#[derive(Debug, Clone, Deserialize)]
pub struct AiCompiledContext {
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub spec_id: String,
    #[serde(default)]
    pub spec_version: u32,
    #[serde(default)]
    pub tokenizer: String,
    #[serde(default)]
    pub sections: Vec<AiContextSection>,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub data: serde_json::Value,
    #[serde(default)]
    pub tokens: u32,
    #[serde(default)]
    pub sha256: String,
}

async fn decode<T: for<'de> Deserialize<'de>>(
    resp: gloo_net::http::Response,
) -> Result<T, String> {
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn list_ai_sessions() -> Result<Vec<AiSessionInfo>, String> {
    let resp = Request::get("/api/ai/sessions")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    decode(resp).await
}

pub async fn create_ai_session(
    title: &str,
    mode: &str,
    home_board_id: Option<&str>,
    target_board_id: Option<&str>,
) -> Result<AiSessionInfo, String> {
    let resp = Request::post("/api/ai/sessions")
        .json(&serde_json::json!({
            "title": title,
            "mode": mode,
            "home_board_id": home_board_id,
            "target_board_id": target_board_id,
        }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    decode(resp).await
}

pub async fn get_ai_session(id: &str) -> Result<AiSessionInfo, String> {
    let resp = Request::get(&format!("/api/ai/sessions/{id}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    decode(resp).await
}

pub async fn rename_ai_session(id: &str, title: &str) -> Result<AiSessionInfo, String> {
    let resp = Request::post(&format!("/api/ai/sessions/{id}/rename"))
        .json(&serde_json::json!({ "title": title }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    decode(resp).await
}

pub async fn archive_ai_session(id: &str) -> Result<(), String> {
    let resp = Request::delete(&format!("/api/ai/sessions/{id}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.ok() {
        Ok(())
    } else {
        Err(resp.text().await.unwrap_or_default())
    }
}

pub async fn set_ai_target(id: &str, board_id: Option<&str>) -> Result<AiSessionInfo, String> {
    let resp = Request::post(&format!("/api/ai/sessions/{id}/target"))
        .json(&serde_json::json!({ "board_id": board_id }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    decode(resp).await
}

pub async fn list_ai_turns(id: &str) -> Result<Vec<AiTurnInfo>, String> {
    let resp = Request::get(&format!("/api/ai/sessions/{id}/turns"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    decode(resp).await
}

pub async fn send_ai_message(
    id: &str,
    message: &str,
    fen: Option<&str>,
) -> Result<AiSendResponse, String> {
    let resp = Request::post(&format!("/api/ai/sessions/{id}/send"))
        .json(&serde_json::json!({ "message": message, "fen": fen }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    decode(resp).await
}

pub async fn compile_context(
    fen: Option<&str>,
    mode: &str,
    side: Option<&str>,
    preset: &str,
) -> Result<AiCompiledContext, String> {
    let resp = Request::post("/api/ai/context/compile")
        .json(&serde_json::json!({
            "fen": fen,
            "mode": mode,
            "side": side,
            "preset": preset,
        }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    decode(resp).await
}

/// Stop the turn running in a session.
///
/// The server answers `{cancelled: bool}`: cancelling a session with nothing
/// running is a no-op, not an error.
pub async fn cancel_ai_turn(id: &str) -> Result<bool, String> {
    let resp = Request::post(&format!("/api/ai/sessions/{id}/cancel"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let body = resp.text().await.map_err(|e| e.to_string())?;
    let value: serde_json::Value =
        serde_json::from_str(&body).map_err(|_| if body.is_empty() { "cancel failed".to_string() } else { body.clone() })?;
    Ok(value
        .get("cancelled")
        .and_then(|cancelled| cancelled.as_bool())
        .unwrap_or(false))
}

/// The event stream of a session, from the beginning.
///
/// Replayed events for turns that are already stored are ignored by the view,
/// so it can open this at 0 without rendering history twice.
pub fn stream_url(id: &str) -> String {
    format!("/api/ai/sessions/{id}/stream")
}

/// Rebuild a stored turn's context and compare it with what was sent.
pub async fn replay_ai_turn(id: &str, seq: i64) -> Result<serde_json::Value, String> {
    let resp = Request::get(&format!("/api/ai/sessions/{id}/replay/{seq}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let body = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&body).map_err(|_| body)
}

/// One queued approval, as the queue API returns it.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ApprovalInfo {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub tool: String,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub changeset_summary: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub target_id: Option<String>,
    #[serde(default)]
    pub changeset: Option<serde_json::Value>,
    #[serde(default)]
    pub created_at: i64,
}

/// Everything waiting for a person, oldest first.
pub async fn list_approvals() -> Result<Vec<ApprovalInfo>, String> {
    let resp = Request::get("/api/ai/approvals")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let body = resp.text().await.map_err(|e| e.to_string())?;
    let value: serde_json::Value = serde_json::from_str(&body).map_err(|_| body.clone())?;
    serde_json::from_value(value.get("pending").cloned().unwrap_or(serde_json::json!([])))
        .map_err(|e| e.to_string())
}

/// Answer a pending approval.
pub async fn resolve_approval(id: &str, approve: bool) -> Result<(), String> {
    let path = if approve { "approve" } else { "deny" };
    let resp = Request::post(&format!("/api/ai/approvals/{id}/{path}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let body = resp.text().await.map_err(|e| e.to_string())?;
    if body.contains("\"error\"") {
        return Err(body);
    }
    Ok(())
}
