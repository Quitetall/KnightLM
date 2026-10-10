//! Procedural SVG fixture images, not a trained recognizer.
use crate::state::GameState;

/// Two palettes and both orientations provide deterministic visual variants.
/// Targets stay outside the image: no FEN or game identity is embedded in SVG.
pub fn render_svg(state: &GameState, black_bottom: bool, blue_theme: bool) -> String {
    let fen = state.fen();
    let mut squares = Vec::new();
    for c in fen.split_whitespace().next().unwrap().chars() {
        match c {
            '/' => {}
            '1'..='8' => squares.extend(std::iter::repeat_n(' ', c.to_digit(10).unwrap() as usize)),
            _ => squares.push(c),
        }
    }
    let mut svg = String::from(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"512\" height=\"512\" viewBox=\"0 0 512 512\">\n",
    );
    let colors = if blue_theme {
        ["#e8eef7", "#6387ae"]
    } else {
        ["#f0d9b5", "#b58863"]
    };
    for display in 0..64 {
        let index = if black_bottom { 63 - display } else { display };
        let x = (display % 8) * 64;
        let y = (display / 8) * 64;
        svg.push_str(&format!(
            "<rect x=\"{x}\" y=\"{y}\" width=\"64\" height=\"64\" fill=\"{}\"/>\n",
            colors[(display / 8 + display % 8) % 2]
        ));
        let piece = squares[index];
        if piece != ' ' {
            // Letter-piece fixtures are portable without third-party font assets.
            let (fill, ink) = if piece.is_ascii_uppercase() {
                ("#fafafa", "#111111")
            } else {
                ("#222222", "#fafafa")
            };
            svg.push_str(&format!("<circle cx=\"{}\" cy=\"{}\" r=\"25\" fill=\"{fill}\" stroke=\"{ink}\"/><text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-family=\"monospace\" font-size=\"32\" font-weight=\"bold\" fill=\"{ink}\">{}</text>\n", x + 32, y + 32, x + 32, y + 43, piece.to_ascii_uppercase()));
        }
    }
    svg.push_str("</svg>\n");
    svg
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{GameRecord, START_FEN};
    #[test]
    fn render_is_reproducible_and_has_no_hidden_board_labels() {
        let state = GameState::replay(&GameRecord {
            schema_version: 1,
            game_id: "secret-identity".into(),
            source: "fixture".into(),
            initial_fen: START_FEN.into(),
            history_complete: true,
            moves: vec![],
        })
        .unwrap();
        let white = render_svg(&state, false, false);
        assert_eq!(white, render_svg(&state, false, false));
        assert_eq!(white.matches("<rect ").count(), 64);
        assert_eq!(white.matches("<text ").count(), 32);
        assert!(!white.contains(START_FEN) && !white.contains("secret-identity"));
        assert_ne!(white, render_svg(&state, true, false));
        assert_ne!(white, render_svg(&state, false, true));
    }
}
