//! Start a loopback LAMU runtime only when the KnightLM Assistant needs it.

use std::time::Duration;

use anyhow::{Context, Result, anyhow, ensure};
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

pub(crate) async fn knightlm_endpoint(child: &Mutex<Option<Child>>) -> Result<String> {
    let port = std::env::var("RBC_LAMU_PORT")
        .unwrap_or_else(|_| "8020".into())
        .parse::<u16>()
        .context("RBC_LAMU_PORT must be a port number")?;
    ensure!(port != 0, "RBC_LAMU_PORT must be nonzero");
    let base = format!("http://127.0.0.1:{port}");
    let mut child = child.lock().await;
    if !healthy(&base).await {
        if let Some(process) = child.as_mut()
            && process.try_wait()?.is_some()
        {
            *child = None;
        }
        if child.is_none() {
            // Do not try to launch over a live listener that failed the LAMU probe.
            if tokio::time::timeout(
                Duration::from_millis(300),
                tokio::net::TcpStream::connect(("127.0.0.1", port)),
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
                if healthy(&base).await {
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
    Ok(format!("{base}/v1/chat/completions"))
}
