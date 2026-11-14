//! Stages owned by the KnightLM cookbook. The trainer comes from
//! `blut-cookbook-standard`; so does dataset registration, which
//! `register_corpus` wraps to make it rerunnable.

pub mod claim_run;
pub mod evaluate_checkpoint;
pub mod materialize_corpus;
pub mod register_corpus;
pub mod register_run;

pub use claim_run::ClaimRun;
pub use evaluate_checkpoint::EvaluateCheckpoint;
pub use materialize_corpus::MaterializeCorpus;
pub use register_corpus::RegisterCorpus;
pub use register_run::RegisterRun;
