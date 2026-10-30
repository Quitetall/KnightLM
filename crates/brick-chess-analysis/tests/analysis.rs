use brick_chess_analysis::*;

#[test]
fn win_percent_anchor_points() {
    assert!((win_percent_cp(0.0) - 50.0).abs() < 1e-9);
    assert!(win_percent_cp(100.0) > 55.0);
    assert!(win_percent_cp(-100.0) < 45.0);
    assert_eq!(Eval::Mate(3).win_percent(), 100.0);
    assert_eq!(Eval::Mate(-3).win_percent(), 0.0);
}

#[test]
fn accuracy_anchor() {
    assert!((accuracy_percent(0.0) - 100.0).abs() < 0.01);
    assert!(accuracy_percent(30.0) < 30.0);
}

#[test]
fn lichess_thresholds() {
    assert_eq!(classify_lichess(0.0, true), MoveLabel::Best);
    assert_eq!(classify_lichess(9.9, false), MoveLabel::Good);
    assert_eq!(classify_lichess(10.0, false), MoveLabel::Inaccuracy);
    assert_eq!(classify_lichess(20.0, false), MoveLabel::Mistake);
    assert_eq!(classify_lichess(30.0, false), MoveLabel::Blunder);
}

#[test]
fn ep_thresholds() {
    assert_eq!(classify_expected_points(0.01, false), MoveLabel::Excellent);
    assert_eq!(classify_expected_points(0.03, false), MoveLabel::Good);
    assert_eq!(classify_expected_points(0.07, false), MoveLabel::Inaccuracy);
    assert_eq!(classify_expected_points(0.15, false), MoveLabel::Mistake);
    assert_eq!(classify_expected_points(0.25, false), MoveLabel::Blunder);
}

#[test]
fn ep_monotonic() {
    for cp in [-800, -400, -100, 0, 100, 400, 800] {
        let ep = expected_points_cp(cp as f64, 400.0);
        assert!((0.0..=1.0).contains(&ep));
    }
    assert!(expected_points_cp(0.0, 400.0) - 0.5 < 1e-9);
    assert!(expected_points_cp(400.0, 400.0) > expected_points_cp(100.0, 400.0));
}

#[test]
fn analyze_small_game() {
    let moves = vec![
        MoveInput {
            san: "e4".into(),
            uci: "e2e4".into(),
            eval_before: Eval::Cp(0),
            eval_after: Eval::Cp(20),
            best_move: Some("e2e4".into()),
        },
        MoveInput {
            san: "e5".into(),
            uci: "e7e5".into(),
            eval_before: Eval::Cp(0),
            eval_after: Eval::Cp(-300),
            best_move: Some("g8f6".into()),
        },
    ];
    let r = analyze(&moves, &AnalysisConfig::default());
    assert_eq!(r.len(), 2);
    assert_eq!(r.moves[0].lichess, MoveLabel::Best);
    assert_eq!(r.moves[1].lichess, MoveLabel::Mistake); // ~25% win drop
    assert_eq!(r.moves[1].expected_points, MoveLabel::Blunder);
    assert!(r.white_accuracy.unwrap() > 99.0);
    assert!(r.black_accuracy.unwrap() < 50.0);
    assert_eq!(r.lichess_counts.best, 1);
    assert_eq!(r.lichess_counts.mistake, 1);
    assert_eq!(r.count(MoveLabel::Best), (1, 1));
}
