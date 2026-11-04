//! Deterministic context compiler (PKG-010/011/013/015).
//!
//! `rbc-context` builds the exact bytes an assistant sees. It is pure: given
//! the same [`ContextSpec`] and [`ContextInput`], [`compile`] returns the same
//! prompt, structured data and SHA-256 on every platform. It performs no
//! network, database, engine, clock or random access.
//!
//! The canonical order is [`SectionId::ALL`]; caller map or vector order never
//! affects the result. JSON is serialized with sorted keys, text uses LF, and
//! the hash covers the packet without its own `sha256` field.
//!
//! ```
//! use rbc_context::{ContextInput, ContextSpec, compile};
//!
//! let spec = ContextSpec::chat_v1();
//! let input = ContextInput::start();
//! let context = compile(&spec, &input).expect("start position compiles");
//! assert!(context.verify_hash().expect("hash is representable"));
//! ```

pub mod annotations;
pub mod canonical;
pub mod compile;
pub mod evidence;
pub mod input;
pub mod output;
pub mod section;
pub mod spec;

mod position;
mod render;

pub use annotations::{Annotation, Annotations, Arrow};
pub use canonical::{
    canonical_bytes, canonical_json, canonical_string, estimate_tokens, fingerprint,
    normalize_text, sha256_hex,
};
pub use compile::{canonical_ascii, canonical_fen, compile};
pub use evidence::{
    BoardSummary, DatabaseEvidence, DatabaseGame, EngineEvidence, EngineLine, Evidence, OpeningRow,
    OpeningsEvidence,
};
pub use input::{ContextInput, Mode, Side, Turn, TurnRole};
pub use output::CompiledContext;
pub use section::Section;
pub use spec::{ContextSpec, PROMPT_TEMPLATE_VERSION, SCHEMA, SectionId, TOKENIZER_ESTIMATE_V1};
