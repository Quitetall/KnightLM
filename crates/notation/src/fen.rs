//! FEN, X-FEN and Shredder-FEN, delegated to shakmaty for the heavy lifting.
//!
//! Parsing is stricter than shakmaty's `Fen::from_ascii`, which fills missing
//! fields with defaults and accepts underscores as separators. The hub must
//! "either normalize or fail explicitly", so this module insists on exactly
//! four or six fields and a well-formed board before delegating, and it never
//! repairs an illegal position (shakmaty's validation errors are surfaced).
//!
//! Castling notations accepted on input: `KQkq` (standard, or X-FEN when the
//! king or a rook is off its home square), and file letters (`HAha`,
//! Shredder-FEN, or X-FEN for inner rooks). Chess960 is resolved by
//! `knightlm_core::state::parse_chess`, so the same rules apply as for game replay.

use anyhow::{Context, Result, bail};
use knightlm_core::state::parse_chess;
use shakmaty::{CastlingMode, Chess, Color, EnPassantMode, Position, Rank, Square, fen::Fen};

/// How castling rights are spelled on output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastlingStyle {
    /// `KQkq` where unambiguous, file letters for inner rooks (X-FEN).
    XFen,
    /// Always file letters, white first, highest file first (`HAha`).
    Shredder,
}

/// A parsed FEN: the position, the castling mode it implies, and whether the
/// source carried the halfmove/fullmove clocks (6 fields) or not (4 fields).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FenPosition {
    pub position: Chess,
    pub mode: CastlingMode,
    pub has_clocks: bool,
}

/// Validate the shape of the four placement/state fields.
pub(crate) fn check_core_fields(fields: &[&str]) -> Result<()> {
    let board = fields[0];
    let ranks: Vec<&str> = board.split('/').collect();
    if ranks.len() != 8 {
        bail!("FEN board must have 8 ranks, found {}", ranks.len());
    }
    for rank in &ranks {
        let mut width = 0u32;
        let mut prev_digit = false;
        for c in rank.chars() {
            if let Some(d) = c.to_digit(10) {
                if !(1..=8).contains(&d) || prev_digit {
                    bail!("FEN rank {rank:?} has an invalid empty-square run");
                }
                width += d;
                prev_digit = true;
            } else if "pnbrqkPNBRQK".contains(c) {
                width += 1;
                prev_digit = false;
            } else {
                bail!("FEN rank {rank:?} has an unknown piece character {c:?}");
            }
        }
        if width != 8 {
            bail!("FEN rank {rank:?} covers {width} files, expected 8");
        }
    }
    if !matches!(fields[1], "w" | "b") {
        bail!("FEN side to move must be w or b, found {:?}", fields[1]);
    }
    let castling = fields[2];
    if castling != "-"
        && (castling.is_empty()
            || !castling
                .chars()
                .all(|c| matches!(c, 'K' | 'Q' | 'k' | 'q' | 'A'..='H' | 'a'..='h')))
    {
        bail!("FEN castling field {castling:?} is not -, KQkq or file letters");
    }
    let ep = fields[3];
    if ep != "-" {
        let b = ep.as_bytes();
        let ok = b.len() == 2
            && b[0].is_ascii_lowercase()
            && (b'a'..=b'h').contains(&b[0])
            && matches!(b[1], b'3' | b'6');
        if !ok {
            bail!("FEN en passant field {ep:?} is not - or a rank 3/6 square");
        }
    }
    Ok(())
}

/// Parse a four- or six-field FEN, X-FEN or Shredder-FEN as standard chess or
/// Chess960. Rejects anything else, including illegal positions.
pub fn parse_fen(text: &str) -> Result<FenPosition> {
    if text.contains('_') {
        bail!("FEN fields must be separated by spaces, not underscores");
    }
    let fields: Vec<&str> = text.split_whitespace().collect();
    if fields.len() != 4 && fields.len() != 6 {
        bail!("FEN must have 4 or 6 fields, found {}", fields.len());
    }
    check_core_fields(&fields)?;
    if fields.len() == 6 {
        fields[4]
            .parse::<u32>()
            .with_context(|| format!("FEN halfmove clock {:?} is not a number", fields[4]))?;
        let full: u32 = fields[5]
            .parse()
            .with_context(|| format!("FEN fullmove number {:?} is not a number", fields[5]))?;
        if full == 0 {
            bail!("FEN fullmove number must be at least 1");
        }
    }
    let (position, mode) = parse_chess(&fields.join(" "))?;
    Ok(FenPosition {
        position,
        mode,
        has_clocks: fields.len() == 6,
    })
}

fn shredder_castling<P: Position>(pos: &P) -> String {
    let rights = pos.castles().castling_rights();
    let mut out = String::new();
    for color in [Color::White, Color::Black] {
        let rank = color.fold_wb(Rank::First, Rank::Eighth);
        let mut files: Vec<char> = rights
            .into_iter()
            .filter(|sq: &Square| sq.rank() == rank)
            .map(|sq| {
                let c = char::from(b'a' + u8::from(sq.file()));
                if color == Color::White {
                    c.to_ascii_uppercase()
                } else {
                    c
                }
            })
            .collect();
        files.sort_by_key(|a| std::cmp::Reverse(a.to_ascii_lowercase()));
        out.extend(files);
    }
    if out.is_empty() { "-".into() } else { out }
}

/// Write the six-field FEN of any position in the requested castling style.
pub fn write_fen<P: Position>(pos: &P, style: CastlingStyle) -> String {
    let fen = Fen::from_position(pos, EnPassantMode::Legal).to_string();
    match style {
        CastlingStyle::XFen => fen,
        CastlingStyle::Shredder => {
            let mut f: Vec<String> = fen.split(' ').map(str::to_owned).collect();
            f[2] = shredder_castling(pos);
            f.join(" ")
        }
    }
}

/// Write the four-field form (placement, side, castling, en passant).
pub fn write_fen4<P: Position>(pos: &P, style: CastlingStyle) -> String {
    write_fen(pos, style)
        .split(' ')
        .take(4)
        .collect::<Vec<_>>()
        .join(" ")
}
