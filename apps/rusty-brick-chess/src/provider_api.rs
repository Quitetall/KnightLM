//! HTTP surface for the provider registry.
//!
//! Four things a person can do: see the providers, add or change one, test that
//! it answers, and ask this machine what is running locally. Nothing here returns
//! or accepts a secret — a request carries a `key_ref`, a name, and the answer
//! carries whether that name resolves, never what it resolves to.

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::{ApiError, AppState};
use crate::providers::{
    acceptable_base_url, capabilities_of, models_url, resolve_secret, ModelCard, LocalEndpoint,
    LOCAL_CANDIDATES,
};

#[derive(Deserialize)]
pub struct NewProvider {
    pub label: String,
    pub base_url: String,
    #[serde(default)]
    pub key_ref: Option<String>,
}

#[derive(Deserialize)]
pub struct EditProvider {
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    /// `Some(None)` clears the reference; absent leaves it alone. Serde can tell
    /// the two apart because the field is `Option<Option<_>>`.
    #[serde(default, deserialize_with = "double_option")]
    pub key_ref: Option<Option<String>>,
}

fn double_option<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<String>::deserialize(deserializer)?))
}

fn provider_json(row: &rbc_db::ProviderRow) -> Value {
    let models: Vec<ModelCard> = serde_json::from_str(&row.models_json).unwrap_or_default();
    // Whether the named secret resolves, and from where. Never the value.
    let (resolved, source) = match row.key_ref.as_deref() {
        Some(reference) => match resolve_secret(reference) {
            Ok(Some((_, source))) => (true, format!("{source:?}")),
            Ok(None) => (false, "unresolved".to_string()),
            Err(_) => (false, "unusable".to_string()),
        },
        None => (false, "no reference".to_string()),
    };
    json!({
        "id": row.id,
        "label": row.label,
        "kind": row.kind,
        "base_url": row.base_url,
        "key_ref": row.key_ref,
        "key_resolves": resolved,
        "key_source": source,
        "models": models,
        "created": row.created,
        "updated": row.updated,
    })
}

pub async fn list_providers(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let rows = state.db.list_providers().map_err(ApiError::from)?;
    Ok(Json(json!({
        "providers": rows.iter().map(provider_json).collect::<Vec<_>>()
    })))
}

pub async fn add_provider(
    State(state): State<AppState>,
    Json(body): Json<NewProvider>,
) -> Result<Json<Value>, ApiError> {
    let label = body.label.trim();
    if label.is_empty() {
        return Err(ApiError::BadRequest("a provider needs a label".into()));
    }
    if !acceptable_base_url(&body.base_url) {
        return Err(ApiError::BadRequest(
            "a base URL starts with http:// or https://".into(),
        ));
    }
    let row = state
        .db
        .add_provider(
            label,
            "openai_compatible",
            body.base_url.trim(),
            body.key_ref.as_deref().map(str::trim).filter(|r| !r.is_empty()),
        )
        .map_err(ApiError::from)?;
    // A key reference is configuration; the fact that one was set is worth
    // recording, the reference itself is not a secret but the habit is.
    let _ = state.db.append_audit(
        "person",
        "provider.add",
        "provider",
        &row.id,
        &json!({ "label": row.label, "base_url": row.base_url, "has_key_ref": row.key_ref.is_some() })
            .to_string(),
    );
    Ok(Json(provider_json(&row)))
}

pub async fn edit_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<EditProvider>,
) -> Result<Json<Value>, ApiError> {
    if let Some(base) = body.base_url.as_deref() {
        if !acceptable_base_url(base) {
            return Err(ApiError::BadRequest(
                "a base URL starts with http:// or https://".into(),
            ));
        }
    }
    let key_ref = body
        .key_ref
        .map(|inner| inner.map(|value| value.trim().to_string()))
        .map(|inner| inner.filter(|value| !value.is_empty()));
    let key_ref = key_ref.as_ref().map(|inner| inner.as_deref());
    let updated = state
        .db
        .update_provider(&id, body.label.as_deref(), body.base_url.as_deref(), key_ref)
        .map_err(ApiError::from)?;
    match updated {
        Some(row) => Ok(Json(provider_json(&row))),
        None => Err(ApiError::NotFound),
    }
}

pub async fn delete_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let removed = state.db.delete_provider(&id).map_err(ApiError::from)?;
    if !removed {
        return Err(ApiError::NotFound);
    }
    let _ = state
        .db
        .append_audit("person", "provider.delete", "provider", &id, "{}");
    Ok(Json(json!({ "deleted": id })))
}

/// Ask the endpoint what models it has, and keep the answer.
pub async fn refresh_models(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let Some(row) = state.db.provider(&id).map_err(ApiError::from)? else {
        return Err(ApiError::NotFound);
    };
    let started = std::time::Instant::now();
    let (models, detail) = fetch_models(&row.base_url, row.key_ref.as_deref()).await;
    let latency = started.elapsed().as_millis() as u64;
    let caps: Vec<ModelCard> = models.iter().map(|id| capabilities_of(id)).collect();
    state
        .db
        .set_provider_models(
            &id,
            &serde_json::to_string(&caps).unwrap_or_else(|_| "[]".into()),
            &serde_json::to_string(&json!({ "count": caps.len() })).unwrap_or_else(|_| "{}".into()),
        )
        .map_err(ApiError::from)?;
    Ok(Json(json!({
        "ok": detail.is_none(),
        "models": caps,
        "detail": detail.unwrap_or_default(),
        "latency_ms": latency,
    })))
}

/// Test without saving: reachable, and does the key work.
pub async fn test_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let Some(row) = state.db.provider(&id).map_err(ApiError::from)? else {
        return Err(ApiError::NotFound);
    };
    let started = std::time::Instant::now();
    let (models, detail) = fetch_models(&row.base_url, row.key_ref.as_deref()).await;
    Ok(Json(json!({
        "ok": detail.is_none(),
        "models": models.len(),
        "detail": detail.unwrap_or_default(),
        "latency_ms": started.elapsed().as_millis() as u64,
    })))
}

/// What is running on this machine right now.
pub async fn scan_local(_state: State<AppState>) -> Json<Value> {
    let mut found = Vec::new();
    for (label, base_url) in LOCAL_CANDIDATES {
        let (models, detail) = fetch_models(base_url, None).await;
        found.push(LocalEndpoint {
            label: (*label).to_string(),
            base_url: (*base_url).to_string(),
            responding: detail.is_none(),
            models,
        });
    }
    Json(json!({ "endpoints": found }))
}

/// Fetch the model list from an endpoint. Returns ids and, when it failed, why —
/// trimmed, because a provider's error body can be long.
async fn fetch_models(base_url: &str, key_ref: Option<&str>) -> (Vec<String>, Option<String>) {
    if !acceptable_base_url(base_url) {
        return (Vec::new(), Some("the base URL is not http(s)".into()));
    }
    let key = match key_ref {
        Some(reference) => match resolve_secret(reference) {
            Ok(Some((value, _))) => Some(value),
            Ok(None) => None,
            Err(error) => return (Vec::new(), Some(error)),
        },
        None => None,
    };
    let url = models_url(base_url);
    // ureq 3: statuses are not errors, the agent carries the timeout.
    let agent = ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .http_status_as_error(false)
            .timeout_global(Some(std::time::Duration::from_secs(4)))
            .build(),
    );
    let mut request = agent.get(&url).header("accept", "application/json");
    if let Some(key) = key.as_deref() {
        request = request.header("authorization", format!("Bearer {key}"));
    }
    match request.call() {
        Ok(mut response) => {
            let status = response.status().as_u16();
            let body = response
                .body_mut()
                .read_to_string()
                .unwrap_or_default();
            if status >= 400 {
                // A 401 is the interesting one: it means the endpoint is there
                // and the key is not. Say that rather than "failed".
                let hint = if status == 401 || status == 403 {
                    "the endpoint refused the credentials"
                } else {
                    "the endpoint refused the request"
                };
                let trimmed: String = body.chars().take(160).collect();
                return (Vec::new(), Some(format!("{hint} ({status}): {trimmed}")));
            }
            match serde_json::from_str::<Value>(&body) {
                Ok(json) => {
                    let ids = json
                        .get("data")
                        .and_then(|data| data.as_array())
                        .or_else(|| json.get("models").and_then(|models| models.as_array()))
                        .map(|entries| {
                            entries
                                .iter()
                                .filter_map(|entry| {
                                    entry
                                        .get("id")
                                        .and_then(|id| id.as_str())
                                        .or_else(|| entry.get("name").and_then(|n| n.as_str()))
                                        .map(str::to_string)
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    (ids, None)
                }
                Err(error) => (Vec::new(), Some(format!("unreadable answer: {error}"))),
            }
        }
        Err(error) => (Vec::new(), Some(format!("could not reach the endpoint: {error}"))),
    }
}
