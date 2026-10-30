//! Renderer-agnostic scene: what any front end needs to draw a position.
//! No HTML, no SVG, no framework types — just squares, pieces and highlights.
//!
//! FEN parsing and the check square come from shakmaty; this module only maps
//! them into the small types the renderers consume.

use crate::command::Shape;
use crate::pieces::PieceSet;
use crate::widget::{Orientation, Theme, Widget};
use shakmaty::{
    variant::{Variant, VariantPosition},
    ByColor, ByRole, CastlingMode, Color as ShakColor, Position, Role as ShakRole, Square,
};
use knightlm_core::state::{castling_mode_for_fen, parse_variant_fen, variant_for_name};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    White,
    Black,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    King,
    Queen,
    Rook,
    Bishop,
    Knight,
    Pawn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacedPiece {
    pub color: Color,
    pub kind: Kind,
}

#[derive(Debug, Clone)]
pub struct Scene {
    /// 64 squares, index 0 = a1, index 63 = h8.
    pub board: [Option<PlacedPiece>; 64],
    pub orientation: Orientation,
    pub theme: Theme,
    pub pieces: PieceSet,
    pub selected: Option<usize>,
    pub targets: Vec<usize>,
    pub last_move: Option<(usize, usize)>,
    pub check: Option<usize>,
    pub shapes: Vec<Shape>,
    pub fen: String,
    pub variant: String,
    pub pockets: Option<ByColor<ByRole<u8>>>,
}

impl Scene {
    pub fn from_widget(w: &Widget) -> Self {
        let fen = w.fen();
        let variant_name = w.variant().to_owned();
        let variant = variant_for_name(&variant_name);
        let mode = if variant == Variant::Chess {
            if variant_name.eq_ignore_ascii_case("chess960") {
                CastlingMode::Chess960
            } else {
                castling_mode_for_fen(&fen)
            }
        } else {
            CastlingMode::Standard
        };
        let pos = parse_variant_fen(&fen, variant, mode).ok().map(|(p, _)| p);
        let board = pos.as_ref().map(board_array).unwrap_or([None; 64]);
        let check = pos
            .as_ref()
            .and_then(|p| {
                if p.checkers().any() {
                    p.board().king_of(p.turn())
                } else {
                    None
                }
            })
            .map(|sq| sq as usize);
        let selected = w.selection().and_then(name_to_index);
        let targets = w
            .current()
            .legal_moves()
            .into_iter()
            .filter(|m| {
                w.selection()
                    .map(|s| m.len() >= 4 && &m[0..2] == s)
                    .unwrap_or(false)
            })
            .filter_map(|m| name_to_index(&m[2..4]))
            .collect();
        let last_move = w
            .last_move_pair()
            .and_then(|(f, t)| Some((name_to_index(&f)?, name_to_index(&t)?)));
        Self {
            board,
            orientation: w.orientation(),
            theme: w.theme(),
            pieces: w.pieces(),
            selected,
            targets,
            last_move,
            check,
            shapes: w.shapes().to_vec(),
            fen,
            variant: variant_name,
            pockets: pos.as_ref().and_then(|p| p.pockets().copied()),
        }
    }

    /// Crazyhouse pocket count for a colour and piece kind.
    pub fn pocket(&self, color: Color, kind: Kind) -> u8 {
        let Some(pockets) = self.pockets else {
            return 0;
        };
        let color = match color {
            Color::White => ShakColor::White,
            Color::Black => ShakColor::Black,
        };
        let role = match kind {
            Kind::King => ShakRole::King,
            Kind::Queen => ShakRole::Queen,
            Kind::Rook => ShakRole::Rook,
            Kind::Bishop => ShakRole::Bishop,
            Kind::Knight => ShakRole::Knight,
            Kind::Pawn => ShakRole::Pawn,
        };
        *pockets.get(color).get(role)
    }

    /// True when the square is light-coloured for the given orientation.
    pub fn is_light(&self, display: usize) -> bool {
        (display / 8 + display % 8) % 2 == 0
    }

    /// Map a display cell (row-major from top-left) to a board index (a1=0).
    pub fn square_at(&self, display: usize) -> usize {
        let row = display / 8;
        let col = display % 8;
        match self.orientation {
            Orientation::White => (7 - row) * 8 + col,
            Orientation::Black => row * 8 + (7 - col),
        }
    }
}

pub fn name_to_index(name: &str) -> Option<usize> {
    let bytes = name.as_bytes();
    if bytes.len() != 2 {
        return None;
    }
    let file = bytes[0].checked_sub(b'a')? as usize;
    let rank = bytes[1].checked_sub(b'1')? as usize;
    if file >= 8 || rank >= 8 {
        return None;
    }
    Some(rank * 8 + file)
}

pub fn index_to_name(idx: usize) -> String {
    let file = (b'a' + (idx % 8) as u8) as char;
    let rank = (idx / 8) + 1;
    format!("{}{}", file, rank)
}

fn _position_from_fen(fen: &str) -> Option<VariantPosition> {
    knightlm_core::state::parse_chess(fen)
        .ok()
        .map(|(pos, _)| VariantPosition::from(pos))
}

fn board_array(pos: &VariantPosition) -> [Option<PlacedPiece>; 64] {
    let mut out = [None; 64];
    let board = pos.board();
    for sq in Square::ALL {
        if let Some(p) = board.piece_at(sq) {
            out[sq as usize] = Some(PlacedPiece {
                color: match p.color {
                    ShakColor::White => Color::White,
                    ShakColor::Black => Color::Black,
                },
                kind: match p.role {
                    ShakRole::King => Kind::King,
                    ShakRole::Queen => Kind::Queen,
                    ShakRole::Rook => Kind::Rook,
                    ShakRole::Bishop => Kind::Bishop,
                    ShakRole::Knight => Kind::Knight,
                    ShakRole::Pawn => Kind::Pawn,
                },
            });
        }
    }
    out
}
