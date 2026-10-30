//! Plain-text board rendering (CLI/agent-readable).

use crate::scene::{Kind, Scene};
use crate::widget::Orientation;

fn glyph(kind: Kind, white: bool) -> char {
    match (kind, white) {
        (Kind::King, true) => 'K',
        (Kind::Queen, true) => 'Q',
        (Kind::Rook, true) => 'R',
        (Kind::Bishop, true) => 'B',
        (Kind::Knight, true) => 'N',
        (Kind::Pawn, true) => 'P',
        (Kind::King, false) => 'k',
        (Kind::Queen, false) => 'q',
        (Kind::Rook, false) => 'r',
        (Kind::Bishop, false) => 'b',
        (Kind::Knight, false) => 'n',
        (Kind::Pawn, false) => 'p',
    }
}

pub fn render(scene: &Scene) -> String {
    let mut out = String::new();
    for display in 0..64 {
        if display % 8 == 0 {
            let view_rank = 8 - (display / 8);
            out.push_str(&format!("{} ", view_rank));
        }
        let idx = scene.square_at(display);
        let ch = match scene.board[idx] {
            Some(p) => glyph(p.kind, p.color == crate::scene::Color::White),
            None => '.',
        };
        out.push(ch);
        out.push(' ');
        if display % 8 == 7 {
            out.push('\n');
        }
    }
    match scene.orientation {
        Orientation::White => out.push_str("  a b c d e f g h\n"),
        Orientation::Black => out.push_str("  h g f e d c b a\n"),
    }
    out.push_str(&format!("FEN: {}\n", scene.fen));
    out
}
