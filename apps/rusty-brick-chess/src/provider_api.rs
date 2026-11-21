//! HTTP surface for the provider registry.
//!
//! Four things a person can do: see the providers, add or change one, test that
//! it answers, and ask this machine what is running locally. Nothing here returns
//! or accepts a secret — a request carries a `key_ref`, a name, and the answer
//! carries whether that name resolves, never what it resolves to.

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::api::{ApiError, AppState};
use crate::providers::{
    LOCAL_CANDIDATES, LocalEndpoint, ModelCard, ProbeError, acceptable_base_url, capabilities_of,
    probe_models, resolve_secret, validate_key_ref,
};

#[derive(Deserialize)]
pub struct NewProvider {
    /// Filled from `preset` when empty.
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub key_ref: Option<String>,
    /// A preset id from `GET /api/providers/presets`; its values fill any field left empty.
    #[serde(default)]
    pub preset: Option<String>,
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
    Json(mut body): Json<NewProvider>,
) -> Result<Json<Value>, ApiError> {
    if let Some(id) = body.preset.as_deref() {
        let preset = crate::providers::preset(id)
            .ok_or_else(|| ApiError::BadRequest(format!("unknown preset `{id}`")))?;
        if body.label.trim().is_empty() {
            body.label = preset.label;
        }
        if body.base_url.trim().is_empty() {
            body.base_url = preset.base_url;
        }
        if body.key_ref.is_none() {
            body.key_ref = preset.key_ref;
        }
    }
    let label = body.label.trim();
    if label.is_empty() {
        return Err(ApiError::BadRequest("a provider needs a label".into()));
    }
    if !acceptable_base_url(&body.base_url) {
        return Err(ApiError::BadRequest(
            "a base URL starts with http:// or https://".into(),
        ));
    }
    if let Some(reference) = body.key_ref.as_deref() {
        validate_key_ref(reference).map_err(ApiError::BadRequest)?;
    }
    let row = state
        .db
        .add_provider(
            label,
            "openai_compatible",
            body.base_url.trim(),
            body.key_ref
                .as_deref()
                .map(str::trim)
                .filter(|r| !r.is_empty()),
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
    if let Some(base) = body.base_url.as_deref()
        && !acceptable_base_url(base)
    {
        return Err(ApiError::BadRequest(
            "a base URL starts with http:// or https://".into(),
        ));
    }
    if let Some(Some(reference)) = body.key_ref.as_ref() {
        validate_key_ref(reference).map_err(ApiError::BadRequest)?;
    }
    let key_ref = body
        .key_ref
        .map(|inner| inner.map(|value| value.trim().to_string()))
        .map(|inner| inner.filter(|value| !value.is_empty()));
    let key_ref = key_ref.as_ref().map(|inner| inner.as_deref());
    let updated = state
        .db
        .update_provider(
            &id,
            body.label.as_deref(),
            body.base_url.as_deref(),
            key_ref,
        )
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

#[derive(Deserialize, Default)]
pub struct ProbeBody {
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub key_ref: Option<String>,
    /// When set, the test also checks that the endpoint lists this model.
    #[serde(default)]
    pub model: Option<String>,
}

/// The typed answer of a connection test: `ok`, a stable `error.code` when it
/// failed, the model count, and the round trip. `detail` repeats the message for
/// older clients.
async fn run_probe(base_url: String, key_ref: Option<String>, model: Option<String>) -> Value {
    let started = std::time::Instant::now();
    let outcome = tokio::task::spawn_blocking(move || {
        probe_models(&base_url, key_ref.as_deref(), model.as_deref())
    })
    .await
    .unwrap_or_else(|_| {
        Err(ProbeError {
            code: crate::providers::probe_code::UNREACHABLE,
            message: "the connection test was interrupted".into(),
        })
    });
    let latency = started.elapsed().as_millis() as u64;
    match outcome {
        Ok(models) => json!({
            "ok": true, "error": null, "detail": "",
            "models": models.len(), "model_ids": models, "latency_ms": latency,
        }),
        Err(error) => json!({
            "ok": false, "error": { "code": error.code, "message": error.message },
            "detail": error.message, "models": 0, "model_ids": [], "latency_ms": latency,
        }),
    }
}

/// Test a saved provider: reachable, does the key work, is the model listed.
pub async fn test_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<ProbeBody>>,
) -> Result<Json<Value>, ApiError> {
    let Some(row) = state.db.provider(&id).map_err(ApiError::from)? else {
        return Err(ApiError::NotFound);
    };
    let model = body.and_then(|Json(body)| body.model);
    Ok(Json(run_probe(row.base_url, row.key_ref, model).await))
}

/// Test an endpoint before saving it. Nothing is stored.
pub async fn probe_endpoint(Json(body): Json<ProbeBody>) -> Json<Value> {
    Json(run_probe(body.base_url, body.key_ref, body.model).await)
}

/// Presets a person can start a provider row from.
pub async fn list_presets() -> Json<Value> {
    Json(json!({ "presets": crate::providers::presets() }))
}

/// What is running on this machine right now.
pub async fn scan_local(_state: State<AppState>) -> Json<Value> {
    let mut found = Vec::new();
    for (label, base_url) in LOCAL_CANDIDATES {
        let override_url = (*label == "LAMU")
            .then(crate::lamu_runtime::base_url)
            .transpose();
        let (base_url, models, detail) = match override_url {
            Ok(url) => {
                let base_url = url.unwrap_or_else(|| (*base_url).to_owned());
                let (models, detail) = fetch_models(&base_url, None).await;
                (base_url, models, detail)
            }
            Err(error) => ((*base_url).to_owned(), Vec::new(), Some(error.to_string())),
        };
        found.push(LocalEndpoint {
            label: (*label).to_string(),
            base_url,
            responding: detail.is_none(),
            models,
        });
    }
    // The served checkpoint is not an OpenAI endpoint: it answers moves, and its
    // seat is `local:knightlm-serve` / `knightlm-serve`.
    let serve_url = crate::providers::knightlm_serve_url();
    let health = serve_url.replace("/v1/move", "/v1/health");
    let responding = tokio::task::spawn_blocking(move || {
        ureq::Agent::new_with_config(
            ureq::config::Config::builder()
                .timeout_global(Some(std::time::Duration::from_secs(2)))
                .build(),
        )
        .get(&health)
        .call()
        .is_ok()
    })
    .await
    .unwrap_or(false);
    found.push(LocalEndpoint {
        label: rbc_ai::provider::SERVE_PROVIDER
            .trim_start_matches("local:")
            .to_string(),
        base_url: serve_url,
        responding,
        models: if responding {
            vec![rbc_ai::provider::SERVE_MODEL.to_string()]
        } else {
            Vec::new()
        },
    });
    Json(json!({ "endpoints": found }))
}

/// Fetch the model list from an endpoint. Returns ids and, when it failed, why.
async fn fetch_models(base_url: &str, key_ref: Option<&str>) -> (Vec<String>, Option<String>) {
    let (base_url, key_ref) = (base_url.to_string(), key_ref.map(str::to_string));
    match tokio::task::spawn_blocking(move || probe_models(&base_url, key_ref.as_deref(), None))
        .await
    {
        Ok(Ok(ids)) => (ids, None),
        Ok(Err(error)) => (Vec::new(), Some(error.message)),
        Err(error) => (Vec::new(), Some(error.to_string())),
    }
}

// --- seats -----------------------------------------------------------------

/// The prefix of a seat's `provider` that names a registered provider row:
/// `provider:<id>`. Other spellings (`local:LAMU`, `local:Ollama`, the served
/// checkpoint) keep their existing meaning.
pub const REGISTERED_PREFIX: &str = "provider:";

/// One model seat: which provider, which model name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatRef {
    pub provider: String,
    pub model: String,
}

/// Which model holds each seat, and whether chat is on. Stored in the settings
/// table, so it survives a restart; a match carries its own seats and these are
/// what a new match and a coach turn start from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSeats {
    #[serde(default)]
    pub white: Option<SeatRef>,
    #[serde(default)]
    pub black: Option<SeatRef>,
    #[serde(default)]
    pub coach: Option<SeatRef>,
    /// Chat on: a coach turn. Off: the competition (play) harness. Unset follows
    /// the match (a casual game chats, a rated one never does).
    #[serde(default)]
    pub chat: Option<bool>,
}

const SEATS_KEY: &str = "model_seats";

pub fn load_seats(db: &rbc_db::Db) -> ModelSeats {
    db.settings()
        .ok()
        .and_then(|settings| settings.get(SEATS_KEY).cloned())
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

fn check_seat(db: &rbc_db::Db, name: &str, seat: &SeatRef) -> Result<(), ApiError> {
    if seat.model.trim().is_empty() {
        return Err(ApiError::BadRequest(format!(
            "the {name} seat needs a model"
        )));
    }
    if let Some(id) = seat.provider.strip_prefix(REGISTERED_PREFIX) {
        if db.provider(id).map_err(ApiError::from)?.is_none() {
            return Err(ApiError::BadRequest(format!(
                "the {name} seat names a provider that is not registered"
            )));
        }
    } else if !(seat.provider.starts_with("local:")
        || seat.provider == rbc_ai::provider::SERVE_PROVIDER)
    {
        return Err(ApiError::BadRequest(format!(
            "the {name} seat's provider is `provider:<id>` or `local:<name>`"
        )));
    }
    Ok(())
}

pub async fn get_seats(State(state): State<AppState>) -> Json<Value> {
    Json(json!(load_seats(&state.db)))
}

pub async fn put_seats(
    State(state): State<AppState>,
    Json(seats): Json<ModelSeats>,
) -> Result<Json<Value>, ApiError> {
    for (name, seat) in [
        ("white", &seats.white),
        ("black", &seats.black),
        ("coach", &seats.coach),
    ] {
        if let Some(seat) = seat {
            check_seat(&state.db, name, seat)?;
        }
    }
    let json = serde_json::to_string(&seats).map_err(|e| ApiError::Internal(e.into()))?;
    state
        .db
        .set_setting(SEATS_KEY, &json)
        .map_err(ApiError::from)?;
    Ok(Json(json!(seats)))
}
