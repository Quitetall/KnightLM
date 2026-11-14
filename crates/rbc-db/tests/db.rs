use rbc_db::{Db, GameQuery, SessionProfile};

const PGN: &str =
    "[Event \"T\"]\n[White \"A\"]\n[Black \"B\"]\n[Result \"1-0\"]\n\n1. e4 e5 2. Nf3 Nc6 1-0\n";

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
    let mut q = GameQuery {
        player: Some("A".into()),
        ..GameQuery::default()
    };
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
    let a = AnnotationInput {
        game_id: ids[0],
        ply: 1,
        path: Some("0".into()),
        comment: Some("good".into()),
        nag: Some(1),
        shapes: None,
    };
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
    db.import_pgn(
        "[Event \"T\"]\n[White \"A\"]\n[Black \"B\"]\n[Result \"1-0\"]\n\n1. e4 c5 2. Nf3 d6 1-0\n",
        None,
    )
    .unwrap();
    let fen = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
    let ex = db.explore(fen).unwrap();
    assert_eq!(ex.name.as_deref(), Some("King's Pawn Game"));
    let c = ex
        .continuations
        .iter()
        .find(|c| c.uci == "c7c5")
        .expect("c5");
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
    assert_eq!(
        s.get("engine_path").map(String::as_str),
        Some("/usr/bin/stockfish")
    );
    // Upsert rather than duplicate.
    db.set_setting("depth", "18").unwrap();
    assert_eq!(
        db.settings().unwrap().get("depth").map(String::as_str),
        Some("18")
    );
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
fn export_includes_saved_annotations_on_their_own_nodes() {
    let db = Db::open_memory().unwrap();
    let pgn =
        "[White \"A\"]\n[Black \"B\"]\n[Result \"*\"]\n\n1. e4 $1 {[%clk 0:04:58]} (1. d4 d5) e5 *";
    let id = db.import_pgn(pgn, None).unwrap()[0];
    let original = db.get(id).unwrap().unwrap().movetext;
    for input in [
        serde_json::json!({
            "game_id": id, "ply": 0, "path": "0", "comment": "Main {line}", "nag": 1,
            "shapes": r#"[{"kind":"arrow","from":"e2","to":"e4","color":"red"},{"kind":"circle","square":"e4","color":"blue"}]"#,
        }),
        serde_json::json!({"game_id": id, "ply": 0, "path": "0~0.0", "comment": "Branch", "nag": 5}),
        serde_json::json!({"game_id": id, "ply": 1, "comment": "Legacy main line", "nag": 2}),
        serde_json::json!({"game_id": id, "ply": 0, "path": "9~9.0", "comment": "Stale"}),
    ] {
        db.add_annotation(&serde_json::from_value(input).unwrap())
            .unwrap();
    }

    let exported = db.export_pgn(id).unwrap().unwrap();
    let tree = knightlm_notation::pgn::parse(&exported).unwrap().movetext;
    assert_eq!(tree[0].nags, vec![1]);
    assert_eq!(
        tree[0].post_comment.as_deref(),
        Some("[%clk 0:04:58] Main (line) [%cal Re2e4] [%csl Be4]")
    );
    assert_eq!(
        tree[0].variations[0][0].post_comment.as_deref(),
        Some("Branch")
    );
    assert_eq!(tree[0].variations[0][0].nags, vec![5]);
    assert_eq!(tree[1].post_comment.as_deref(), Some("Legacy main line"));
    assert!(!exported.contains("Stale"));
    assert_eq!(db.get(id).unwrap().unwrap().movetext, original);
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
    db.save_evals(7, 12, &[(0, Some(20), None), (1, None, Some(3))])
        .unwrap();
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
    let p = db
        .next_puzzle(Some(2600), Some("mateIn1"))
        .unwrap()
        .expect("theme puzzle");
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
        game_id: 1,
        ply: 1,
        path: Some("1".into()),
        comment: Some("main".into()),
        nag: None,
        shapes: None,
    })
    .unwrap();
    db.add_annotation(&AnnotationInput {
        game_id: 1,
        ply: 1,
        path: Some("1.0".into()),
        comment: Some("variation".into()),
        nag: None,
        shapes: None,
    })
    .unwrap();
    assert_eq!(db.annotations(1).unwrap().len(), 2);
    db.add_annotation(&AnnotationInput {
        game_id: 1,
        ply: 1,
        path: Some("1".into()),
        comment: Some("updated".into()),
        nag: None,
        shapes: None,
    })
    .unwrap();
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

    let card = db
        .next_repertoire_card(rep.id)
        .unwrap()
        .expect("another due card");
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
    assert!(
        db.update_pgn_tree(id, "1. d4 d5 (1... Nf6 2. c4) 2. c4 e6 *")
            .unwrap()
    );
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

#[test]
fn schema_is_versioned_and_sessions_round_trip() {
    let db = Db::open_memory().unwrap();
    assert_eq!(db.schema_version().unwrap(), rbc_db::SCHEMA_VERSION);
    assert!(db.load_session("client-1").unwrap().is_none());
    db.save_session("client-1", r#"{"tabs":["a"]}"#).unwrap();
    assert_eq!(
        db.load_session("client-1").unwrap().as_deref(),
        Some(r#"{"tabs":["a"]}"#)
    );
    db.save_session("client-1", r#"{"tabs":["b"]}"#).unwrap();
    assert_eq!(
        db.load_session("client-1").unwrap().as_deref(),
        Some(r#"{"tabs":["b"]}"#)
    );
    // Re-running the migration is a no-op and keeps the version.
    assert_eq!(db.schema_version().unwrap(), rbc_db::SCHEMA_VERSION);
}

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

#[test]
fn workspaces_hold_boards_and_revisions() {
    let db = Db::open_memory().unwrap();
    let ws = db.create_workspace("Analysis", "analysis").unwrap();
    assert_eq!(ws.position, 0);
    let b1 = db
        .create_board("workspace", &ws.id, "analysis", "Board 1", START, "{}")
        .unwrap();
    let b2 = db
        .create_board("workspace", &ws.id, "analysis", "Board 2", START, "{}")
        .unwrap();
    let boards = db.boards("workspace", &ws.id).unwrap();
    assert_eq!(boards.len(), 2);
    assert_eq!(boards[0].id, b1.id);
    assert_eq!(boards[1].id, b2.id);

    let updated = db
        .update_board_content(&b1.id, 1, r#"{"moves":["e4"]}"#, Some("test"))
        .unwrap();
    assert_eq!(updated.revision, 2);
    assert_eq!(updated.content_json, r#"{"moves":["e4"]}"#);
    assert_eq!(
        db.update_board_content(&b1.id, 1, "{}", None).unwrap_err(),
        rbc_db::BoardWriteError::Conflict { current: 2 }
    );

    let copy = db.copy_board(&b1.id, None, None, Some("Copy")).unwrap();
    assert_ne!(copy.id, b1.id);
    assert_eq!(copy.revision, 1);
    assert!(copy.metadata_json.contains("copied_from"));
    assert_eq!(copy.content_json, r#"{"moves":["e4"]}"#);
}

#[test]
fn locked_or_published_boards_require_a_copy() {
    let db = Db::open_memory().unwrap();
    let ws = db.create_workspace("Analysis", "analysis").unwrap();
    let board = db
        .create_board("workspace", &ws.id, "analysis", "Board", START, "{}")
        .unwrap();
    db.set_board_mutability(&board.id, "locked").unwrap();
    assert_eq!(
        db.update_board_content(&board.id, 1, "{}", None)
            .unwrap_err(),
        rbc_db::BoardWriteError::CopyRequired
    );
    db.set_board_mutability(&board.id, "mutable").unwrap();
    db.set_board_lifecycle(&board.id, "published").unwrap();
    assert_eq!(
        db.update_board_content(&board.id, 1, "{}", None)
            .unwrap_err(),
        rbc_db::BoardWriteError::CopyRequired
    );
    let copy = db.copy_board(&board.id, None, None, None).unwrap();
    assert_eq!(copy.mutability, "mutable");
    assert_eq!(copy.lifecycle, "draft");
}

#[test]
fn studies_copy_chapters_versions_and_progress() {
    let db = Db::open_memory().unwrap();
    let study = db.create_study("Sicilian", "A study").unwrap();
    let (chapter, primary) = db.add_chapter(&study.id, "Open", START, "notes").unwrap();
    let extra = db
        .add_board_to_chapter(&study.id, &chapter.id, "Line", START, "{}")
        .unwrap();
    let primary = db
        .update_board_content(&primary.id, 1, r#"{"moves":["e4"]}"#, Some("test"))
        .unwrap();
    let chapters = db.chapters(&study.id).unwrap();
    assert_eq!(chapters.len(), 1);
    assert_eq!(chapters[0].primary_board_id, primary.id);
    let ids: Vec<String> = serde_json::from_str(&chapters[0].board_ids_json).unwrap();
    assert_eq!(ids, vec![primary.id.clone(), extra.id.clone()]);

    db.reorder_chapters(&study.id, std::slice::from_ref(&chapter.id))
        .unwrap();
    let version = db.publish_study(&study.id, 1).unwrap();
    assert_eq!(version.version, 2);
    let published = db.study(&study.id).unwrap().unwrap();
    assert_eq!(published.lifecycle, "published");
    assert_eq!(published.current_version, 2);
    assert_eq!(db.study_versions(&study.id).unwrap().len(), 1);

    let original_board_count = db.boards("study_chapter", &chapter.id).unwrap().len();
    assert!(
        db.add_chapter(&study.id, "New chapter", &primary.start_fen, "")
            .is_err()
    );
    assert!(
        db.add_board_to_chapter(
            &study.id,
            &chapter.id,
            "New board",
            &primary.start_fen,
            "{}"
        )
        .is_err()
    );
    assert!(
        db.update_study(&study.id, Some("Rewritten"), None, None, None)
            .is_err()
    );
    assert!(
        db.update_chapter(&study.id, &chapter.id, None, Some("changed"))
            .is_err()
    );
    assert!(
        db.write_study_text(&study.id, Some(&chapter.id), "changed")
            .is_err()
    );
    assert!(db.delete_chapter(&study.id, &chapter.id).is_err());
    assert!(db.duplicate_chapter(&study.id, &chapter.id).is_err());
    assert!(
        db.reorder_chapters(&study.id, std::slice::from_ref(&chapter.id))
            .is_err()
    );
    assert!(
        db.set_chapter_primary_board(&study.id, &chapter.id, &primary.id)
            .is_err()
    );
    assert_eq!(db.chapters(&study.id).unwrap().len(), 1);
    assert_eq!(
        db.boards("study_chapter", &chapter.id).unwrap().len(),
        original_board_count
    );
    assert_eq!(
        db.study_versions(&study.id).unwrap()[0].bundle_json,
        version.bundle_json
    );

    let copy = db.copy_study(&study.id).unwrap();
    assert_ne!(copy.id, study.id);
    assert_eq!(copy.lifecycle, "draft");
    let copied_chapters = db.chapters(&copy.id).unwrap();
    assert_eq!(copied_chapters.len(), 1);
    let copied_boards = db.boards("study_chapter", &copied_chapters[0].id).unwrap();
    assert_eq!(copied_boards.len(), 2);
    assert_eq!(copied_boards[0].content_json, r#"{"moves":["e4"]}"#);

    db.set_study_progress(
        &study.id,
        &chapter.id,
        &primary.id,
        "local",
        true,
        1,
        Some(1.0),
        0,
        None,
    )
    .unwrap();
    let progress = db.study_progress(&study.id, "local").unwrap();
    assert_eq!(progress.len(), 1);
    assert!(progress[0].completed);
    assert_eq!(progress[0].attempts, 1);
}

#[test]
fn ai_sessions_persist_turns_in_order() {
    use rbc_db::AiTurnInput;
    let db = Db::open_memory().unwrap();
    let session = db
        .create_ai_session(
            "Test chat",
            "chat",
            None,
            None,
            SessionProfile {
                model_json: r#"{"model":"test"}"#,
                context_spec_json: r#"{"id":"chat-v1"}"#,
                permissions_json: "{}",
            },
        )
        .unwrap();
    assert_eq!(db.ai_sessions().unwrap().len(), 1);

    let first = AiTurnInput {
        role: "user".into(),
        content: "hello".into(),
        context_json: r#"{"position":{}}"#.into(),
        context_sha256: "abc".into(),
        ..AiTurnInput::default()
    };
    let turn = db.append_ai_turn(&session.id, &first).unwrap();
    assert_eq!(turn.seq, 1);
    assert_eq!(turn.content, "hello");

    let second = AiTurnInput {
        role: "assistant".into(),
        content: "hi".into(),
        tokens_in: 3,
        tokens_out: 1,
        ..AiTurnInput::default()
    };
    let turn = db.append_ai_turn(&session.id, &second).unwrap();
    assert_eq!(turn.seq, 2);

    let turns = db.ai_turns(&session.id).unwrap();
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0].seq, 1);
    assert_eq!(turns[1].seq, 2);

    let renamed = db.rename_ai_session(&session.id, "Renamed").unwrap();
    assert_eq!(renamed.title, "Renamed");
    db.archive_ai_session(&session.id).unwrap();
    assert!(db.ai_sessions().unwrap().is_empty());
}

#[test]
fn ai_matches_save_state_and_pgn() {
    let db = Db::open_memory().unwrap();
    let session = db
        .create_ai_session(
            "Match",
            "play",
            None,
            None,
            SessionProfile {
                model_json: "{}",
                context_spec_json: "{}",
                permissions_json: "{}",
            },
        )
        .unwrap();
    let game = db
        .create_ai_match(
            Some(&session.id),
            r#"{"kind":"human","name":"A"}"#,
            r#"{"kind":"model","provider":"p","model":"m"}"#,
            r#"{"max_plies":300}"#,
        )
        .unwrap();
    assert!(!game.rated);
    let saved = db
        .save_ai_match(
            &game.id,
            r#"{"state":"finished"}"#,
            "[Event \"x\"]\n\n1. e4 *\n",
            Some(r#"{"score":"1-0"}"#),
            Some("checkmate"),
            true,
        )
        .unwrap();
    assert_eq!(saved.termination.as_deref(), Some("checkmate"));
    assert!(saved.ended.is_some());
    assert!(saved.pgn.contains("e4"));
}

#[test]
fn ai_session_budget_refuses_a_turn_once_spent() {
    let db = Db::open_memory().unwrap();
    let session = db
        .create_ai_session(
            "Budgeted",
            "chat",
            None,
            None,
            SessionProfile {
                model_json: "{}",
                context_spec_json: "{}",
                permissions_json: "{}",
            },
        )
        .unwrap();
    // No ceiling means it always may.
    assert!(db.ai_session_has_budget(&session.id).unwrap());
    assert_eq!(session.token_budget, None);
    assert_eq!(session.tokens_used, 0);

    // With a ceiling of 100 tokens, 60 spent leaves room and 40 more does not.
    db.set_ai_session_budget(&session.id, Some(100)).unwrap();
    assert!(db.ai_session_has_budget(&session.id).unwrap());
    assert_eq!(db.add_ai_session_tokens(&session.id, 60).unwrap(), 60);
    assert!(db.ai_session_has_budget(&session.id).unwrap());
    assert_eq!(db.add_ai_session_tokens(&session.id, 40).unwrap(), 100);
    assert!(
        !db.ai_session_has_budget(&session.id).unwrap(),
        "a session at its ceiling takes no more turns"
    );

    // Clearing the ceiling lets it run again.
    db.set_ai_session_budget(&session.id, None).unwrap();
    assert!(db.ai_session_has_budget(&session.id).unwrap());
}

#[test]
fn ai_session_budget_survives_a_reopen_of_the_row() {
    let db = Db::open_memory().unwrap();
    let session = db
        .create_ai_session(
            "Persisted",
            "chat",
            None,
            None,
            SessionProfile {
                model_json: "{}",
                context_spec_json: "{}",
                permissions_json: "{}",
            },
        )
        .unwrap();
    db.set_ai_session_budget(&session.id, Some(500)).unwrap();
    db.add_ai_session_tokens(&session.id, 120).unwrap();

    let reloaded = db.ai_session(&session.id).unwrap().unwrap();
    assert_eq!(reloaded.token_budget, Some(500));
    assert_eq!(reloaded.tokens_used, 120);
}
