use knightlm_notation::pgn;

#[test]
fn tags_and_moves_round_trip() {
    let input = r#"[Event "Test"]
[Site "Here"]
[Result "1-0"]

1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 1-0
"#;
    let g = pgn::parse(input).unwrap();
    assert_eq!(g.tags.get("Event").unwrap(), "Test");
    assert_eq!(g.result.as_deref(), Some("1-0"));
    assert_eq!(g.movetext.len(), 6);
    let out = pgn::write(&g);
    let g2 = pgn::parse(&out).unwrap();
    assert_eq!(g, g2, "round-trip mismatch:\n---OUT---\n{out}");
}

#[test]
fn comments_nags_variations_preserved() {
    let input = r#"[Event "X"]

1. e4 {best by test} e5 $1 (1... c5 {Sicilian}) 2. Nf3 Nc6 3. Bb5 *
"#;
    let g = pgn::parse(input).unwrap();
    assert_eq!(g.movetext[0].san, "e4");
    assert_eq!(g.movetext[0].post_comment.as_deref(), Some("best by test"));
    assert_eq!(g.movetext[1].san, "e5");
    assert_eq!(g.movetext[1].nags, vec![1]);
    assert_eq!(g.movetext[1].variations.len(), 1);
    assert_eq!(g.movetext[1].variations[0][0].san, "c5");
    let out = pgn::write(&g);
    let g2 = pgn::parse(&out).unwrap();
    assert_eq!(g, g2, "round-trip mismatch:\n---OUT---\n{out}");
}

#[test]
fn castling_round_trips() {
    let input = "1. e4 e5 2. Nf3 Nc6 3. Bc4 Nf6 4. O-O Be7 5. Re1 O-O 1-0\n";
    let g = pgn::parse(input).unwrap();
    let out = pgn::write(&g);
    let g2 = pgn::parse(&out).unwrap();
    assert_eq!(g, g2);
    assert!(g.movetext.iter().any(|n| n.san == "O-O"));
}

#[test]
fn empty_game_parses() {
    let g = pgn::parse("").unwrap();
    assert!(g.movetext.is_empty());
    assert!(g.result.is_none());
}
