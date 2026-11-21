//! The wire contract between RBC (the app) and KnightLM (the ML path).
//!
//! This crate holds only what both sides must agree on, and nothing that runs a model or an app:
//!
//! - the versioned position packet type ([`ContextPacket`], [`PACKET_FORMAT_VERSION`]);
//! - the model tool schemas and the prompt and completion goldens (`schemas/*.json`), embedded
//!   here as strings and read directly by `model/prompt.py`;
//! - the OpenAI chat-completions request and response types in [`openai`], including the
//!   namespaced `knightlm` extension the serving side adds.
//!
//! `docs/model/seam.md` says what each side may import and how the contract is versioned.

pub mod openai;

pub use knightlm_core::SCHEMA_VERSION as PACKET_FORMAT_VERSION;
pub use knightlm_core::state::ContextPacket;

/// Version of this wire contract. Bump when a type or golden here changes incompatibly.
pub const CONTRACT_VERSION: u32 = 1;

/// The model tool schemas by mode, `{"schema_version": str, "modes": {mode: [tool]}}`.
pub const MODEL_TOOLS_JSON: &str = include_str!("../schemas/model_tools.json");
/// Prompt goldens the Rust and Python renderers must both reproduce byte for byte.
pub const MODEL_PROMPTS_JSON: &str = include_str!("../schemas/model_prompts.json");
/// Play completion cases both parsers answer identically.
pub const PLAY_COMPLETION_CASES_JSON: &str = include_str!("../schemas/play_completion_cases.json");
/// Chat summary cases both summarizers answer identically.
pub const CHAT_SUMMARY_CASES_JSON: &str = include_str!("../schemas/chat_summary_cases.json");
/// Coach `Provisional:` line cases both trackers answer identically.
pub const PROVISIONAL_CASES_JSON: &str = include_str!("../schemas/provisional_cases.json");
/// An example `chat.completion` the serving side produces for a play-mode request.
pub const CHAT_COMPLETION_EXAMPLE_JSON: &str =
    include_str!("../schemas/chat_completion_example.json");

/// An example coach-turn `chat.completion`: the move in `knightlm.coach`, coach tool calls to run.
pub const CHAT_COMPLETION_COACH_EXAMPLE_JSON: &str =
    include_str!("../schemas/chat_completion_coach_example.json");

/// Absolute path of the schemas directory in this checkout, for tests that regenerate goldens.
pub fn schemas_dir() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/schemas")
}
