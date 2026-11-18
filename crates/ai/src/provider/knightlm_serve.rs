//! The KnightLM move server (`python -m model.serve.server`, `POST /v1/move`).
//!
//! It is not a chat endpoint: it takes the canonical packet and answers one
//! move. This adapter sits behind the same [`Provider`] trait so a match seat
//! can name it. It reads the packet from the request `knightlm_move_request`
//! builds (the user message is a one-line instruction followed by the packet's
//! JSON), asks the server for a `constrained` move and returns the move as the
//! response text. The caller still validates it against the real position.

use serde_json::{Value, json};

use crate::protocol::{ErrorInfo, StopReason, Usage};
use crate::provider::{
    Cancellation, CompletionRequest, CompletionResponse, Provider, ProviderEvent, error,
    error_code, error_for_status,
};

/// The seat provider id that selects this adapter.
pub const SERVE_PROVIDER: &str = "local:knightlm-serve";
/// The seat model id for the served checkpoint.
pub const SERVE_MODEL: &str = "knightlm-serve";

#[derive(Debug)]
pub struct KnightLmServeProvider {
    url: String,
    agent: ureq::Agent,
}

impl KnightLmServeProvider {
    /// `url` is the full move endpoint, for example `http://127.0.0.1:8000/v1/move`.
    pub fn new(url: impl Into<String>, timeout_secs: u64) -> Result<Self, ErrorInfo> {
        let url = url.into();
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(error(
                error_code::NOT_CONFIGURED,
                "the KnightLM move endpoint must be an http:// or https:// URL",
                false,
            ));
        }
        let agent = ureq::Agent::new_with_config(
            ureq::config::Config::builder()
                .http_status_as_error(false)
                .timeout_global(Some(std::time::Duration::from_secs(timeout_secs)))
                .build(),
        );
        Ok(Self { url, agent })
    }
}

/// The packet JSON in the last user message: everything after its first line.
fn packet_of(request: &CompletionRequest) -> Result<Value, ErrorInfo> {
    let text = request
        .messages
        .iter()
        .rev()
        .find_map(|message| message.content.as_deref())
        .and_then(|content| content.split_once('\n'))
        .map(|(_, rest)| rest)
        .ok_or_else(|| {
            error(
                error_code::BAD_REQUEST,
                "the request carries no packet",
                false,
            )
        })?;
    serde_json::from_str(text.trim()).map_err(|_| {
        error(
            error_code::BAD_REQUEST,
            "the request packet is not JSON",
            false,
        )
    })
}

impl Provider for KnightLmServeProvider {
    fn name(&self) -> &str {
        "knightlm_serve"
    }

    fn complete(
        &self,
        request: &CompletionRequest,
        cancel: &Cancellation,
        _on_event: &mut dyn FnMut(ProviderEvent),
    ) -> Result<CompletionResponse, ErrorInfo> {
        cancel.check()?;
        let packet = packet_of(request)?;
        let version = packet["schema_version"].clone();
        let body = json!({
            "packet": packet,
            "packet_format_version": version,
            "mode": "constrained",
        });
        let mut response = self
            .agent
            .post(&self.url)
            .send_json(body)
            .map_err(|failure| {
                error(
                    error_code::NETWORK,
                    format!("could not reach the KnightLM move server: {failure}"),
                    true,
                )
            })?;
        let status = response.status().as_u16();
        let text = response.body_mut().read_to_string().unwrap_or_default();
        if status >= 400 {
            return Err(error_for_status(status, &text));
        }
        let value: Value = serde_json::from_str(&text).map_err(|_| {
            error(
                error_code::PROTOCOL,
                "the move server did not answer JSON",
                false,
            )
        })?;
        let mv = value["move"].as_str().ok_or_else(|| {
            error(
                error_code::PROTOCOL,
                "the move server's answer has no `move`",
                false,
            )
        })?;
        Ok(CompletionResponse {
            content: mv.to_owned(),
            reasoning: None,
            tool_calls: Vec::new(),
            usage: Usage {
                tokens_in: value["input_tokens"].as_u64().unwrap_or(0) as u32,
                tokens_out: value["output_tokens"].as_u64().unwrap_or(0) as u32,
                latency_ms: value["latency_ms"].as_f64().unwrap_or(0.0) as u64,
                ..Default::default()
            },
            stop_reason: StopReason::EndTurn,
            provider_request_id: None,
        })
    }
}
