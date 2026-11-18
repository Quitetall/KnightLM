//! Universal notation layer: SAN, PGN, FEN/X-FEN/Shredder-FEN, EPD, UCI, and format detection.
//!
//! This crate depends on `knightlm-core` for the canonical `GameState` and
//! adds no engine, model, or network access. Seams consumed by other crates:
//! `nota-reader`, `nota-detect`, `nota-annot` (the last two land in later
//! Warrants).

pub mod epd;
pub mod fen;
pub mod frc;
pub mod pgn;
pub mod reader;
pub mod san;
pub mod uci;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    San,
    Pgn,
    Fen,
    Epd,
    UciPosition,
    UciMoves,
}

/// Sniff the notation of `bytes`. Detection looks at shape only and never
/// validates; run the matching parser, which fails explicitly on bad input.
pub fn detect(bytes: &[u8]) -> Format {
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim_start();
    if trimmed.starts_with("position ") || trimmed.trim() == "position" {
        return Format::UciPosition;
    }
    if bytes.contains(&b'[') {
        return Format::Pgn;
    }
    let fields: Vec<&str> = trimmed.split_whitespace().collect();
    if fields.len() >= 4 && fields[0].split('/').count() == 8 && matches!(fields[1], "w" | "b") {
        let fenish_tail = fields.len() == 4
            || (fields.len() == 6 && fields[4..].iter().all(|f| f.parse::<u32>().is_ok()));
        return if fenish_tail {
            Format::Fen
        } else {
            Format::Epd
        };
    }
    if !fields.is_empty() && fields.iter().all(|t| is_uci_move_token(t)) {
        return Format::UciMoves;
    }
    Format::San
}

fn is_uci_move_token(t: &str) -> bool {
    let b = t.as_bytes();
    let sq = |f: u8, r: u8| (b'a'..=b'h').contains(&f) && (b'1'..=b'8').contains(&r);
    match b.len() {
        4 => sq(b[0], b[1]) && sq(b[2], b[3]),
        5 => sq(b[0], b[1]) && sq(b[2], b[3]) && matches!(b[4], b'q' | b'r' | b'b' | b'n'),
        _ => false,
    }
}

pub use frc::scharnagl_starts;
pub use san::{parse_san, to_san};
