//! The per-section record emitted by the compiler.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One rendered section. `content` is the canonical text; `data` is the
/// structured form for tooling and training.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub id: String,
    pub version: u32,
    pub priority: u8,
    pub required: bool,
    pub content: String,
    pub data: Option<Value>,
    pub tokens: u32,
    pub included: bool,
    pub truncated: bool,
}

impl Section {
    /// Whether this section contributes to the prompt and token total.
    pub fn contributes(&self) -> bool {
        self.included
    }
}
