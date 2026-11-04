//! The compiled context packet and its verifiable hash.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::canonical::{canonical_bytes, sha256_hex};
use crate::section::Section;
use crate::spec::SCHEMA;

/// The exact input an agent sees, plus the metadata needed to replay it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompiledContext {
    pub schema: String,
    pub spec_id: String,
    pub spec_version: u32,
    pub tokenizer: String,
    pub sections: Vec<Section>,
    pub prompt: String,
    pub data: Value,
    pub tokens: u32,
    pub sha256: String,
}

impl CompiledContext {
    /// Canonical bytes that the `sha256` field commits to. The hash field
    /// itself is excluded so verification is possible without recursion.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        let payload = HashPayload {
            schema: &self.schema,
            spec_id: &self.spec_id,
            spec_version: self.spec_version,
            tokenizer: &self.tokenizer,
            sections: &self.sections,
            prompt: &self.prompt,
            data: &self.data,
            tokens: self.tokens,
        };
        canonical_bytes(&payload)
    }

    /// Recompute the hash and compare it with the stored field.
    pub fn verify_hash(&self) -> Result<bool> {
        Ok(sha256_hex(&self.canonical_bytes()?) == self.sha256)
    }

    /// Build a packet with an empty hash field. The compiler fills it after
    /// canonicalizing everything else.
    pub(crate) fn new(
        spec_id: String,
        spec_version: u32,
        tokenizer: String,
        sections: Vec<Section>,
        prompt: String,
        data: Value,
        tokens: u32,
    ) -> Self {
        Self {
            schema: SCHEMA.to_owned(),
            spec_id,
            spec_version,
            tokenizer,
            sections,
            prompt,
            data,
            tokens,
            sha256: String::new(),
        }
    }
}

#[derive(Serialize)]
struct HashPayload<'a> {
    schema: &'a str,
    spec_id: &'a str,
    spec_version: u32,
    tokenizer: &'a str,
    sections: &'a [Section],
    prompt: &'a str,
    data: &'a Value,
    tokens: u32,
}
