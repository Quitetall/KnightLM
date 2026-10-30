//! Vendored chess piece sets (all licenses compatible with AGPL-3.0).
//!
//! | Set | License |
//! |---|---|
//! | Cburnett | GPL-2.0-or-later |
//! | Chessnut | Apache-2.0 |
//! | Merida | GPL-2.0-or-later |
//! | Fantasy | MIT |
//! | Spatial | MIT |
//! | Celtic | MIT |
//! | RhosGFX | CC0-1.0 |
//! | Letter | AGPL-3.0-or-later |
//! | Pirouetti | AGPL-3.0-or-later |
//! | Pixel | AGPL-3.0-or-later |
//!
//! SVG files live under `assets/pieces/<set>/` with a LICENSE note each.
//! The renderer scopes each piece's ids, gradient refs and <style> rules so
//! multiple pieces can be embedded in one SVG without collisions.

use crate::scene::{Color, Kind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PieceSet {
    Cburnett,
    Chessnut,
    Merida,
    Fantasy,
    Spatial,
    Celtic,
    Rhosgfx,
    Letter,
    Pirouetti,
    Pixel,
}

impl PieceSet {
    pub const ALL: [PieceSet; 10] = [
        PieceSet::Cburnett, PieceSet::Chessnut, PieceSet::Merida, PieceSet::Fantasy, PieceSet::Spatial, PieceSet::Celtic, PieceSet::Rhosgfx, PieceSet::Letter, PieceSet::Pirouetti, PieceSet::Pixel,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            PieceSet::Cburnett => "cburnett",
            PieceSet::Chessnut => "chessnut",
            PieceSet::Merida => "merida",
            PieceSet::Fantasy => "fantasy",
            PieceSet::Spatial => "spatial",
            PieceSet::Celtic => "celtic",
            PieceSet::Rhosgfx => "rhosgfx",
            PieceSet::Letter => "letter",
            PieceSet::Pirouetti => "pirouetti",
            PieceSet::Pixel => "pixel",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PieceSet::Cburnett => "Cburnett",
            PieceSet::Chessnut => "Chessnut",
            PieceSet::Merida => "Merida",
            PieceSet::Fantasy => "Fantasy",
            PieceSet::Spatial => "Spatial",
            PieceSet::Celtic => "Celtic",
            PieceSet::Rhosgfx => "RhosGFX",
            PieceSet::Letter => "Letter",
            PieceSet::Pirouetti => "Pirouetti",
            PieceSet::Pixel => "Pixel",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "cburnett" => Some(PieceSet::Cburnett),
            "chessnut" => Some(PieceSet::Chessnut),
            "merida" => Some(PieceSet::Merida),
            "fantasy" => Some(PieceSet::Fantasy),
            "spatial" => Some(PieceSet::Spatial),
            "celtic" => Some(PieceSet::Celtic),
            "rhosgfx" => Some(PieceSet::Rhosgfx),
            "letter" => Some(PieceSet::Letter),
            "pirouetti" => Some(PieceSet::Pirouetti),
            "pixel" => Some(PieceSet::Pixel),
            _ => None,
        }
    }
}

/// The SVG viewBox for a set (min-x, min-y, width, height).
pub fn view_box(set: PieceSet) -> &'static str {
    match set {
        PieceSet::Cburnett => "0 0 45 45",
        PieceSet::Chessnut => "0 0 800 800",
        PieceSet::Merida => "0 0 50 50",
        PieceSet::Fantasy => "0 0 933.333 933.333",
        PieceSet::Spatial => "0 0 933.333 933.333",
        PieceSet::Celtic => "0 0 933 933",
        PieceSet::Rhosgfx => "0 0 72 72",
        PieceSet::Letter => "0 0 300 300",
        PieceSet::Pirouetti => "0 0 3810 3810",
        PieceSet::Pixel => "0 -0.5 16 16",
    }
}

fn raw(set: PieceSet, kind: Kind, color: Color) -> &'static str {
    match (set, kind, color) {
        (PieceSet::Cburnett, Kind::King, Color::White) => include_str!("../assets/pieces/cburnett/wK.svg"),
        (PieceSet::Cburnett, Kind::King, Color::Black) => include_str!("../assets/pieces/cburnett/bK.svg"),
        (PieceSet::Cburnett, Kind::Queen, Color::White) => include_str!("../assets/pieces/cburnett/wQ.svg"),
        (PieceSet::Cburnett, Kind::Queen, Color::Black) => include_str!("../assets/pieces/cburnett/bQ.svg"),
        (PieceSet::Cburnett, Kind::Rook, Color::White) => include_str!("../assets/pieces/cburnett/wR.svg"),
        (PieceSet::Cburnett, Kind::Rook, Color::Black) => include_str!("../assets/pieces/cburnett/bR.svg"),
        (PieceSet::Cburnett, Kind::Bishop, Color::White) => include_str!("../assets/pieces/cburnett/wB.svg"),
        (PieceSet::Cburnett, Kind::Bishop, Color::Black) => include_str!("../assets/pieces/cburnett/bB.svg"),
        (PieceSet::Cburnett, Kind::Knight, Color::White) => include_str!("../assets/pieces/cburnett/wN.svg"),
        (PieceSet::Cburnett, Kind::Knight, Color::Black) => include_str!("../assets/pieces/cburnett/bN.svg"),
        (PieceSet::Cburnett, Kind::Pawn, Color::White) => include_str!("../assets/pieces/cburnett/wP.svg"),
        (PieceSet::Cburnett, Kind::Pawn, Color::Black) => include_str!("../assets/pieces/cburnett/bP.svg"),
        (PieceSet::Chessnut, Kind::King, Color::White) => include_str!("../assets/pieces/chessnut/wK.svg"),
        (PieceSet::Chessnut, Kind::King, Color::Black) => include_str!("../assets/pieces/chessnut/bK.svg"),
        (PieceSet::Chessnut, Kind::Queen, Color::White) => include_str!("../assets/pieces/chessnut/wQ.svg"),
        (PieceSet::Chessnut, Kind::Queen, Color::Black) => include_str!("../assets/pieces/chessnut/bQ.svg"),
        (PieceSet::Chessnut, Kind::Rook, Color::White) => include_str!("../assets/pieces/chessnut/wR.svg"),
        (PieceSet::Chessnut, Kind::Rook, Color::Black) => include_str!("../assets/pieces/chessnut/bR.svg"),
        (PieceSet::Chessnut, Kind::Bishop, Color::White) => include_str!("../assets/pieces/chessnut/wB.svg"),
        (PieceSet::Chessnut, Kind::Bishop, Color::Black) => include_str!("../assets/pieces/chessnut/bB.svg"),
        (PieceSet::Chessnut, Kind::Knight, Color::White) => include_str!("../assets/pieces/chessnut/wN.svg"),
        (PieceSet::Chessnut, Kind::Knight, Color::Black) => include_str!("../assets/pieces/chessnut/bN.svg"),
        (PieceSet::Chessnut, Kind::Pawn, Color::White) => include_str!("../assets/pieces/chessnut/wP.svg"),
        (PieceSet::Chessnut, Kind::Pawn, Color::Black) => include_str!("../assets/pieces/chessnut/bP.svg"),
        (PieceSet::Merida, Kind::King, Color::White) => include_str!("../assets/pieces/merida/wK.svg"),
        (PieceSet::Merida, Kind::King, Color::Black) => include_str!("../assets/pieces/merida/bK.svg"),
        (PieceSet::Merida, Kind::Queen, Color::White) => include_str!("../assets/pieces/merida/wQ.svg"),
        (PieceSet::Merida, Kind::Queen, Color::Black) => include_str!("../assets/pieces/merida/bQ.svg"),
        (PieceSet::Merida, Kind::Rook, Color::White) => include_str!("../assets/pieces/merida/wR.svg"),
        (PieceSet::Merida, Kind::Rook, Color::Black) => include_str!("../assets/pieces/merida/bR.svg"),
        (PieceSet::Merida, Kind::Bishop, Color::White) => include_str!("../assets/pieces/merida/wB.svg"),
        (PieceSet::Merida, Kind::Bishop, Color::Black) => include_str!("../assets/pieces/merida/bB.svg"),
        (PieceSet::Merida, Kind::Knight, Color::White) => include_str!("../assets/pieces/merida/wN.svg"),
        (PieceSet::Merida, Kind::Knight, Color::Black) => include_str!("../assets/pieces/merida/bN.svg"),
        (PieceSet::Merida, Kind::Pawn, Color::White) => include_str!("../assets/pieces/merida/wP.svg"),
        (PieceSet::Merida, Kind::Pawn, Color::Black) => include_str!("../assets/pieces/merida/bP.svg"),
        (PieceSet::Fantasy, Kind::King, Color::White) => include_str!("../assets/pieces/fantasy/wK.svg"),
        (PieceSet::Fantasy, Kind::King, Color::Black) => include_str!("../assets/pieces/fantasy/bK.svg"),
        (PieceSet::Fantasy, Kind::Queen, Color::White) => include_str!("../assets/pieces/fantasy/wQ.svg"),
        (PieceSet::Fantasy, Kind::Queen, Color::Black) => include_str!("../assets/pieces/fantasy/bQ.svg"),
        (PieceSet::Fantasy, Kind::Rook, Color::White) => include_str!("../assets/pieces/fantasy/wR.svg"),
        (PieceSet::Fantasy, Kind::Rook, Color::Black) => include_str!("../assets/pieces/fantasy/bR.svg"),
        (PieceSet::Fantasy, Kind::Bishop, Color::White) => include_str!("../assets/pieces/fantasy/wB.svg"),
        (PieceSet::Fantasy, Kind::Bishop, Color::Black) => include_str!("../assets/pieces/fantasy/bB.svg"),
        (PieceSet::Fantasy, Kind::Knight, Color::White) => include_str!("../assets/pieces/fantasy/wN.svg"),
        (PieceSet::Fantasy, Kind::Knight, Color::Black) => include_str!("../assets/pieces/fantasy/bN.svg"),
        (PieceSet::Fantasy, Kind::Pawn, Color::White) => include_str!("../assets/pieces/fantasy/wP.svg"),
        (PieceSet::Fantasy, Kind::Pawn, Color::Black) => include_str!("../assets/pieces/fantasy/bP.svg"),
        (PieceSet::Spatial, Kind::King, Color::White) => include_str!("../assets/pieces/spatial/wK.svg"),
        (PieceSet::Spatial, Kind::King, Color::Black) => include_str!("../assets/pieces/spatial/bK.svg"),
        (PieceSet::Spatial, Kind::Queen, Color::White) => include_str!("../assets/pieces/spatial/wQ.svg"),
        (PieceSet::Spatial, Kind::Queen, Color::Black) => include_str!("../assets/pieces/spatial/bQ.svg"),
        (PieceSet::Spatial, Kind::Rook, Color::White) => include_str!("../assets/pieces/spatial/wR.svg"),
        (PieceSet::Spatial, Kind::Rook, Color::Black) => include_str!("../assets/pieces/spatial/bR.svg"),
        (PieceSet::Spatial, Kind::Bishop, Color::White) => include_str!("../assets/pieces/spatial/wB.svg"),
        (PieceSet::Spatial, Kind::Bishop, Color::Black) => include_str!("../assets/pieces/spatial/bB.svg"),
        (PieceSet::Spatial, Kind::Knight, Color::White) => include_str!("../assets/pieces/spatial/wN.svg"),
        (PieceSet::Spatial, Kind::Knight, Color::Black) => include_str!("../assets/pieces/spatial/bN.svg"),
        (PieceSet::Spatial, Kind::Pawn, Color::White) => include_str!("../assets/pieces/spatial/wP.svg"),
        (PieceSet::Spatial, Kind::Pawn, Color::Black) => include_str!("../assets/pieces/spatial/bP.svg"),
        (PieceSet::Celtic, Kind::King, Color::White) => include_str!("../assets/pieces/celtic/wK.svg"),
        (PieceSet::Celtic, Kind::King, Color::Black) => include_str!("../assets/pieces/celtic/bK.svg"),
        (PieceSet::Celtic, Kind::Queen, Color::White) => include_str!("../assets/pieces/celtic/wQ.svg"),
        (PieceSet::Celtic, Kind::Queen, Color::Black) => include_str!("../assets/pieces/celtic/bQ.svg"),
        (PieceSet::Celtic, Kind::Rook, Color::White) => include_str!("../assets/pieces/celtic/wR.svg"),
        (PieceSet::Celtic, Kind::Rook, Color::Black) => include_str!("../assets/pieces/celtic/bR.svg"),
        (PieceSet::Celtic, Kind::Bishop, Color::White) => include_str!("../assets/pieces/celtic/wB.svg"),
        (PieceSet::Celtic, Kind::Bishop, Color::Black) => include_str!("../assets/pieces/celtic/bB.svg"),
        (PieceSet::Celtic, Kind::Knight, Color::White) => include_str!("../assets/pieces/celtic/wN.svg"),
        (PieceSet::Celtic, Kind::Knight, Color::Black) => include_str!("../assets/pieces/celtic/bN.svg"),
        (PieceSet::Celtic, Kind::Pawn, Color::White) => include_str!("../assets/pieces/celtic/wP.svg"),
        (PieceSet::Celtic, Kind::Pawn, Color::Black) => include_str!("../assets/pieces/celtic/bP.svg"),
        (PieceSet::Rhosgfx, Kind::King, Color::White) => include_str!("../assets/pieces/rhosgfx/wK.svg"),
        (PieceSet::Rhosgfx, Kind::King, Color::Black) => include_str!("../assets/pieces/rhosgfx/bK.svg"),
        (PieceSet::Rhosgfx, Kind::Queen, Color::White) => include_str!("../assets/pieces/rhosgfx/wQ.svg"),
        (PieceSet::Rhosgfx, Kind::Queen, Color::Black) => include_str!("../assets/pieces/rhosgfx/bQ.svg"),
        (PieceSet::Rhosgfx, Kind::Rook, Color::White) => include_str!("../assets/pieces/rhosgfx/wR.svg"),
        (PieceSet::Rhosgfx, Kind::Rook, Color::Black) => include_str!("../assets/pieces/rhosgfx/bR.svg"),
        (PieceSet::Rhosgfx, Kind::Bishop, Color::White) => include_str!("../assets/pieces/rhosgfx/wB.svg"),
        (PieceSet::Rhosgfx, Kind::Bishop, Color::Black) => include_str!("../assets/pieces/rhosgfx/bB.svg"),
        (PieceSet::Rhosgfx, Kind::Knight, Color::White) => include_str!("../assets/pieces/rhosgfx/wN.svg"),
        (PieceSet::Rhosgfx, Kind::Knight, Color::Black) => include_str!("../assets/pieces/rhosgfx/bN.svg"),
        (PieceSet::Rhosgfx, Kind::Pawn, Color::White) => include_str!("../assets/pieces/rhosgfx/wP.svg"),
        (PieceSet::Rhosgfx, Kind::Pawn, Color::Black) => include_str!("../assets/pieces/rhosgfx/bP.svg"),
        (PieceSet::Letter, Kind::King, Color::White) => include_str!("../assets/pieces/letter/wK.svg"),
        (PieceSet::Letter, Kind::King, Color::Black) => include_str!("../assets/pieces/letter/bK.svg"),
        (PieceSet::Letter, Kind::Queen, Color::White) => include_str!("../assets/pieces/letter/wQ.svg"),
        (PieceSet::Letter, Kind::Queen, Color::Black) => include_str!("../assets/pieces/letter/bQ.svg"),
        (PieceSet::Letter, Kind::Rook, Color::White) => include_str!("../assets/pieces/letter/wR.svg"),
        (PieceSet::Letter, Kind::Rook, Color::Black) => include_str!("../assets/pieces/letter/bR.svg"),
        (PieceSet::Letter, Kind::Bishop, Color::White) => include_str!("../assets/pieces/letter/wB.svg"),
        (PieceSet::Letter, Kind::Bishop, Color::Black) => include_str!("../assets/pieces/letter/bB.svg"),
        (PieceSet::Letter, Kind::Knight, Color::White) => include_str!("../assets/pieces/letter/wN.svg"),
        (PieceSet::Letter, Kind::Knight, Color::Black) => include_str!("../assets/pieces/letter/bN.svg"),
        (PieceSet::Letter, Kind::Pawn, Color::White) => include_str!("../assets/pieces/letter/wP.svg"),
        (PieceSet::Letter, Kind::Pawn, Color::Black) => include_str!("../assets/pieces/letter/bP.svg"),
        (PieceSet::Pirouetti, Kind::King, Color::White) => include_str!("../assets/pieces/pirouetti/wK.svg"),
        (PieceSet::Pirouetti, Kind::King, Color::Black) => include_str!("../assets/pieces/pirouetti/bK.svg"),
        (PieceSet::Pirouetti, Kind::Queen, Color::White) => include_str!("../assets/pieces/pirouetti/wQ.svg"),
        (PieceSet::Pirouetti, Kind::Queen, Color::Black) => include_str!("../assets/pieces/pirouetti/bQ.svg"),
        (PieceSet::Pirouetti, Kind::Rook, Color::White) => include_str!("../assets/pieces/pirouetti/wR.svg"),
        (PieceSet::Pirouetti, Kind::Rook, Color::Black) => include_str!("../assets/pieces/pirouetti/bR.svg"),
        (PieceSet::Pirouetti, Kind::Bishop, Color::White) => include_str!("../assets/pieces/pirouetti/wB.svg"),
        (PieceSet::Pirouetti, Kind::Bishop, Color::Black) => include_str!("../assets/pieces/pirouetti/bB.svg"),
        (PieceSet::Pirouetti, Kind::Knight, Color::White) => include_str!("../assets/pieces/pirouetti/wN.svg"),
        (PieceSet::Pirouetti, Kind::Knight, Color::Black) => include_str!("../assets/pieces/pirouetti/bN.svg"),
        (PieceSet::Pirouetti, Kind::Pawn, Color::White) => include_str!("../assets/pieces/pirouetti/wP.svg"),
        (PieceSet::Pirouetti, Kind::Pawn, Color::Black) => include_str!("../assets/pieces/pirouetti/bP.svg"),
        (PieceSet::Pixel, Kind::King, Color::White) => include_str!("../assets/pieces/pixel/wK.svg"),
        (PieceSet::Pixel, Kind::King, Color::Black) => include_str!("../assets/pieces/pixel/bK.svg"),
        (PieceSet::Pixel, Kind::Queen, Color::White) => include_str!("../assets/pieces/pixel/wQ.svg"),
        (PieceSet::Pixel, Kind::Queen, Color::Black) => include_str!("../assets/pieces/pixel/bQ.svg"),
        (PieceSet::Pixel, Kind::Rook, Color::White) => include_str!("../assets/pieces/pixel/wR.svg"),
        (PieceSet::Pixel, Kind::Rook, Color::Black) => include_str!("../assets/pieces/pixel/bR.svg"),
        (PieceSet::Pixel, Kind::Bishop, Color::White) => include_str!("../assets/pieces/pixel/wB.svg"),
        (PieceSet::Pixel, Kind::Bishop, Color::Black) => include_str!("../assets/pieces/pixel/bB.svg"),
        (PieceSet::Pixel, Kind::Knight, Color::White) => include_str!("../assets/pieces/pixel/wN.svg"),
        (PieceSet::Pixel, Kind::Knight, Color::Black) => include_str!("../assets/pieces/pixel/bN.svg"),
        (PieceSet::Pixel, Kind::Pawn, Color::White) => include_str!("../assets/pieces/pixel/wP.svg"),
        (PieceSet::Pixel, Kind::Pawn, Color::Black) => include_str!("../assets/pieces/pixel/bP.svg"),
    }
}


fn inner(full: &str) -> &str {
    let start = full.find('>').map(|i| i + 1).unwrap_or(0);
    let end = full.rfind("</svg>").unwrap_or(full.len());
    if start <= end { &full[start..end] } else { full }
}

fn prefix_for(set: PieceSet, kind: Kind, color: Color) -> String {
    let k = match kind {
        Kind::King => "k", Kind::Queen => "q", Kind::Rook => "r",
        Kind::Bishop => "b", Kind::Knight => "n", Kind::Pawn => "p",
    };
    let c = match color { Color::White => "w", Color::Black => "b" };
    format!("pc-{}{}{}-", set.slug(), k, c)
}

fn scope(inner_svg: &str, class: &str) -> String {
    let mut s = inner_svg.replace("id=\"", &format!("id=\"{class}"));
    s = s.replace("url(#", &format!("url(#{class}"));
    s = s.replace("href=\"#", &format!("href=\"#{class}"));
    s = scope_styles(&s, class);
    format!("<g class=\"{class}\">{s}</g>")
}

fn scope_styles(s: &str, class: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find("<style") {
        let after = &rest[start..];
        if let Some(close) = after.find("</style>") {
            let end = start + close + "</style>".len();
            out.push_str(&rest[..start]);
            out.push_str(&scope_css(&rest[start..end], class));
            rest = &rest[end..];
        } else {
            break;
        }
    }
    out.push_str(rest);
    out
}

fn scope_css(style: &str, class: &str) -> String {
    let open_end = style.find('>').map(|i| i + 1).unwrap_or(0);
    let content_end = style.rfind("</style>").unwrap_or(style.len());
    let open = &style[..open_end];
    let content = &style[open_end..content_end];
    let mut rules = String::new();
    for rule in content.split('}') {
        let rule = rule.trim();
        if rule.is_empty() { continue; }
        if let Some((selectors, decls)) = rule.split_once('{') {
            let scoped = selectors
                .split(',')
                .map(|sel| {
                    let sel = sel.trim();
                    if sel.is_empty() || sel.starts_with('@') {
                        sel.to_string()
                    } else {
                        format!(".{class} {sel}")
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            rules.push_str(&format!("{scoped} {{{decls}}}"));
        }
    }
    format!("{open}{rules}</style>")
}

/// The inner SVG markup for one piece, scoped to be safe to embed repeatedly.
pub fn piece_markup(set: PieceSet, kind: Kind, color: Color) -> String {
    let class = prefix_for(set, kind, color);
    scope(inner(raw(set, kind, color)), &class)
}
