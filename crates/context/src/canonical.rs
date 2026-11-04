//! Canonical JSON, text normalization, token estimation and SHA-256.
//!
//! The rules here are the ones the spec version fixes:
//!
//! * object keys are sorted by their UTF-8 bytes;
//! * no insignificant whitespace; compact separators;
//! * arrays keep the order the renderer gave them (renderers sort explicitly);
//! * text is UTF-8 with LF line endings and no trailing whitespace;
//! * `estimate-v1` counts Unicode scalar values at four characters per token,
//!   rounded up. It over-counts real BPE tokens for English and code, which is
//!   the conservative direction for a budget.

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Render a JSON value with sorted keys and no insignificant whitespace.
pub fn canonical_json(value: &Value) -> String {
    let mut out = String::new();
    write_value(value, &mut out);
    out
}

/// Serialize any value to canonical JSON bytes.
pub fn canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let value = serde_json::to_value(value).context("value is not representable as JSON")?;
    Ok(canonical_json(&value).into_bytes())
}

/// Serialize any value to canonical JSON text.
pub fn canonical_string<T: Serialize>(value: &T) -> Result<String> {
    Ok(String::from_utf8(canonical_bytes(value)?).expect("canonical JSON is always valid UTF-8"))
}

/// SHA-256 over `bytes`, lower-case hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// SHA-256 of a value's canonical JSON form.
pub fn fingerprint<T: Serialize>(value: &T) -> Result<String> {
    Ok(sha256_hex(&canonical_bytes(value)?))
}

/// Conservative token count for `estimate-v1`.
///
/// Four Unicode scalar values per token, rounded up; empty text is zero. The
/// rule is deliberately simple so every platform agrees byte-for-byte.
pub fn estimate_tokens(text: &str) -> u32 {
    let chars = text.chars().count();
    if chars == 0 {
        return 0;
    }
    u32::try_from(chars.div_ceil(4)).unwrap_or(u32::MAX)
}

/// Normalize text to LF, strip trailing whitespace on each line, and remove
/// trailing blank lines. Leading indentation is preserved.
pub fn normalize_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let unified = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = unified.split('\n').map(str::trim_end).collect();
    let end = lines
        .iter()
        .rposition(|line| !line.is_empty())
        .map_or(0, |index| index + 1);
    for (index, line) in lines[..end].iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(line);
    }
    out
}

fn write_value(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(number) => out.push_str(&number.to_string()),
        Value::String(text) => {
            out.push_str(&serde_json::to_string(text).expect("a JSON string always serializes"));
        }
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_value(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).expect("a JSON key always serializes"));
                out.push(':');
                if let Some(item) = map.get(key) {
                    write_value(item, out);
                }
            }
            out.push('}');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_json_sorts_nested_keys() {
        let value = json!({"b": 1, "a": {"z": 2, "y": [{"q": 4, "p": 3}]}});
        assert_eq!(
            canonical_json(&value),
            r#"{"a":{"y":[{"p":3,"q":4}],"z":2},"b":1}"#
        );
    }

    #[test]
    fn estimate_is_conservative_and_stable() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("a"), 1);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
        assert_eq!(estimate_tokens("éééé"), 1);
    }

    #[test]
    fn normalization_uses_lf_and_strips_trailing_space() {
        assert_eq!(normalize_text("a  \r\nb\t\r\n\r\n"), "a\nb");
    }
}
