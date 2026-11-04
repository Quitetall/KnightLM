//! Comparing the context a turn was sent with the context rebuilt today.
//!
//! Replay is only useful if it can say *what* changed. A turn stores the exact
//! compiled context it used; rebuilding it later and hashing the whole thing
//! answers "same or not", which is not enough to debug a drift. This diff names
//! the sections that appeared, vanished or changed, and gives each a digest so
//! a reader can see it is a different section rather than a different order.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// One section whose content differs between the two contexts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionChange {
    /// The section id, for example `position` or `legal_moves`.
    pub id: String,
    /// Digest of the stored content.
    pub stored_sha256: String,
    /// Digest of the rebuilt content.
    pub rebuilt_sha256: String,
}

/// How a stored context differs from a rebuilt one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ContextDiff {
    /// True when every section is present in both and byte-identical.
    pub identical: bool,
    /// Section ids only the rebuilt context has.
    pub added: Vec<String>,
    /// Section ids only the stored context has.
    pub removed: Vec<String>,
    /// Section ids in both with different content.
    pub changed: Vec<SectionChange>,
}

impl ContextDiff {
    /// Whether a reader needs to look at this.
    pub fn is_clean(&self) -> bool {
        self.identical
    }
}

/// Diff two compiled contexts (their canonical `data`, section by section).
///
/// Order does not matter: sections are matched by id, so a rebuild that emits
/// the same sections in the same order is identical, and one that reorders them
/// but says the same things still is. A section whose *value* differs is
/// reported with both digests.
pub fn diff_contexts(stored: &Value, rebuilt: &Value) -> ContextDiff {
    let stored_sections = sections(stored);
    let rebuilt_sections = sections(rebuilt);

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();

    for (id, rebuilt_value) in &rebuilt_sections {
        match stored_sections.get(id) {
            None => added.push(id.clone()),
            Some(stored_value) => {
                let stored_sha = digest(stored_value);
                let rebuilt_sha = digest(rebuilt_value);
                if stored_sha != rebuilt_sha {
                    changed.push(SectionChange {
                        id: id.clone(),
                        stored_sha256: stored_sha,
                        rebuilt_sha256: rebuilt_sha,
                    });
                }
            }
        }
    }
    for id in stored_sections.keys() {
        if !rebuilt_sections.contains_key(id) {
            removed.push(id.clone());
        }
    }

    // A stable reading order: the diff is shown, not just counted.
    added.sort();
    removed.sort();
    changed.sort_by(|a, b| a.id.cmp(&b.id));

    ContextDiff {
        identical: added.is_empty() && removed.is_empty() && changed.is_empty(),
        added,
        removed,
        changed,
    }
}

/// The sections of a compiled context: a top-level object of section id → content.
///
/// A value that is not an object (or an empty one) is treated as having no
/// sections, which makes a mismatch show up as everything added or removed
/// rather than as a panic.
fn sections(context: &Value) -> std::collections::BTreeMap<String, Value> {
    match context.as_object() {
        Some(object) => object
            .iter()
            .map(|(id, value)| (id.clone(), value.clone()))
            .collect(),
        None => std::collections::BTreeMap::new(),
    }
}

/// `sha256:<hex>` of a value's canonical JSON.
///
/// `serde_json` writes object keys in sorted order, so the digest depends on
/// the value and not on how it happened to be built.
pub fn digest(value: &Value) -> String {
    let canonical = serde_json::to_string(value).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    format!("sha256:{}", hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn context_diff_of_identical_contexts_is_clean() {
        let context = json!({"system": {"mode": "chat"}, "position": {"fen": "8/8/8/8/8/8/8/8"}});
        let diff = diff_contexts(&context, &context.clone());
        assert!(diff.identical);
        assert!(diff.is_clean());
        assert!(diff.added.is_empty() && diff.removed.is_empty() && diff.changed.is_empty());
    }

    #[test]
    fn context_diff_reports_added_and_removed_sections() {
        let stored = json!({"system": 1, "position": 2});
        let rebuilt = json!({"system": 1, "engine": 3});
        let diff = diff_contexts(&stored, &rebuilt);
        assert!(!diff.identical);
        assert_eq!(diff.added, vec!["engine"]);
        assert_eq!(diff.removed, vec!["position"]);
        assert!(diff.changed.is_empty());
    }

    #[test]
    fn context_diff_reports_changed_sections_with_both_digests() {
        let stored = json!({"position": {"fen": "start"}});
        let rebuilt = json!({"position": {"fen": "end"}});
        let diff = diff_contexts(&stored, &rebuilt);
        assert_eq!(diff.changed.len(), 1);
        assert_eq!(diff.changed[0].id, "position");
        assert_ne!(diff.changed[0].stored_sha256, diff.changed[0].rebuilt_sha256);
        assert!(diff.changed[0].stored_sha256.starts_with("sha256:"));
    }

    #[test]
    fn context_diff_ignores_key_order() {
        let stored = json!({"a": 1, "b": {"x": 1, "y": 2}});
        let rebuilt = json!({"b": {"y": 2, "x": 1}, "a": 1});
        assert!(diff_contexts(&stored, &rebuilt).identical);
    }

    #[test]
    fn context_diff_treats_a_non_object_as_empty() {
        let diff = diff_contexts(&json!(null), &json!({"system": 1}));
        assert_eq!(diff.added, vec!["system"]);
        assert!(!diff.identical);
    }
}
