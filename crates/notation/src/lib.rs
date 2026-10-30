//! Universal notation layer: SAN, format detection, and (later) PGN and the rest.
//!
//! This crate depends on `knightlm-core` for the canonical `GameState` and
//! adds no engine, model, or network access. Seams consumed by other crates:
//! `nota-reader`, `nota-detect`, `nota-annot` (the last two land in later
//! Warrants).

pub mod san;
pub mod pgn;
pub mod frc;
pub mod reader;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    San,
    Pgn,
}

pub fn detect(bytes: &[u8]) -> Format {
    // A deliberate, trivial first cut: any byte stream containing a '[' tag
    // header is treated as PGN; otherwise SAN. Real detection (and the
    // remaining variants) land with KLM-WAR-0004.
    if bytes.iter().any(|&b| b == b'[') {
        Format::Pgn
    } else {
        Format::San
    }
}

pub use san::{parse_san, to_san};
pub use frc::scharnagl_starts;
