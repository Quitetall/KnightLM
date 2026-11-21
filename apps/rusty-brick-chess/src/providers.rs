//! The provider registry: where a model endpoint is, which secret it needs, what
//! it can do, and whether it answers.
//!
//! The secret itself is never stored. A row keeps a `key_ref`, a *name* that is
//! looked up at call time — an environment variable, or a keyring entry. That is
//! the whole point: a database dumped, a log pasted into a ticket, or a compiled
//! context inspected must never yield the key. `resolve_secret` is the only
//! function that touches one, and it returns the value to the caller without
//! recording it anywhere.

use serde::{Deserialize, Serialize};

/// What a model can do, as far as this build cares.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ModelCard {
    pub id: String,
    #[serde(default)]
    pub context: Option<u32>,
    /// Whether the model accepts images. Derived from the id when the endpoint
    /// does not say: providers report a list of ids and nothing else.
    #[serde(default)]
    pub vision: bool,
    /// Whether the model can be asked to think before answering.
    #[serde(default)]
    pub reasoning: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalEndpoint {
    pub label: String,
    pub base_url: String,
    /// True when a models list actually came back.
    pub responding: bool,
    pub models: Vec<String>,
}

/// Guess capabilities from an id, because `/v1/models` returns little else.
pub fn capabilities_of(id: &str) -> ModelCard {
    let lower = id.to_lowercase();
    let vision = ["vision", "vl", "llava", "bakllava", "moondream", "qwen2-vl"]
        .iter()
        .any(|needle| lower.contains(needle));
    let reasoning = ["reason", "think", "o1", "o3", "r1", "qwq"]
        .iter()
        .any(|needle| lower.contains(needle));
    // Context windows the common families publish. Unknown ids get None rather
    // than a made-up number: a wrong window silently truncates a prompt.
    let context = if lower.contains("gpt-4o") || lower.contains("gpt-4.1") {
        Some(128_000)
    } else if lower.contains("claude") {
        Some(200_000)
    } else if lower.contains("gemini") {
        Some(1_048_576)
    } else if lower.contains("qwen") {
        Some(32_768)
    } else if lower.contains("llama3") || lower.contains("llama-3") {
        Some(8_192)
    } else {
        None
    };
    ModelCard {
        id: id.to_string(),
        context,
        vision,
        reasoning,
    }
}

/// Where a secret comes from. Named so a caller can say which link of the chain
/// answered, without ever holding the value in a log line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecretSource {
    Env,
    Keyring,
}

/// Resolve a `key_ref` into a secret.
///
/// The chain is deliberately short and explicit:
/// - `keyring:<service>/<account>` asks `secret-tool` for that entry;
/// - anything else is an environment variable name, which is how `secrets run`
///   injects keys on this machine.
///
/// A missing secret is `Ok(None)` rather than an error: an endpoint that needs no
/// key is the common local case, and the caller decides whether that is fatal.
pub fn resolve_secret(key_ref: &str) -> Result<Option<(String, SecretSource)>, String> {
    let reference = key_ref.trim();
    if reference.is_empty() {
        return Ok(None);
    }
    if let Some(rest) = reference.strip_prefix("keyring:") {
        let (service, account) = rest
            .split_once('/')
            .ok_or_else(|| "a keyring reference is keyring:<service>/<account>".to_string())?;
        let output = std::process::Command::new("secret-tool")
            .args(["lookup", "service", service, "account", account])
            .output()
            .map_err(|error| format!("secret-tool is not available: {error}"))?;
        if !output.status.success() {
            return Ok(None);
        }
        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Ok((!value.is_empty()).then_some((value, SecretSource::Keyring)));
    }
    match std::env::var(reference) {
        Ok(value) if !value.trim().is_empty() => Ok(Some((value, SecretSource::Env))),
        _ => Ok(None),
    }
}

/// The local endpoints worth probing, in the order a person is likely to have
/// them running.
pub const LOCAL_CANDIDATES: &[(&str, &str)] = &[
    ("LAMU", "http://127.0.0.1:8020/v1"),
    ("Ollama", "http://127.0.0.1:11434/v1"),
    ("LM Studio", "http://127.0.0.1:1234/v1"),
    ("vLLM", "http://127.0.0.1:8000/v1"),
    ("llama.cpp", "http://127.0.0.1:8080/v1"),
    ("Jan", "http://127.0.0.1:1337/v1"),
];

/// Where the served KnightLM checkpoint's move endpoint is
/// (`python -m model.serve.server`, `POST /v1/move`). A URL, never a secret:
/// `RBC_KNIGHTLM_SERVE_URL` overrides the loopback default.
pub fn knightlm_serve_url() -> String {
    std::env::var("RBC_KNIGHTLM_SERVE_URL")
        .ok()
        .filter(|value| acceptable_base_url(value))
        .unwrap_or_else(|| "http://127.0.0.1:8000/v1/move".into())
}

/// Whether a URL is one this build will fetch. Providers are configured by the
/// person using the app, so the risk is a typo or a stale paste, not an attack;
/// the check that matters is that nothing but http(s) is offered to the fetcher.
pub fn acceptable_base_url(base: &str) -> bool {
    let trimmed = base.trim();
    (trimmed.starts_with("http://") || trimmed.starts_with("https://")) && !trimmed.contains(' ')
}

/// The models path for a base URL, tolerating a trailing slash and a base that
/// already ends in `/v1`.
pub fn models_url(base: &str) -> String {
    let trimmed = base.trim().trim_end_matches('/');
    format!("{trimmed}/models")
}

/// The label, model name and base URL of the "KnightLM server" preset.
///
/// It is an ordinary OpenAI-compatible provider row: the base URL is the served
/// checkpoint's loopback `/v1`, so the chat route is `<base>/chat/completions`.
/// `RBC_KNIGHTLM_SERVE_URL` moves it (the same variable the move seat reads).
pub const KNIGHTLM_PRESET_LABEL: &str = "KnightLM server";
pub const KNIGHTLM_PRESET_MODEL: &str = "knightlm";

pub fn knightlm_preset_base_url() -> String {
    let serve = knightlm_serve_url();
    serve
        .strip_suffix("/move")
        .map_or_else(|| "http://127.0.0.1:8000/v1".to_string(), str::to_string)
}

/// A named starting point for a provider row. It carries no secret.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderPreset {
    pub id: &'static str,
    pub label: String,
    pub base_url: String,
    pub model: String,
    pub key_ref: Option<String>,
    pub note: &'static str,
}

pub fn presets() -> Vec<ProviderPreset> {
    vec![ProviderPreset {
        id: "knightlm",
        label: KNIGHTLM_PRESET_LABEL.into(),
        base_url: knightlm_preset_base_url(),
        model: KNIGHTLM_PRESET_MODEL.into(),
        key_ref: None,
        note: "A KnightLM checkpoint served locally (python -m model.serve.server). It speaks the same OpenAI-compatible API as any other provider.",
    }]
}

pub fn preset(id: &str) -> Option<ProviderPreset> {
    presets().into_iter().find(|preset| preset.id == id)
}

/// Accept a key reference (a name) and refuse anything that looks like a key.
///
/// A reference is an environment variable name (`[A-Za-z_][A-Za-z0-9_]*`) or
/// `keyring:<service>/<account>`. The refusal never repeats what it was given,
/// so a pasted key does not reach a response or a log.
pub fn validate_key_ref(reference: &str) -> Result<(), String> {
    let reference = reference.trim();
    if reference.is_empty() {
        return Ok(());
    }
    let refuse = || {
        Err(
            "a key reference is an environment variable name or keyring:<service>/<account>, \
             never the key itself"
                .to_string(),
        )
    };
    if let Some(rest) = reference.strip_prefix("keyring:") {
        let good = rest.split_once('/').is_some_and(|(service, account)| {
            let ok = |part: &str| {
                !part.is_empty()
                    && part.len() <= 128
                    && part
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "._-@:".contains(c))
            };
            ok(service) && ok(account)
        });
        return if good { Ok(()) } else { refuse() };
    }
    let mut chars = reference.chars();
    let first_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    let rest_ok = chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    let lower = reference.to_ascii_lowercase();
    let key_shaped = ["sk_", "pk_", "rk_", "ghp_", "gho_", "xox", "aiza", "bearer"]
        .iter()
        .any(|prefix| lower.starts_with(prefix));
    if first_ok && rest_ok && reference.len() <= 128 && !key_shaped {
        Ok(())
    } else {
        refuse()
    }
}

/// Why a connection test failed. The code is stable; the message is for people.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProbeError {
    pub code: &'static str,
    pub message: String,
}

impl ProbeError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// The codes a connection test can return, in the order the checks run.
pub mod probe_code {
    pub const INVALID_URL: &str = "invalid_url";
    pub const KEY_UNUSABLE: &str = "key_unusable";
    pub const KEY_UNRESOLVED: &str = "key_unresolved";
    pub const TIMEOUT: &str = "timeout";
    pub const UNREACHABLE: &str = "unreachable";
    pub const UNAUTHORIZED: &str = "unauthorized";
    pub const NOT_FOUND: &str = "not_found";
    pub const SERVER_ERROR: &str = "server_error";
    pub const BAD_RESPONSE: &str = "bad_response";
    pub const MODEL_MISSING: &str = "model_missing";
}

/// Ask an endpoint for its model list, with typed failures.
///
/// Blocking: call it from `spawn_blocking`. `key_ref` is resolved here and the
/// value goes only into the request header.
pub fn probe_models(
    base_url: &str,
    key_ref: Option<&str>,
    wanted_model: Option<&str>,
) -> Result<Vec<String>, ProbeError> {
    if !acceptable_base_url(base_url) {
        return Err(ProbeError::new(
            probe_code::INVALID_URL,
            "the base URL is not http(s)",
        ));
    }
    let key = match key_ref.map(str::trim).filter(|r| !r.is_empty()) {
        Some(reference) => {
            validate_key_ref(reference)
                .map_err(|message| ProbeError::new(probe_code::KEY_UNUSABLE, message))?;
            match resolve_secret(reference) {
                Ok(Some((value, _))) => Some(value),
                Ok(None) => {
                    return Err(ProbeError::new(
                        probe_code::KEY_UNRESOLVED,
                        "the key reference names nothing set on this machine",
                    ));
                }
                Err(message) => return Err(ProbeError::new(probe_code::KEY_UNUSABLE, message)),
            }
        }
        None => None,
    };
    let agent = ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .http_status_as_error(false)
            .timeout_global(Some(std::time::Duration::from_secs(4)))
            .build(),
    );
    let mut request = agent
        .get(&models_url(base_url))
        .header("accept", "application/json");
    if let Some(key) = key.as_deref() {
        request = request.header("authorization", format!("Bearer {key}"));
    }
    let mut response = request.call().map_err(|error| {
        if matches!(error, ureq::Error::Timeout(_)) {
            ProbeError::new(probe_code::TIMEOUT, "the endpoint did not answer in time")
        } else {
            ProbeError::new(
                probe_code::UNREACHABLE,
                format!("could not reach the endpoint: {error}"),
            )
        }
    })?;
    let status = response.status().as_u16();
    let body = response.body_mut().read_to_string().unwrap_or_default();
    if status >= 400 {
        let trimmed: String = body.chars().take(160).collect();
        let (code, hint) = match status {
            401 | 403 => (
                probe_code::UNAUTHORIZED,
                "the endpoint refused the credentials",
            ),
            404 => (
                probe_code::NOT_FOUND,
                "the endpoint has no models route here; check the base URL (it usually ends in /v1)",
            ),
            500.. => (probe_code::SERVER_ERROR, "the endpoint failed"),
            _ => (probe_code::SERVER_ERROR, "the endpoint refused the request"),
        };
        return Err(ProbeError::new(
            code,
            format!("{hint} ({status}): {trimmed}"),
        ));
    }
    let json: serde_json::Value = serde_json::from_str(&body).map_err(|error| {
        ProbeError::new(
            probe_code::BAD_RESPONSE,
            format!("unreadable answer: {error}"),
        )
    })?;
    let entries = json
        .get("data")
        .and_then(|data| data.as_array())
        .or_else(|| json.get("models").and_then(|models| models.as_array()))
        .ok_or_else(|| {
            ProbeError::new(
                probe_code::BAD_RESPONSE,
                "the answer has no model list (expected data[] or models[])",
            )
        })?;
    let ids: Vec<String> = entries
        .iter()
        .filter_map(|entry| {
            entry
                .get("id")
                .and_then(|id| id.as_str())
                .or_else(|| entry.get("name").and_then(|n| n.as_str()))
                .map(str::to_string)
        })
        .collect();
    // Some servers list nothing; a model that is not in a non-empty list is an error.
    if let Some(model) = wanted_model.map(str::trim).filter(|m| !m.is_empty())
        && !ids.is_empty()
        && !ids.iter().any(|id| id == model)
    {
        return Err(ProbeError::new(
            probe_code::MODEL_MISSING,
            format!("the endpoint answered but does not list the model `{model}`"),
        ));
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_reference_names_a_secret_and_never_holds_one() {
        // The invariant the whole ticket rests on: what a row stores is a name.
        // Asserted against the row type the routes actually read, so it cannot
        // drift away from what is stored.
        let provider = rbc_db::ProviderRow {
            id: "p1".into(),
            label: "Local".into(),
            kind: "openai_compatible".into(),
            base_url: "http://127.0.0.1:11434/v1".into(),
            key_ref: Some("RBC_TEST_KEY_NAME".into()),
            models_json: "[]".into(),
            caps_json: "{}".into(),
            created: 0,
            updated: 0,
        };
        let json = serde_json::to_string(&provider).expect("json");
        assert!(json.contains("RBC_TEST_KEY_NAME"));
        assert!(!json.contains("sk-"), "no secret-shaped value is stored");
    }

    #[test]
    fn an_empty_reference_resolves_to_nothing_rather_than_failing() {
        assert_eq!(resolve_secret("").expect("resolve"), None);
        assert_eq!(resolve_secret("   ").expect("resolve"), None);
    }

    #[test]
    fn an_environment_reference_resolves_to_its_value() {
        // SAFETY: tests in this binary do not read this name concurrently.
        unsafe { std::env::set_var("RBC_TEST_PROVIDER_KEY", "secret-value") };
        let (value, source) = resolve_secret("RBC_TEST_PROVIDER_KEY")
            .expect("resolve")
            .expect("a value");
        assert_eq!(value, "secret-value");
        assert_eq!(source, SecretSource::Env);
        unsafe { std::env::remove_var("RBC_TEST_PROVIDER_KEY") };
    }

    #[test]
    fn a_missing_environment_reference_is_none_not_an_error() {
        assert_eq!(
            resolve_secret("RBC_TEST_PROVIDER_KEY_THAT_IS_NOT_SET").expect("resolve"),
            None
        );
    }

    #[test]
    fn a_keyring_reference_without_an_account_is_refused_by_name() {
        let error = resolve_secret("keyring:only-service").expect_err("refused");
        assert!(error.contains("keyring:<service>/<account>"), "{error}");
    }

    #[test]
    fn capabilities_are_read_off_the_id_and_left_unknown_when_unrecognised() {
        assert!(capabilities_of("qwen2-vl-7b").vision);
        assert!(!capabilities_of("qwen2-vl-7b").reasoning);
        assert!(capabilities_of("o3-mini").reasoning);
        assert_eq!(capabilities_of("gpt-4o-mini").context, Some(128_000));
        assert_eq!(capabilities_of("some-unknown-model").context, None);
    }

    #[test]
    fn a_url_is_only_offered_to_the_fetcher_when_it_is_http() {
        assert!(acceptable_base_url("http://127.0.0.1:11434/v1"));
        assert!(acceptable_base_url("https://api.example.com/v1"));
        assert!(!acceptable_base_url("file:///etc/passwd"));
        assert!(!acceptable_base_url("ftp://example.com"));
        assert!(!acceptable_base_url("has a space"));
    }

    #[test]
    fn a_key_reference_is_a_name_and_a_key_shaped_value_is_refused() {
        for good in [
            "OPENAI_API_KEY",
            "_x1",
            "keyring:rbc/openai",
            "keyring:a.b/me@x",
            "",
        ] {
            assert!(validate_key_ref(good).is_ok(), "{good}");
        }
        for bad in [
            "sk-abcdef0123456789",
            "sk_live_abcdef",
            "has space",
            "keyring:onlyservice",
            "keyring:/acct",
            "1LEADING_DIGIT",
            "ghp_abcdef",
        ] {
            let error = validate_key_ref(bad).expect_err(bad);
            assert!(
                !error.contains(bad),
                "the refusal repeats the input: {error}"
            );
        }
    }

    #[test]
    fn the_knightlm_preset_points_at_the_local_chat_base() {
        let preset = preset("knightlm").expect("preset");
        assert_eq!(preset.label, "KnightLM server");
        assert_eq!(preset.model, "knightlm");
        assert!(preset.base_url.ends_with("/v1"), "{}", preset.base_url);
        assert!(preset.key_ref.is_none());
        assert!(super::preset("nope").is_none());
    }

    #[test]
    fn the_models_path_does_not_double_its_slash() {
        assert_eq!(
            models_url("http://127.0.0.1:11434/v1/"),
            "http://127.0.0.1:11434/v1/models"
        );
        assert_eq!(
            models_url("http://127.0.0.1:11434/v1"),
            "http://127.0.0.1:11434/v1/models"
        );
    }
}
