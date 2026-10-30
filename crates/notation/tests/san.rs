use knightlm_notation::san::{parse_san, to_san};
use shakmaty::{fen::Fen, uci::UciMove, CastlingMode, Chess, Position};

fn pos_from_fen(fen: &str) -> Chess {
    Fen::from_ascii(fen.as_bytes()).unwrap().into_position(CastlingMode::Standard).unwrap()
}

#[test]
fn every_legal_move_round_trips_from_startpos() {
    let pos = Chess::default();
    let moves: Vec<_> = pos.legal_moves().into_iter().collect();
    assert!(!moves.is_empty());
    for mv in moves {
        let uci = UciMove::from_move(mv, CastlingMode::Standard).to_string();
        let san = to_san(&pos, &uci).expect("to_san");
        let parsed = parse_san(&pos, &san).expect("parse_san");
        assert_eq!(parsed, uci, "round-trip failed for {uci} -> {san} -> {parsed}");
    }
}

#[test]
fn kingside_castling_san() {
    let moves = ["e2e4", "e7e5", "g1f3", "b8c6", "f1c4", "g8f6", "e1g1"];
    let mut pos = Chess::default();
    let mut sans = Vec::new();
    for uci in &moves {
        let mv = UciMove::from_ascii(uci.as_bytes()).unwrap().to_move(&pos).unwrap();
        sans.push(to_san(&pos, uci).unwrap());
        pos = pos.play(mv).unwrap();
    }
    assert_eq!(sans.last().unwrap(), "O-O", "got {:?}", sans.last());
    // Replay and compare final FEN.
    let mut p2 = Chess::default();
    for san in &sans {
        let uci = parse_san(&p2, san).expect("parse");
        let mv = UciMove::from_ascii(uci.as_bytes()).unwrap().to_move(&p2).unwrap();
        p2 = p2.play(mv).unwrap();
    }
    assert_eq!(p2.board(), pos.board());
}

#[test]
fn promotion_san() {
    let pos = pos_from_fen("8/P7/8/8/8/8/8/k6K w - - 0 1");
    let san = to_san(&pos, "a7a8q").expect("to_san");
    assert_eq!(san, "a8=Q+");
    let uci = parse_san(&pos, "a8=Q").expect("parse");
    assert_eq!(uci, "a7a8q");
}

#[test]
fn refusal_illegal_uci() {
    let pos = Chess::default();
    assert!(to_san(&pos, "e2e5").is_err());
}

#[test]
fn refusal_bad_san() {
    let pos = Chess::default();
    assert!(parse_san(&pos, "Qh5").is_err());
    assert!(parse_san(&pos, "").is_err());
    assert!(parse_san(&pos, "Z9").is_err());
}

#[test]
fn round_trip_short_game() {
    let ucis = ["e2e4", "e7e5", "g1f3", "b8c6", "f1c4", "g8f6", "e1g1", "f6e4", "d2d4", "d7d5"];
    let mut pos = Chess::default();
    let mut sans = Vec::new();
    for uci in &ucis {
        let san = to_san(&pos, uci).expect(uci);
        sans.push(san);
        let mv = UciMove::from_ascii(uci.as_bytes()).unwrap().to_move(&pos).unwrap();
        pos = pos.play(mv).unwrap();
    }
    let mut p2 = Chess::default();
    for san in &sans {
        let uci = parse_san(&p2, san).expect(san);
        let mv = UciMove::from_ascii(uci.as_bytes()).unwrap().to_move(&p2).unwrap();
        p2 = p2.play(mv).unwrap();
    }
    assert_eq!(p2.board(), pos.board());
}

#[test]
fn pawn_capture_never_doubles_file() {
    let pos = pos_from_fen("4k3/8/8/2P1P3/8/8/8/4K3 w - - 0 1");
    for uci in ["c5d6", "e5d6"] {
        if uci.len() != 4 {
            continue;
        }
        let legal = UciMove::from_ascii(uci.as_bytes())
            .ok()
            .and_then(|u| u.to_move(&pos).ok());
        let Some(mv) = legal else { continue };
        let san = to_san(&pos, uci).unwrap();
        let _ = mv;
        // exactly one capture marker and one file letter
        assert_eq!(san.matches('x').count(), 1, "{uci} -> {san}");
        let from_file = uci.chars().next().unwrap();
        assert_eq!(san.matches(from_file).count(), 1, "{uci} -> {san}");
    }
}
