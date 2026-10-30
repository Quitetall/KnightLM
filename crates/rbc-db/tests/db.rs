use rbc_db::{Db, GameQuery};

const PGN: &str = "[Event \"T\"]\n[White \"A\"]\n[Black \"B\"]\n[Result \"1-0\"]\n\n1. e4 e5 2. Nf3 Nc6 1-0\n";

#[test]
fn import_query_and_get() {
    let db = Db::open_memory().unwrap();
    let ids = db.import_pgn(PGN, None).unwrap();
    assert_eq!(ids.len(), 1);
    assert_eq!(db.count().unwrap(), 1);
    let g = db.get(ids[0]).unwrap().unwrap();
    assert_eq!(g.white, "A");
    assert_eq!(g.black, "B");
    assert_eq!(g.result, "1-0");
    assert_eq!(g.moves, "e4 e5 Nf3 Nc6");
    assert_eq!(g.ply_count, 4);
    let r = db.query(&GameQuery::default()).unwrap();
    assert_eq!(r.total, 1);
    assert_eq!(r.games.len(), 1);
}

#[test]
fn query_filters() {
    let db = Db::open_memory().unwrap();
    db.import_pgn(PGN, None).unwrap();
    let mut q = GameQuery::default();
    q.player = Some("A".into());
    assert_eq!(db.query(&q).unwrap().total, 1);
    q.player = Some("Nobody".into());
    assert_eq!(db.query(&q).unwrap().total, 0);
}

#[test]
fn position_search() {
    let db = Db::open_memory().unwrap();
    db.import_pgn(PGN, None).unwrap();
    let fen = "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2";
    let games = db.search_position(fen).unwrap();
    assert_eq!(games.len(), 1);
    let games = db.search_position("4k3/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
    assert!(games.is_empty());
}

#[test]
fn collections_and_catalogs() {
    let db = Db::open_memory().unwrap();
    let c = db.create_collection("Study", Some("test")).unwrap();
    db.import_pgn(PGN, Some(c.id)).unwrap();
    let cols = db.collections().unwrap();
    assert_eq!(cols.len(), 1);
    assert_eq!(cols[0].games, 1);
    let players = db.players().unwrap();
    assert_eq!(players.len(), 2);
    let events = db.events().unwrap();
    assert_eq!(events[0].name, "T");
    assert_eq!(events[0].games, 1);
}

#[test]
fn annotations() {
    use rbc_db::AnnotationInput;
    let db = Db::open_memory().unwrap();
    let ids = db.import_pgn(PGN, None).unwrap();
    let a = AnnotationInput { game_id: ids[0], ply: 1, path: Some("0".into()), comment: Some("good".into()), nag: Some(1), shapes: None };
    db.add_annotation(&a).unwrap();
    let all = db.annotations(ids[0]).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].comment.as_deref(), Some("good"));
}

#[test]
fn opening_classification() {
    // After 1. e4 c5
    let fen = "rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2";
    let o = rbc_db::classify_opening(fen).expect("opening");
    assert!(o.name.contains("Sicilian"), "got {}", o.name);
    assert_eq!(o.eco, "B20");
}

#[test]
fn explorer_counts_continuations() {
    let db = Db::open_memory().unwrap();
    db.import_pgn("[Event \"T\"]\n[White \"A\"]\n[Black \"B\"]\n[Result \"1-0\"]\n\n1. e4 c5 2. Nf3 d6 1-0\n", None).unwrap();
    let fen = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
    let ex = db.explore(fen).unwrap();
    assert_eq!(ex.name.as_deref(), Some("King's Pawn Game"));
    let c = ex.continuations.iter().find(|c| c.uci == "c7c5").expect("c5");
    assert_eq!(c.games, 1);
    assert_eq!(c.san, "c5");
}

#[test]
fn puzzles_are_seeded_and_solvable() {
    let db = Db::open_memory().unwrap();
    let p = db.next_puzzle(Some(1500), None).unwrap().expect("puzzle");
    assert!(!p.fen.is_empty());
    assert!(!p.moves_san.is_empty(), "solution SAN should be derived");
    assert!(p.rating > 0);
}

#[test]
fn puzzle_import_accepts_headerless_csv() {
    let db = Db::open_memory().unwrap();
    let row = "99999,r6k/pp2r2p/4Rp1Q/3p4/8/1N1P2R1/PqP2bPP/7K b - - 0 24,\
               f2g3 e6e7 b2b1 b3c1 b1c1 h6c1,1811,76,95,10282,crushing,https://lichess.org/x,,";
    assert_eq!(db.import_puzzles_csv(row).unwrap(), 1);
    // Re-importing the same puzzle inserts nothing new.
    assert_eq!(db.import_puzzles_csv(row).unwrap(), 0);
}

#[test]
fn settings_round_trip() {
    let db = Db::open_memory().unwrap();
    assert!(db.settings().unwrap().is_empty());
    db.set_setting("depth", "12").unwrap();
    db.set_setting("engine_path", "/usr/bin/stockfish").unwrap();
    let s = db.settings().unwrap();
    assert_eq!(s.get("depth").map(String::as_str), Some("12"));
    assert_eq!(s.get("engine_path").map(String::as_str), Some("/usr/bin/stockfish"));
    // Upsert rather than duplicate.
    db.set_setting("depth", "18").unwrap();
    assert_eq!(db.settings().unwrap().get("depth").map(String::as_str), Some("18"));
}

#[test]
fn annotation_replaces_same_ply() {
    let db = Db::open_memory().unwrap();
    let input = r#"{"game_id":1,"ply":2,"comment":"first","nag":1}"#;
    let a: rbc_db::AnnotationInput = serde_json::from_str(input).unwrap();
    db.add_annotation(&a).unwrap();
    let b: rbc_db::AnnotationInput =
        serde_json::from_str(r#"{"game_id":1,"ply":2,"comment":"second","nag":2}"#).unwrap();
    db.add_annotation(&b).unwrap();
    let anns = db.annotations(1).unwrap();
    assert_eq!(anns.len(), 1);
    assert_eq!(anns[0].comment.as_deref(), Some("second"));
    assert_eq!(anns[0].nag, Some(2));
}

#[test]
fn movetext_variations_round_trip_through_export() {
    let db = Db::open_memory().unwrap();
    let pgn = "[Event \"Var\"]\n[White \"A\"]\n[Black \"B\"]\n[Result \"*\"]\n\n1. e4! {best} (1. d4 d5 2. c4) 1... e5 2. Nf3 *\n";
    let ids = db.import_pgn(pgn, None).unwrap();
    let g = db.get(ids[0]).unwrap().unwrap();
    let tree = g.movetext.expect("movetext stored");
    assert!(tree.contains("variations"));
    assert!(tree.contains("d4"));
    let out = db.export_pgn(ids[0]).unwrap().unwrap();
    assert!(out.contains("(1. d4 d5 2. c4) 1... e5"), "{out}");
    assert!(out.contains("$1"), "{out}");
    assert!(out.contains("{best}"), "{out}");
    assert!(out.contains("[Event \"Var\"]"), "{out}");
}

#[test]
fn evals_round_trip() {
    let db = Db::open_memory().unwrap();
    db.save_evals(7, 12, &[(0, Some(20), None), (1, None, Some(3))]).unwrap();
    let rows = db.evals(7, 12).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].cp, Some(20));
    assert_eq!(rows[1].mate, Some(3));
    // Re-saving at the same depth replaces the set.
    db.save_evals(7, 12, &[(0, Some(0), None)]).unwrap();
    assert_eq!(db.evals(7, 12).unwrap().len(), 1);
}

#[test]
fn puzzle_theme_filter_and_rating() {
    let db = Db::open_memory().unwrap();
    let p = db.next_puzzle(Some(2600), Some("mateIn1")).unwrap().expect("theme puzzle");
    assert!(p.themes.as_deref().unwrap_or("").contains("mateIn1"));
    let all = db.puzzle_themes().unwrap();
    assert!(all.iter().any(|(t, _)| t == "middlegame"));
    assert_eq!(db.puzzle_rating().unwrap(), 1500);
    let (before, after) = db.record_puzzle_attempt(&p.id, p.rating, true).unwrap();
    assert_eq!(before, 1500);
    assert!(after > before, "a win should raise the rating");
    let (before2, after2) = db.record_puzzle_attempt(&p.id, p.rating, false).unwrap();
    assert_eq!(before2, after);
    assert!(after2 < before2, "a loss should lower the rating");
    assert_eq!(db.puzzle_attempts(10).unwrap().len(), 2);
}

#[test]
fn annotations_are_scoped_by_path() {
    use rbc_db::AnnotationInput;
    let db = Db::open_memory().unwrap();
    db.add_annotation(&AnnotationInput {
        game_id: 1, ply: 1, path: Some("1".into()), comment: Some("main".into()), nag: None, shapes: None,
    }).unwrap();
    db.add_annotation(&AnnotationInput {
        game_id: 1, ply: 1, path: Some("1.0".into()), comment: Some("variation".into()), nag: None, shapes: None,
    }).unwrap();
    assert_eq!(db.annotations(1).unwrap().len(), 2);
    db.add_annotation(&AnnotationInput {
        game_id: 1, ply: 1, path: Some("1".into()), comment: Some("updated".into()), nag: None, shapes: None,
    }).unwrap();
    let all = db.annotations(1).unwrap();
    assert_eq!(all.len(), 2);
    assert!(all.iter().any(|a| a.comment.as_deref() == Some("updated")));
}

#[test]
fn delete_game_removes_dependents() {
    let db = Db::open_memory().unwrap();
    let ids = db.import_pgn(PGN, None).unwrap();
    db.save_evals(ids[0], 10, &[(0, Some(0), None)]).unwrap();
    assert!(db.delete_game(ids[0]).unwrap());
    assert!(db.get(ids[0]).unwrap().is_none());
    assert!(db.evals(ids[0], 10).unwrap().is_empty());
    assert!(!db.delete_game(ids[0]).unwrap());
}

#[test]
fn update_game_tags() {
    let db = Db::open_memory().unwrap();
    let ids = db.import_pgn(PGN, None).unwrap();
    let tags = rbc_db::GameTags {
        event: Some("New Event".into()),
        white: Some("Alice".into()),
        white_elo: Some(2400),
        ..Default::default()
    };
    assert!(db.update_tags(ids[0], &tags).unwrap());
    let g = db.get(ids[0]).unwrap().unwrap();
    assert_eq!(g.event.as_deref(), Some("New Event"));
    assert_eq!(g.white, "Alice");
    assert_eq!(g.white_elo, Some(2400));
    assert_eq!(g.black, "B", "untouched fields stay");
    assert!(!db.update_tags(999, &tags).unwrap());
}

#[test]
fn repertoire_import_and_spaced_repetition() {
    let db = Db::open_memory().unwrap();
    let rep = db.create_repertoire("White e4", "white").unwrap();
    assert_eq!(rep.color, "white");
    let pgn = "[Event \"Rep\"]\n[White \"A\"]\n[Black \"B\"]\n[Result \"*\"]\n\n1. e4 e5 2. Nf3 (2. Bc4 Nf6) Nc6 *\n";
    let added = db.import_repertoire_pgn(rep.id, pgn).unwrap();
    assert_eq!(added, 3, "e4, Nf3 and Bc4 are White moves");
    let list = db.repertoires().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].cards, 3);
    assert_eq!(list[0].due, 3);

    let card = db.next_repertoire_card(rep.id).unwrap().expect("due card");
    let reviewed = db.review_repertoire_card(card.id, true).unwrap();
    assert_eq!(reviewed.reviews, 1);
    assert_eq!(reviewed.interval_days, 1.0);
    assert!(reviewed.due_at > card.due_at || reviewed.due_at != card.due_at);

    let card = db.next_repertoire_card(rep.id).unwrap().expect("another due card");
    let failed = db.review_repertoire_card(card.id, false).unwrap();
    assert_eq!(failed.lapses, 1);
    assert!(failed.ease < 2.5);

    // A black repertoire keeps Black's moves only.
    let black = db.create_repertoire("Black e5", "black").unwrap();
    let added = db.import_repertoire_pgn(black.id, pgn).unwrap();
    assert_eq!(added, 3, "e5, Nf6 and Nc6 are Black moves");
    assert!(db.delete_repertoire(black.id).unwrap());
    assert!(db.repertoire(black.id).unwrap().is_none());
}

#[test]
fn imports_and_indexes_chess960() {
    let db = Db::open_memory().unwrap();
    let pgn = "[Event \"960\"]\n[Variant \"Chess960\"]\n[SetUp \"1\"]\n[FEN \"rk5r/8/8/8/8/8/8/RK5R w HAha - 0 1\"]\n[White \"A\"]\n[Black \"B\"]\n[Result \"*\"]\n\n1. O-O O-O-O *\n";
    let ids = db.import_pgn(pgn, None).unwrap();
    let g = db.get(ids[0]).unwrap().unwrap();
    assert_eq!(g.fen, "rk5r/8/8/8/8/8/8/RK5R w HAha - 0 1");
    assert_eq!(g.moves, "O-O O-O-O");
    // Import only succeeds when the 960 UCI castling moves index cleanly.
    let found = db
        .search_position("rk5r/8/8/8/8/8/8/RK5R w HAha - 0 1")
        .unwrap();
    assert_eq!(found.len(), 1, "960 initial position indexed");
}

#[test]
fn collections_can_be_renamed_and_deleted() {
    let db = Db::open_memory().unwrap();
    let c = db.create_collection("Original", None).unwrap();
    assert!(db.rename_collection(c.id, "Renamed").unwrap());
    let list = db.collections().unwrap();
    assert_eq!(list[0].name, "Renamed");
    assert!(db.delete_collection(c.id).unwrap());
    assert!(db.collections().unwrap().is_empty());
    assert!(!db.delete_collection(c.id).unwrap());
}

#[test]
fn update_moves_validates_and_reindexes() {
    let db = Db::open_memory().unwrap();
    let ids = db.import_pgn(PGN, None).unwrap();
    // invalid move: the game is left untouched
    assert!(db.update_moves(ids[0], "e4 e5 Nf6 Nf6").is_err());
    assert_eq!(db.get(ids[0]).unwrap().unwrap().moves, "e4 e5 Nf3 Nc6");

    assert!(db.update_moves(ids[0], "1. d4 d5 2. c4").unwrap());
    let g = db.get(ids[0]).unwrap().unwrap();
    assert_eq!(g.moves, "d4 d5 c4");
    assert_eq!(g.ply_count, 3);

    // New positions are indexed, the old line is gone.
    let after_d4 = "rnbqkbnr/pppppppp/8/8/3P4/8/PPP1PPPP/RNBQKBNR b KQkq - 0 1";
    assert_eq!(db.search_position(after_d4).unwrap().len(), 1);
    let after_e4 = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
    assert!(db.search_position(after_e4).unwrap().is_empty());
}

#[test]
fn imports_non_replayable_variants_without_losing_moves() {
    let db = Db::open_memory().unwrap();
    let pgn = "[Event \"Atomic\"]\n[Variant \"Atomic\"]\n[White \"A\"]\n[Black \"B\"]\n[Result \"1-0\"]\n\n1. e4 e5 2. Nf3 Nc6 1-0\n";
    let ids = db.import_pgn(pgn, None).unwrap();
    let g = db.get(ids[0]).unwrap().unwrap();
    assert_eq!(g.variant.as_deref(), Some("atomic"));
    assert_eq!(g.moves, "e4 e5 Nf3 Nc6");
    assert!(g.movetext.is_some());
    // Variant positions are indexed under a variant-qualified key.
    let found = db
        .search_position_variant(
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
            "atomic",
        )
        .unwrap();
    assert_eq!(found.len(), 1);
    // Export still round-trips the moves and the Variant tag.
    let out = db.export_pgn(ids[0]).unwrap().unwrap();
    assert!(out.contains("[Variant \"Atomic\"]"));
    assert!(out.contains("Nc6"));
}

#[test]
fn tree_editing_keeps_variations_consistent() {
    let db = Db::open_memory().unwrap();
    let pgn = "[Event \"Tree\"]\n[White \"A\"]\n[Black \"B\"]\n[Result \"*\"]\n\n1. e4 e5 2. Nf3 (2. Bc4 Nf6) Nc6 *\n";
    let ids = db.import_pgn(pgn, None).unwrap();
    let id = ids[0];

    // Rewrite the whole tree from PGN text, including a variation.
    assert!(db
        .update_pgn_tree(id, "1. d4 d5 (1... Nf6 2. c4) 2. c4 e6 *")
        .unwrap());
    let g = db.get(id).unwrap().unwrap();
    assert_eq!(g.moves, "d4 d5 c4 e6");
    assert!(g.movetext.as_deref().unwrap().contains("Nf6"));
    // Illegal trees are refused and leave the game unchanged.
    assert!(db.update_pgn_tree(id, "1. e4 e5 2. Nf6").is_err());
    assert_eq!(db.get(id).unwrap().unwrap().moves, "d4 d5 c4 e6");

    // Restore a tree with a variation, then promote it.
    db.update_pgn_tree(id, "1. e4 e5 2. Nf3 (2. Bc4 Nf6) Nc6 *")
        .unwrap();
    assert!(db.promote_variation(id, "2~0.0").unwrap());
    let g = db.get(id).unwrap().unwrap();
    assert_eq!(g.moves, "e4 e5 Bc4 Nf6");
    assert!(g.movetext.as_deref().unwrap().contains("Nf3"));

    // Deleting the (now demoted) variation removes it.
    assert!(db.delete_tree_node(id, "2~0.0").unwrap());
    let g = db.get(id).unwrap().unwrap();
    assert_eq!(g.moves, "e4 e5 Bc4 Nf6");
    assert!(!g.movetext.as_deref().unwrap().contains("Nf3"));
}
