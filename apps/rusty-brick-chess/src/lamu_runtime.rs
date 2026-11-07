//! On-demand loopback LAMU and explicit model residency controls.

use std::time::Duration;

use anyhow::{Context, Result, anyhow, ensure};
use axum::Json;
use axum::extract::{ConnectInfo, Path, State};
use axum::http::{HeaderMap, header::HOST, uri::Authority};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{ApiError, AppState};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

pub(crate) const KNIGHTLM_MODEL: &str = "knightlm-smolvlm-500m";

async fn healthy(base: &str) -> bool {
    let url = format!("{base}/health");
    tokio::task::spawn_blocking(move || {
        let agent = ureq::Agent::new_with_config(
            ureq::config::Config::builder()
                .timeout_global(Some(Duration::from_millis(600)))
                .build(),
        );
        agent
            .get(&url)
            .call()
            .ok()
            .and_then(|mut response| response.body_mut().read_json::<serde_json::Value>().ok())
            .is_some_and(|body| body["service"] == "lamu" && body["status"] == "ok")
    })
    .await
    .unwrap_or(false)
}

fn lamu_port() -> Result<u16> {
    let port = std::env::var("RBC_LAMU_PORT")
        .unwrap_or_else(|_| "8020".into())
        .parse::<u16>()
        .context("RBC_LAMU_PORT must be a port number")?;
    ensure!(port != 0, "RBC_LAMU_PORT must be nonzero");
    Ok(port)
}

pub(crate) fn base_url() -> Result<String> {
    Ok(format!("http://127.0.0.1:{}/v1", lamu_port()?))
}

pub(crate) async fn ensure_runtime(child: &Mutex<Option<Child>>) -> Result<String> {
    let port = lamu_port()?;
    let origin = format!("http://127.0.0.1:{port}");
    let mut child = child.lock().await;
    if !healthy(&origin).await {
        if let Some(process) = child.as_mut()
            && process.try_wait()?.is_some()
        {
            *child = None;
        }
        if child.is_none() {
            // Do not try to launch over a live listener that failed the LAMU probe.
            if tokio::time::timeout(
                Duration::from_millis(300),
                tokio::net::TcpStream::connect(format!("127.0.0.1:{port}")),
            )
            .await
            .is_ok_and(|result| result.is_ok())
            {
                return Err(anyhow!(
                    "port {port} is occupied by a service that did not report healthy LAMU"
                ));
            }
            let program = std::env::var_os("RBC_LAMU_BIN").unwrap_or_else(|| "lamu".into());
            *child = Some(
                Command::new(program)
                    .args(["serve", "--port", &port.to_string()])
                    .env("LAMU_BIND_HOST", "127.0.0.1")
                    .env("LAMU_NO_PRELOAD", "1")
                    .kill_on_drop(true)
                    .spawn()
                    .context("could not start LAMU; install lamu or set RBC_LAMU_BIN")?,
            );
        }
        let started = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if healthy(&origin).await {
                    return Ok(());
                }
                if let Some(status) = child.as_mut().expect("LAMU child started").try_wait()? {
                    return Err(anyhow!("LAMU exited before becoming ready: {status}"));
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        })
        .await
        .context("LAMU did not become ready within 20 seconds");
        if let Err(failure) = started.and_then(|result| result) {
            *child = None;
            return Err(failure);
        }
    }
    Ok(format!("{origin}/v1"))
}

#[derive(Deserialize)]
pub(crate) struct ModelSelection {
    model: String,
}

pub(crate) async fn manage_model(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<std::net::SocketAddr>,
    Path(action): Path<String>,
    headers: HeaderMap,
    Json(selection): Json<ModelSelection>,
) -> Result<Json<Value>, ApiError> {
    // This local app has no remote administrator. Check both the socket and
    // Host so a browser page reached through DNS rebinding cannot manage LAMU.
    let local_host = headers
        .get(HOST)
        .and_then(|host| host.to_str().ok())
        .and_then(|host| host.parse::<Authority>().ok())
        .is_some_and(|host| {
            host.host().eq_ignore_ascii_case("localhost")
                || host
                    .host()
                    .trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
    if !peer.ip().is_loopback() || !local_host {
        return Err(ApiError::Unauthorized);
    }
    if !matches!(action.as_str(), "load" | "unload") || selection.model.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "choose one LAMU model to load or unload".into(),
        ));
    }
    let base = if action == "load" {
        ensure_runtime(&state.lamu_child).await?
    } else {
        base_url()?
    };
    tokio::task::spawn_blocking(move || {
        model_action(&base, &action, &selection.model)
            .map(Json)
            .map_err(|error| ApiError::BadRequest(error.to_string()))
    })
    .await
    .context("LAMU model management task failed")?
}

fn model_action(base: &str, action: &str, model: &str) -> Result<Value> {
    let agent = ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(10)))
            .build(),
    );
    // An untimed chess game needs a short context. Keep the KV cache small
    // enough for larger opponents to fit alongside the desktop.
    let request = if action == "load" {
        json!({ "model": model, "context": 8192 })
    } else {
        json!({ "model": model })
    };
    let mut response = agent
        .post(format!("{base}/admin/models/{action}"))
        .send_json(request)
        .with_context(|| format!("could not reach LAMU to {action} {model}"))?;
    let status = response.status();
    let result: Value = response
        .body_mut()
        .read_json()
        .with_context(|| format!("LAMU returned HTTP {status} while trying to {action} {model}"))?;
    let already_unloaded = action == "unload" && result["error"]["code"] == "not_loaded";
    ensure!(
        status.is_success() || already_unloaded,
        "{}",
        result["error"]["message"]
            .as_str()
            .unwrap_or("LAMU refused the model operation")
    );
    if action == "load" && status.as_u16() == 202 {
        let deadline = std::time::Instant::now() + Duration::from_secs(120);
        loop {
            let mut response = agent.get(format!("{base}/models")).call()?;
            ensure!(
                response.status().is_success(),
                "could not read LAMU model status"
            );
            let catalog: Value = response.body_mut().read_json()?;
            let entry = catalog["data"]
                .as_array()
                .and_then(|models| models.iter().find(|entry| entry["id"] == model))
                .with_context(|| format!("{model} is missing from LAMU's catalog"))?;
            if entry["loaded"] == true {
                break;
            }
            if let Some(error) = entry["last_error"].as_str() {
                return Err(anyhow!("LAMU could not load {model}: {error}"));
            }
            ensure!(
                std::time::Instant::now() < deadline,
                "LAMU is still loading {model}. Try again when it is ready."
            );
            std::thread::sleep(Duration::from_millis(500));
        }
    }
    Ok(json!({ "model": model, "loaded": action == "load" }))
}
