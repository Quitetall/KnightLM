use brick_chess::{Command, Event, Widget};
use brick_chess::protocol;

fn play(w: &mut Widget, san: &str) -> Vec<Event> {
    w.apply(Command::Play { san: Some(san.into()), uci: None })
}

#[test]
fn plays_san_and_updates_fen() {
    let mut w = Widget::new();
    let ev = play(&mut w, "e4");
    assert!(matches!(&ev[0], Event::Move { uci, san, .. } if uci == "e2e4" && san == "e4"));
    assert!(w.fen().starts_with("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b"));
    play(&mut w, "e5");
    assert!(w.fen().contains("4p3"));
}

#[test]
fn illegal_move_is_an_error_event() {
    let mut w = Widget::new();
    let ev = w.apply(Command::Play { san: None, uci: Some("e2e5".into()) });
    assert!(matches!(&ev[0], Event::Error { code, .. } if code == "illegal_move"));
}

#[test]
fn select_reports_legal_targets() {
    let mut w = Widget::new();
    let ev = w.apply(Command::Select { square: "e2".into() });
    match &ev[0] {
        Event::Selected { square, targets } => {
            assert_eq!(square, "e2");
            assert!(targets.contains(&"e3".to_string()));
            assert!(targets.contains(&"e4".to_string()));
        }
        other => panic!("expected Selected, got {other:?}"),
    }
}

#[test]
fn history_goto_and_undo() {
    let mut w = Widget::new();
    play(&mut w, "e4");
    play(&mut w, "e5");
    let fen_after = w.fen();
    w.apply(Command::Goto { index: 0 });
    assert!(w.fen().starts_with("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w"));
    w.apply(Command::Goto { index: 2 });
    assert_eq!(w.fen(), fen_after);
    w.apply(Command::Undo);
    assert!(w.fen().contains("4P3/8/PPPP1PPP") || w.fen().contains("4P3"));
}

#[test]
fn checkmate_reports_game_over() {
    // Fool's mate: 1. f3 e5 2. g4 Qh4#
    let mut w = Widget::new();
    for san in ["f3", "e5", "g4"] {
        play(&mut w, san);
    }
    let ev = play(&mut w, "Qh4#");
    assert!(
        ev.iter().any(|e| matches!(e, Event::GameOver { result, reason } if result == "0-1" && reason == "checkmate")),
        "expected checkmate game over, got {ev:?}"
    );
    // Any further move is refused explicitly, never silently played.
    let ev = play(&mut w, "e4");
    assert!(matches!(&ev[0], Event::Error { .. }), "expected refusal, got {ev:?}");
}

#[test]
fn pgn_load_and_export() {
    let mut w = Widget::new();
    let pgn = "1. e4 e5 2. Nf3 Nc6 1-0";
    let ev = w.apply(Command::LoadPgn { pgn: pgn.into() });
    assert!(matches!(&ev[0], Event::Position { .. }));
    let exported = match w.apply(Command::Export { format: "pgn".into() }).pop() {
        Some(Event::Exported { data, .. }) => data,
        other => panic!("expected export, got {other:?}"),
    };
    assert!(exported.contains("e4"));
    assert!(exported.contains("Nf3"));
}

#[test]
fn pgn_load_keeps_full_history() {
    let mut w = Widget::new();
    w.apply(Command::LoadPgn { pgn: "1. e4 e5 2. Nf3 Nc6".into() });
    // initial position + four plies
    assert_eq!(w.history_len(), 5);
    assert_eq!(w.history_index(), 4);
    // navigation walks back through the loaded game
    w.apply(Command::Goto { index: 0 });
    assert!(w.fen().starts_with("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP"));
    w.apply(Command::Goto { index: 2 });
    assert!(w.fen().contains("4P3"));
}

#[test]
fn json_protocol_round_trip() {
    let mut w = Widget::new();
    let out = protocol::apply_line(&mut w, r#"{"cmd":"play","san":"e4"}"#);
    assert!(out[0].contains("\"event\":\"move\""));
    let out = protocol::apply_line(&mut w, r#"{"cmd":"select","square":"e5"}"#);
    assert!(out[0].contains("\"event\":\"selected\""));
    let out = protocol::apply_line(&mut w, "not json");
    assert!(out[0].contains("bad_json"));
}

#[test]
fn svg_and_ascii_render() {
    let mut w = Widget::new();
    play(&mut w, "e4");
    let svg = w.export("svg").unwrap();
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("<path")); // pieces are paths, not text glyphs
    let ascii = w.export("ascii").unwrap();
    assert!(ascii.contains("FEN:"));
    assert!(ascii.contains('P'));
}
