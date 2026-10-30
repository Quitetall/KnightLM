// quick demo: render a real position to SVG using core's glyph renderer
use knightlm_core::vision::render_glyph_svg;
use knightlm_core::state::{GameState, GameRecord};

fn main() {
    // A recognizable middlegame: after 1.e4 e5 2.Nf3 Nc6 3.Bb5 (Ruy Lopez)
    let rec = GameRecord {
        schema_version: 1,
        game_id: "demo".into(),
        source: "demo".into(),
        initial_fen: knightlm_core::state::START_FEN.into(),
        history_complete: true,
        moves: vec!["e2e4".into(), "e7e5".into(), "g1f3".into(), "b8c6".into(), "f1b5".into()],
        variant: "standard".into(),
    };
    let state = GameState::replay(&rec).unwrap();
    let svg = render_glyph_svg(&state, true, false);
    print!("{}", svg);
}
