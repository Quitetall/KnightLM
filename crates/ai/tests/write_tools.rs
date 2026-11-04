//! Board and study writes against a real (in-memory) repository.
//!
//! These are the checks the ticket asks for: a move is validated against the
//! board's own move list, a stale revision is a conflict and not an overwrite,
//! a locked board asks for a copy, and content this build does not model
//! survives a write.
#![cfg(feature = "host")]

use rbc_ai::agent::ToolHost;
use rbc_ai::board::{BoardContent, BoardWriteHost};
use rbc_ai::protocol::{ToolCall, result_code};
use rbc_db::{Board, Db};

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

fn setup() -> (Db, Board) {
    let db = Db::open_memory().expect("in-memory database");
    let workspace = db
        .create_workspace("Analysis", "analysis")
        .expect("workspace");
    let board = db
        .create_board("workspace", &workspace.id, "analysis", "Board 1", START, "")
        .expect("board");
    (db, board)
}

fn call(name: &str, arguments: serde_json::Value) -> ToolCall {
    ToolCall::new("call_1", name, arguments)
}

fn move_args(board: &Board, text: &str) -> serde_json::Value {
    serde_json::json!({
        "board_id": board.id,
        "move": text,
        "expected_revision": board.revision,
    })
}

fn fresh(db: &Db, board: &Board) -> Board {
    db.board(&board.id).expect("board").expect("row")
}

fn edit_args(board: &Board, extra: serde_json::Value) -> serde_json::Value {
    let mut object = extra.as_object().cloned().unwrap_or_default();
    object.insert("board_id".into(), serde_json::json!(board.id));
    object.insert(
        "expected_revision".into(),
        serde_json::json!(board.revision),
    );
    serde_json::Value::Object(object)
}

fn content_of(db: &Db, board: &Board) -> BoardContent {
    let stored = db.board(&board.id).expect("board").expect("board row");
    BoardContent::parse(&stored.content_json).expect("content")
}

// --- play_move -------------------------------------------------------------

#[test]
fn write_tools_play_move_applies_san_and_bumps_the_revision() {
    let (db, board) = setup();
    let mut host = BoardWriteHost::new(&db, "test");

    let result = host.call(&call("play_move", move_args(&board, "e4")));

    assert!(result.ok, "{result:?}");
    assert_eq!(result.code, result_code::APPLIED);
    assert_eq!(result.summary, "played e4");
    assert_eq!(result.revision, Some(2));
    assert_eq!(result.data.as_ref().expect("data")["san"], serde_json::json!("e4"));
    let fen = result.data.as_ref().expect("data")["fen"]
        .as_str()
        .expect("fen")
        .to_string();
    assert!(fen.starts_with("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b"), "{fen}");

    let stored = db.board(&board.id).expect("board").expect("row");
    assert_eq!(stored.revision, 2);
    assert_eq!(stored.content_json, r#"{"moves":["e4"]}"#);
}

#[test]
fn write_tools_play_move_accepts_uci_and_a_whole_line() {
    let (db, board) = setup();
    let mut host = BoardWriteHost::new(&db, "test");

    for (text, revision) in [("e2e4", board.revision), ("e7e5", 2), ("Nf3", 3)] {
        let mut args = move_args(&board, text);
        args["expected_revision"] = serde_json::json!(revision);
        let result = host.call(&call("play_move", args));
        assert!(result.ok, "{text}: {result:?}");
    }

    let content = content_of(&db, &board);
    assert_eq!(content.moves, vec!["e4", "e5", "Nf3"]);
}

#[test]
fn write_tools_play_move_refuses_an_illegal_move() {
    let (db, board) = setup();
    let mut host = BoardWriteHost::new(&db, "test");

    let result = host.call(&call("play_move", move_args(&board, "e5")));

    assert!(!result.ok);
    assert_eq!(result.code, result_code::ILLEGAL);
    assert!(result.summary.contains("e5"));
    let stored = db.board(&board.id).expect("board").expect("row");
    assert_eq!(stored.revision, 1, "a refused move changes nothing");
    assert_eq!(stored.content_json, "");
}

#[test]
fn write_tools_play_move_reports_the_current_revision_on_conflict() {
    let (db, board) = setup();
    let mut host = BoardWriteHost::new(&db, "test");

    // Someone else already moved the board on; after d4 it is Black's turn.
    db.update_board_content(&board.id, 1, r#"{"moves":["d4"]}"#, Some("other"))
        .expect("other write");

    let mut args = move_args(&board, "Nf6");
    args["expected_revision"] = serde_json::json!(1);
    let result = host.call(&call("play_move", args));

    assert!(!result.ok);
    assert_eq!(result.code, result_code::CONFLICT);
    assert_eq!(
        result.data.as_ref().expect("data")["current_revision"],
        serde_json::json!(2)
    );
    assert!(result.summary.contains("revision 2"));
    // The other write is intact.
    assert_eq!(content_of(&db, &board).moves, vec!["d4"]);
}

#[test]
fn write_tools_play_move_asks_for_a_copy_of_a_locked_board() {
    let (db, board) = setup();
    db.set_board_mutability(&board.id, "locked").expect("lock");
    let mut host = BoardWriteHost::new(&db, "test");

    let result = host.call(&call("play_move", move_args(&board, "e4")));

    assert!(!result.ok);
    assert_eq!(result.code, result_code::COPY_REQUIRED);
    assert!(result.summary.contains("copy"));
}

// --- shapes, annotations, notes --------------------------------------------

#[test]
fn write_tools_draw_shapes_and_clear_them() {
    let (db, board) = setup();
    let mut host = BoardWriteHost::new(&db, "test");

    let arrow = host.call(&call(
        "draw_arrow",
        edit_args(
            &board,
            serde_json::json!({"from": "e2", "to": "e4", "color": "green"}),
        ),
    ));
    assert!(arrow.ok, "{arrow:?}");

    let board = fresh(&db, &board);
    let circle = host.call(&call(
        "draw_circle",
        edit_args(
            &board,
            serde_json::json!({"square": "d5", "color": "red"}),
        ),
    ));
    assert!(circle.ok, "{circle:?}");

    // The same arrow twice replaces rather than doubles.
    let board = fresh(&db, &board);
    let again = host.call(&call(
        "draw_arrow",
        edit_args(&board, serde_json::json!({"from": "e2", "to": "e4"})),
    ));
    assert!(again.ok, "{again:?}");

    let content = content_of(&db, &board);
    assert_eq!(content.arrows.len(), 1);
    assert_eq!(content.circles.len(), 1);
    assert_eq!(content.circles[0].square, "d5");

    let board = fresh(&db, &board);
    let cleared = host.call(&call("clear_shapes", edit_args(&board, serde_json::json!({}))));
    assert!(cleared.ok, "{cleared:?}");
    let content = content_of(&db, &board);
    assert!(content.arrows.is_empty());
    assert!(content.circles.is_empty());
}

#[test]
fn write_tools_refuse_a_shape_that_is_not_a_square() {
    let (db, board) = setup();
    let mut host = BoardWriteHost::new(&db, "test");

    let result = host.call(&call(
        "draw_arrow",
        edit_args(&board, serde_json::json!({"from": "z9", "to": "e4"})),
    ));

    assert!(!result.ok);
    assert_eq!(result.code, result_code::INVALID_ARGUMENTS);
}

#[test]
fn write_tools_annotate_replaces_the_same_ply() {
    let (db, board) = setup();
    let mut host = BoardWriteHost::new(&db, "test");

    let first = host.call(&call(
        "annotate",
        edit_args(&board, serde_json::json!({"ply": 2, "nag": "!?", "comment": "interesting"})),
    ));
    assert!(first.ok, "{first:?}");

    let board = fresh(&db, &board);
    let second = host.call(&call(
        "annotate",
        edit_args(&board, serde_json::json!({"ply": 2, "comment": "better"})),
    ));
    assert!(second.ok, "{second:?}");

    let content = content_of(&db, &board);
    assert_eq!(content.annotations.len(), 1);
    assert_eq!(content.annotations[0].comment.as_deref(), Some("better"));
    assert_eq!(content.annotations[0].nag, None);
}

#[test]
fn write_tools_annotate_needs_a_nag_or_a_comment() {
    let (db, board) = setup();
    let mut host = BoardWriteHost::new(&db, "test");

    let result = host.call(&call("annotate", edit_args(&board, serde_json::json!({"ply": 1}))));

    assert!(!result.ok);
    assert_eq!(result.code, result_code::INVALID_ARGUMENTS);
}

#[test]
fn write_tools_write_note_replaces_the_markdown() {
    let (db, board) = setup();
    let mut host = BoardWriteHost::new(&db, "test");

    let result = host.call(&call(
        "write_note",
        edit_args(&board, serde_json::json!({"markdown": "# Plan\n\nPlay e4."})),
    ));

    assert!(result.ok, "{result:?}");
    assert_eq!(content_of(&db, &board).notes_md, "# Plan\n\nPlay e4.");
}

#[test]
fn write_tools_preserve_content_fields_this_build_does_not_model() {
    let (db, board) = setup();
    // A newer client stored a tree this build knows nothing about.
    db.update_board_content(
        &board.id,
        1,
        r#"{"moves":[],"tree":{"root":{"children":[]}},"clock":{"base":300}}"#,
        Some("client"),
    )
    .expect("seed content");
    let board = db.board(&board.id).expect("board").expect("row");
    let mut host = BoardWriteHost::new(&db, "test");

    let result = host.call(&call(
        "draw_circle",
        edit_args(&board, serde_json::json!({"square": "a1"})),
    ));
    assert!(result.ok, "{result:?}");

    let stored = db.board(&board.id).expect("board").expect("row");
    let value: serde_json::Value = serde_json::from_str(&stored.content_json).expect("json");
    assert_eq!(value["tree"]["root"]["children"], serde_json::json!([]));
    assert_eq!(value["clock"]["base"], serde_json::json!(300));
    assert_eq!(value["circles"][0]["square"], serde_json::json!("a1"));
}

// --- create and copy -------------------------------------------------------

#[test]
fn write_tools_create_and_copy_a_board_with_provenance() {
    let (db, board) = setup();
    let workspace_id = board.parent_id.clone();
    let mut host = BoardWriteHost::new(&db, "test");

    let created = host.call(&call(
        "create_board",
        serde_json::json!({
            "workspace_id": workspace_id,
            "title": "Endgame",
            "kind": "analysis",
        }),
    ));
    assert!(created.ok, "{created:?}");
    assert_eq!(created.revision, Some(1));
    let created_id = created.data.as_ref().expect("data")["board_id"]
        .as_str()
        .expect("board id")
        .to_string();

    let copied = host.call(&call(
        "copy_board",
        serde_json::json!({"board_id": created_id, "title": "Endgame copy"}),
    ));
    assert!(copied.ok, "{copied:?}");
    assert_eq!(
        copied.data.as_ref().expect("data")["copied_from"],
        serde_json::json!(created_id)
    );
    let copy_id = copied.data.as_ref().expect("data")["board_id"]
        .as_str()
        .expect("copy id")
        .to_string();
    assert_ne!(copy_id, created_id);

    let copy = db.board(&copy_id).expect("board").expect("row");
    assert!(copy.metadata_json.contains("copied_from"));
    assert_eq!(copy.revision, 1);
    assert_eq!(copy.mutability, "mutable");
}

#[test]
fn write_tools_lock_then_unlock_a_board() {
    let (db, board) = setup();
    let mut host = BoardWriteHost::new(&db, "test");

    let locked = host.call(&call(
        "lock_board",
        serde_json::json!({"board_id": board.id}),
    ));
    assert!(locked.ok, "{locked:?}");
    assert_eq!(content_of_mutability(&db, &board), "locked");

    let unlocked = host.call(&call(
        "unlock_board",
        serde_json::json!({"board_id": board.id}),
    ));
    assert!(unlocked.ok, "{unlocked:?}");
    assert_eq!(content_of_mutability(&db, &board), "mutable");
}

fn content_of_mutability(db: &Db, board: &Board) -> String {
    db.board(&board.id)
        .expect("board")
        .expect("row")
        .mutability
}

// --- reads and refusals ----------------------------------------------------

#[test]
fn write_tools_get_board_reports_revision_and_mutability() {
    let (db, board) = setup();
    let mut host = BoardWriteHost::new(&db, "test");

    let result = host.call(&call(
        "get_board",
        serde_json::json!({"board_id": board.id}),
    ));

    assert!(result.ok);
    let data = result.data.as_ref().expect("data");
    assert_eq!(data["revision"], serde_json::json!(1));
    assert_eq!(data["mutability"], serde_json::json!("mutable"));
    assert_eq!(data["start_fen"], serde_json::json!(START));
}

#[test]
fn write_tools_refuse_an_unknown_board_and_an_unknown_tool() {
    let (db, _board) = setup();
    let mut host = BoardWriteHost::new(&db, "test");

    let missing = host.call(&call(
        "get_board",
        serde_json::json!({"board_id": "no-such-board"}),
    ));
    assert_eq!(missing.code, result_code::NOT_FOUND);

    let unknown = host.call(&call("summon_dragon", serde_json::json!({})));
    assert_eq!(unknown.code, result_code::NOT_FOUND);
    assert!(unknown.summary.contains("summon_dragon"));
}

#[test]
fn write_tools_create_a_study_in_draft_state() {
    let (db, _board) = setup();
    let mut host = BoardWriteHost::new(&db, "test");

    let result = host.call(&call(
        "create_study",
        serde_json::json!({"title": "Sicilian", "description": "Najdorf"}),
    ));

    assert!(result.ok, "{result:?}");
    let study_id = result.data.as_ref().expect("data")["study_id"]
        .as_str()
        .expect("study id")
        .to_string();
    let study = db.study(&study_id).expect("study").expect("row");
    assert_eq!(study.lifecycle, "draft");
    assert_eq!(study.title, "Sicilian");
}

#[test]
fn write_tools_read_back_the_board_content_after_a_move() {
    let (db, board) = setup();
    let mut host = BoardWriteHost::new(&db, "test");
    host.call(&call("play_move", move_args(&board, "e4")));

    let board = db.board(&board.id).expect("board").expect("row");
    let content = BoardContent::parse(&board.content_json).expect("content");
    assert_eq!(content.moves, vec!["e4"]);
    // The FEN in the write result is what the content replays to.
    assert_eq!(
        content.fen(&board.start_fen).expect("fen"),
        "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1"
    );
}
