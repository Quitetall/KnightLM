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

pub const SCHEMA: &str = "rbc-provider/1";

/// A configured provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Provider {
    pub id: String,
    pub label: String,
    /// Wire shape. Only `openai_compatible` is spoken today; the field exists so
    /// a second shape is a new value rather than a new column.
    pub kind: String,
    pub base_url: String,
    /// The *name* of the secret: an env var, or `keyring:<service>/<account>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_ref: Option<String>,
    /// Models last seen at this endpoint, most recently fetched first.
    #[serde(default)]
    pub models: Vec<ModelCard>,
}

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

/// A provider answers a small request: it is reachable and the key works.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderTest {
    pub ok: bool,
    pub models: usize,
    /// Set when the test failed: what the endpoint said, trimmed.
    pub detail: String,
    pub latency_ms: u64,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_reference_names_a_secret_and_never_holds_one() {
        // The invariant the whole ticket rests on: what a row stores is a name.
        let provider = Provider {
            id: "p1".into(),
            label: "Local".into(),
            kind: "openai_compatible".into(),
            base_url: "http://127.0.0.1:11434/v1".into(),
            key_ref: Some("RBC_TEST_KEY_NAME".into()),
            models: vec![],
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
