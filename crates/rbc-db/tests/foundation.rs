//! Foundation fixtures: a v3 database opened by this build, and the board and
//! study repositories driven end to end.
//!
//! These are fixtures rather than unit tests: each one builds the state a real
//! installation would have (an older schema on disk, a workspace with several
//! boards, a study with chapters and versions) and asserts what a person would
//! notice after the upgrade or after the first session.

use rbc_db::{Db, SessionProfile};

/// A database left behind by the previous release: a session row and a game,
/// stamped at version 3.
///
/// The schema itself is created by this build and then labelled v3, because the
/// migration this fixture exercises is the **v4 session reset** — the part a
/// person notices. A hand-written pre-v4 `games` table would only prove that my
/// guess at the old schema matched my own query.
fn write_v3_database(path: &str) {
    let db = Db::open(path).expect("create database");
    // A game through the real import path, so the row is well formed.
    db.import_pgn(
        "[Event \"Legacy cup\"]\n[White \"A\"]\n[Black \"B\"]\n[Result \"1-0\"]\n\n1. e4 e5 2. Nf3 1-0\n",
        None,
    )
    .expect("import");
    {
        let conn = rusqlite::Connection::open(path).expect("reopen for the fixture rows");
        conn.execute_batch(
            "INSERT OR REPLACE INTO sessions (client, payload, updated)
                 VALUES ('old-client', '{\"tabs\":[{\"kind\":\"analysis\"}]}', 1);
             UPDATE schema_meta SET value = '3' WHERE key = 'version';",
        )
        .expect("fixture rows");
    }
    drop(db);
}

#[test]
fn fixture_a_v3_database_upgrades_and_keeps_its_games() {
    let dir = std::env::temp_dir().join(format!("rbc-fixture-v3-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("v3.db");
    write_v3_database(path.to_str().expect("utf8 path"));

    // Opening the file is the migration.
    let db = Db::open(path.to_str().expect("utf8 path")).expect("open v3 database");
    assert_eq!(
        db.schema_version().expect("version"),
        rbc_db::SCHEMA_VERSION,
        "opening an old database brings it to the current schema"
    );

    // The games are untouched.
    let response = db.query(&Default::default()).expect("games");
    assert_eq!(
        response.total, 1,
        "the chess database is not part of the reset"
    );
    assert_eq!(response.games[0].white, "A");

    // The old session moved aside and is still readable.
    assert_eq!(
        db.legacy_session_count().expect("legacy"),
        1,
        "the reset keeps the old session for one release"
    );
    assert_eq!(
        db.load_session("old-client").expect("session"),
        None,
        "and clears the live table, so the notice has something to say"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fixture_a_workspace_holds_several_boards_and_the_revision_wall_holds() {
    let db = Db::open_memory().expect("db");
    let workspace = db
        .create_workspace("Study of the Ruy Lopez", "analysis")
        .expect("workspace");
    let start = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    let first = db
        .create_board(
            "workspace",
            &workspace.id,
            "analysis",
            "Main line",
            start,
            "",
        )
        .expect("board one");
    let second = db
        .create_board(
            "workspace",
            &workspace.id,
            "analysis",
            "Sideline",
            start,
            "",
        )
        .expect("board two");

    let boards = db.boards("workspace", &workspace.id).expect("list");
    assert_eq!(boards.len(), 2, "a workspace holds both boards");
    assert_eq!(first.revision, 1);
    assert_eq!(second.revision, 1, "each board counts its own revisions");

    // A write with the revision it saw succeeds and increments.
    let updated = db
        .update_board_content(&first.id, 1, r#"{"moves":["e4"]}"#, Some("test"))
        .expect("write at revision 1");
    assert_eq!(updated.revision, 2);

    // The same revision again is a conflict, not an overwrite.
    let conflict = db
        .update_board_content(&first.id, 1, r#"{"moves":["d4"]}"#, Some("test"))
        .expect_err("a stale revision is refused");
    assert!(
        format!("{conflict}").contains("2"),
        "the refusal names the current revision: {conflict}"
    );
    let after = db.board(&first.id).expect("read").expect("present");
    assert_eq!(after.revision, 2, "and the refused write changed nothing");
}

#[test]
fn fixture_a_locked_board_requires_a_copy_before_it_moves() {
    let db = Db::open_memory().expect("db");
    let workspace = db.create_workspace("Locked", "analysis").expect("ws");
    let start = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    let board = db
        .create_board(
            "workspace",
            &workspace.id,
            "analysis",
            "Original",
            start,
            "",
        )
        .expect("board");

    db.set_board_mutability(&board.id, "locked").expect("lock");

    let refused = db
        .update_board_content(&board.id, 1, r#"{"moves":["e4"]}"#, Some("test"))
        .expect_err("a locked board refuses the write");
    assert!(
        format!("{refused}").to_lowercase().contains("copy"),
        "and it offers a copy: {refused}"
    );

    // Copying is the way forward, and the copy is written in, not locked.
    let copy = db
        .copy_board(&board.id, None, None, Some("Copy of Original"))
        .expect("copy");
    assert_eq!(copy.mutability, "mutable");
    assert_eq!(copy.revision, 1);
    assert_ne!(copy.id, board.id);
    db.update_board_content(&copy.id, 1, r#"{"moves":["e4"]}"#, Some("test"))
        .expect("the copy takes the write");
}

#[test]
fn fixture_a_study_keeps_chapters_versions_and_progress_apart() {
    let db = Db::open_memory().expect("db");
    let study = db.create_study("Openings", "a study").expect("study");

    let start = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    let (first, _) = db
        .add_chapter(&study.id, "Italian", start, "")
        .expect("chapter one");
    let (second, _) = db
        .add_chapter(&study.id, "Sicilian", start, "")
        .expect("chapter two");

    let chapters = db.chapters(&study.id).expect("chapters");
    assert_eq!(chapters.len(), 2, "both chapters are there");
    assert_eq!(chapters[0].id, first.id, "in the order they were added");

    // Reordering is its own operation and does not lose a chapter.
    db.reorder_chapters(&study.id, &[second.id.clone(), first.id.clone()])
        .expect("reorder");
    let reordered = db.chapters(&study.id).expect("chapters");
    assert_eq!(reordered[0].id, second.id, "the reorder took effect");

    // A version is a snapshot; publishing marks the study, not the chapter.
    let version = db.publish_study(&study.id, 1).expect("publish");
    assert!(!version.id.is_empty());
    let published = db.study(&study.id).expect("read").expect("present");
    assert_eq!(published.current_version, 2, "publishing bumps the version");

    // Progress is per chapter and survives a reload.
    db.set_study_progress(&study.id, &first.id, "", "you", true, 1, None, 0, None)
        .expect("progress");
    let progress = db.study_progress(&study.id, "you").expect("progress list");
    assert_eq!(progress.len(), 1, "one chapter recorded as done");
    assert!(progress[0].completed);
}

#[test]
fn fixture_a_session_and_its_turns_survive_a_reopen() {
    let dir = std::env::temp_dir().join(format!("rbc-fixture-session-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("sessions.db");

    let session_id = {
        let db = Db::open(path.to_str().expect("utf8 path")).expect("db");
        let session = db
            .create_ai_session(
                "Ruy Lopez",
                "chat",
                None,
                None,
                SessionProfile {
                    model_json: r#"{"provider":"mock","model":"m1"}"#,
                    context_spec_json: r#"{"id":"chat-v1","version":"1"}"#,
                    permissions_json: "{}",
                },
            )
            .expect("session");
        db.append_ai_turn(
            &session.id,
            &rbc_db::AiTurnInput {
                role: "user".into(),
                content: "what should I play?".into(),
                context_json: "{}".into(),
                context_sha256: "abc".into(),
                ..Default::default()
            },
        )
        .expect("turn");
        session.id
    };

    // Reopen: the session and its transcript are still there.
    let db = Db::open(path.to_str().expect("utf8 path")).expect("reopen");
    let turns = db.ai_turns(&session_id).expect("turns");
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].content, "what should I play?");
    assert_eq!(turns[0].seq, 1);

    let _ = std::fs::remove_dir_all(&dir);
}
