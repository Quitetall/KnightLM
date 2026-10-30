use knightlm_engine::Engine;

#[test]
fn stockfish_handshake_and_analyse() {
    let path = std::env::var("STOCKFISH_PATH").unwrap_or_else(|_| "stockfish".to_owned());
    let mut e = Engine::spawn(&path, 60).expect("spawn");
    assert!(e.name().to_lowercase().contains("stockfish"));
    let a = e.analyse("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", 8).unwrap();
    println!("bestmove={} depth={} cp={:?} mate={:?} pv={:?}", a.best_move, a.depth, a.score_cp, a.mate, a.pv);
    assert!(!a.best_move.is_empty());
    assert!(a.depth >= 8);
    let label = e.label("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", 8).unwrap();
    assert_eq!(label.best_move, a.best_move);
}
