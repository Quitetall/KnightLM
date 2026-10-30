//! SVG rendering of a [`Scene`] with original Staunton-style piece paths.
//!
//! No third-party artwork: each piece is hand-authored path data on a 45x45
//! grid (the traditional chess-SVG unit), scaled per square. Renderer-agnostic
//! scenes keep this interchangeable with a Leptos/egui front end.

use crate::command::Shape;
use crate::scene::{index_to_name, Color, Kind, Scene};
use crate::widget::{Orientation, Theme};

const SIZE: f64 = 512.0;
const SQUARE: f64 = SIZE / 8.0;

fn palette(theme: Theme) -> [&'static str; 2] {
    match theme {
        Theme::Brown => ["#f0d9b5", "#b58863"],
        Theme::Blue => ["#e8eef7", "#6387ae"],
        Theme::Green => ["#eeeed2", "#769656"],
    }
}

fn piece_svg(set: crate::pieces::PieceSet, kind: Kind, color: Color, x: f64, y: f64, size: f64) -> String {
    // Scale by the set's own viewBox, honouring a non-zero min-x/min-y (pixel set).
    let vb = crate::pieces::view_box(set);
    let nums: Vec<f64> = vb.split_whitespace().filter_map(|t| t.parse().ok()).collect();
    let (min_x, min_y, w) = if nums.len() == 4 { (nums[0], nums[1], nums[2]) } else { (0.0, 0.0, 800.0) };
    let scale = size / w;
    let tx = x - min_x * scale;
    let ty = y - min_y * scale;
    let markup = crate::pieces::piece_markup(set, kind, color);
    format!(
        "<g transform=\"translate({tx:.3},{ty:.3}) scale({scale:.6})\">{markup}</g>"
    )
}

fn shape_color(name: &str) -> &'static str {
    match name.to_ascii_lowercase().as_str() {
        "blue" => "#4a7ad4",
        "red" => "#d44a4a",
        "yellow" => "#d4b84a",
        "green" => "#4a9e4a",
        _ => "#4a9e4a",
    }
}

fn center(idx: usize, scene: &Scene) -> (f64, f64) {
    // Find the display cell for this board index under the scene's orientation.
    let display = (0..64)
        .find(|&d| scene.square_at(d) == idx)
        .unwrap_or(idx);
    let x = (display % 8) as f64 * SQUARE + SQUARE / 2.0;
    let y = (display / 8) as f64 * SQUARE + SQUARE / 2.0;
    (x, y)
}

pub fn render(scene: &Scene) -> String {
    let colors = palette(scene.theme);
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{SIZE}\" height=\"{SIZE}\" viewBox=\"0 0 {SIZE} {SIZE}\">\n"
    );
    svg.push_str("<defs><marker id=\"arrowhead\" markerWidth=\"4\" markerHeight=\"4\" refX=\"2\" refY=\"2\" orient=\"auto\"><path d=\"M0,0 L4,2 L0,4 Z\" fill=\"#4a7ad4\"/></marker></defs>\n");

    // Squares.
    for display in 0..64 {
        let x = (display % 8) as f64 * SQUARE;
        let y = (display / 8) as f64 * SQUARE;
        let fill = if scene.is_light(display) { colors[0] } else { colors[1] };
        svg.push_str(&format!(
            "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{SQUARE}\" height=\"{SQUARE}\" fill=\"{fill}\"/>\n"
        ));
    }

    // Last-move highlight.
    if let Some((from, to)) = scene.last_move {
        for idx in [from, to] {
            let (cx, cy) = center(idx, scene);
            svg.push_str(&format!(
                "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{SQUARE}\" height=\"{SQUARE}\" fill=\"#f6d32d\" opacity=\"0.35\"/>\n",
                cx - SQUARE / 2.0,
                cy - SQUARE / 2.0
            ));
        }
    }

    // Selected square.
    if let Some(idx) = scene.selected {
        let (cx, cy) = center(idx, scene);
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{SQUARE}\" height=\"{SQUARE}\" fill=\"#f6d32d\" opacity=\"0.45\"/>\n",
            cx - SQUARE / 2.0,
            cy - SQUARE / 2.0
        ));
    }

    // Legal-move targets.
    for &idx in &scene.targets {
        let (cx, cy) = center(idx, scene);
        svg.push_str(&format!(
            "<circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"{:.1}\" fill=\"#222\" opacity=\"0.28\"/>\n",
            SQUARE * 0.16
        ));
    }

    // Check highlight.
    if let Some(idx) = scene.check {
        let (cx, cy) = center(idx, scene);
        svg.push_str(&format!(
            "<circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"{:.1}\" fill=\"#e04a4a\" opacity=\"0.45\"/>\n",
            SQUARE * 0.42
        ));
    }

    // Pieces.
    for display in 0..64 {
        let idx = scene.square_at(display);
        if let Some(p) = scene.board[idx] {
            let x = (display % 8) as f64 * SQUARE;
            let y = (display / 8) as f64 * SQUARE;
            svg.push_str(&piece_svg(scene.pieces, p.kind, p.color, x, y, SQUARE));
            svg.push('\n');
        }
    }

    // Annotations.
    for shape in &scene.shapes {
        match shape {
            Shape::Arrow { from, to, color } => {
                let a = crate::scene::name_to_index(from);
                let b = crate::scene::name_to_index(to);
                if let (Some(a), Some(b)) = (a, b) {
                    let (x1, y1) = center(a, scene);
                    let (x2, y2) = center(b, scene);
                    let c = shape_color(color);
                    svg.push_str(&format!(
                        "<line x1=\"{x1:.1}\" y1=\"{y1:.1}\" x2=\"{x2:.1}\" y2=\"{y2:.1}\" stroke=\"{c}\" stroke-width=\"7\" stroke-linecap=\"round\" opacity=\"0.8\"/>\n"
                    ));
                }
            }
            Shape::Circle { square, color } => {
                if let Some(i) = crate::scene::name_to_index(square) {
                    let (cx, cy) = center(i, scene);
                    let c = shape_color(color);
                    svg.push_str(&format!(
                        "<circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"{:.1}\" fill=\"none\" stroke=\"{c}\" stroke-width=\"5\" opacity=\"0.8\"/>\n",
                        SQUARE * 0.40
                    ));
                }
            }
            Shape::Highlight { squares, color } => {
                let c = shape_color(color);
                for s in squares {
                    if let Some(i) = crate::scene::name_to_index(s) {
                        let (cx, cy) = center(i, scene);
                        svg.push_str(&format!(
                            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{SQUARE}\" height=\"{SQUARE}\" fill=\"{c}\" opacity=\"0.35\"/>\n",
                            cx - SQUARE / 2.0,
                            cy - SQUARE / 2.0
                        ));
                    }
                }
            }
        }
    }

    // Coordinates.
    for i in 0..8 {
        let file = (b'a' + i as u8) as char;
        let x = i as f64 * SQUARE + SQUARE - 12.0;
        let y = SIZE - 6.0;
        svg.push_str(&format!(
            "<text x=\"{x:.1}\" y=\"{y:.1}\" font-family=\"sans-serif\" font-size=\"12\" fill=\"#333\" opacity=\"0.65\">{file}</text>\n"
        ));
        let rank = 8 - i;
        let x = 4.0;
        let y = i as f64 * SQUARE + 14.0;
        svg.push_str(&format!(
            "<text x=\"{x:.1}\" y=\"{y:.1}\" font-family=\"sans-serif\" font-size=\"12\" fill=\"#333\" opacity=\"0.65\">{rank}</text>\n"
        ));
    }

    let _ = Orientation::White;
    let _ = index_to_name;
    svg.push_str("</svg>\n");
    svg
}
