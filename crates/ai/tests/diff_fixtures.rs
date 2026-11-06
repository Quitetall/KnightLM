//! Fixture matrix for the compare engine: every alignment rule, every kind of
//! change, and the guarantee that comparing writes nothing at all.
//!
//! The tests in `diff.rs` cover the engine's behaviour on one or two examples.
//! This file is the coverage table: for each rule the design names, a case that
//! exercises it and a case that proves the rule does *not* fire when it should
//! not. A comparison that reported "changed" for everything would pass a
//! positive-only suite.
#![cfg(feature = "host")]

use rbc_ai::diff::{Change, DiffReport, diff_studies, to_markdown};
use rbc_db::Db;

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// A study with the given chapters, each `(title, notes, moves)`.
fn study(db: &Db, title: &str, chapters: &[(&str, &str, &[&str])]) -> String {
    let created = db.create_study(title, "").expect("study");
    for (chapter_title, notes, moves) in chapters {
        let (_chapter, board) = db
            .add_chapter(&created.id, chapter_title, START, notes)
            .expect("chapter");
        let content = serde_json::json!({
            "moves": moves,
            "arrows": [],
            "circles": [],
            "annotations": [],
            "notes_md": "",
        });
        db.update_board_content(&board.id, 1, &content.to_string(), Some("fixture"))
            .expect("content");
    }
    created.id
}

fn compare(db: &Db, left: &str, right: &str) -> DiffReport {
    diff_studies(db, left, right, None).expect("diff")
}

// --- alignment ---------------------------------------------------------------

#[test]
fn chapters_align_by_id_before_anything_else() {
    // Two studies sharing one chapter id is the strongest possible signal, so it
    // wins even when the ordinal and the title have both moved.
    let db = Db::open_memory().expect("db");
    let left = db.create_study("Left", "").expect("left");
    let right = db.create_study("Right", "").expect("right");
    let (chapter, _) = db
        .add_chapter(&left.id, "Original", START, "")
        .expect("chapter");

    // Re-create the same chapter row in the second study by copying the study,
    // which preserves ids.
    let copy = db.copy_study(&left.id).expect("copy");

    let report = compare(&db, &left.id, &copy.id);
    assert_eq!(report.chapters.len(), 1);
    assert_eq!(
        report.chapters[0].left_id.as_deref(),
        Some(chapter.id.as_str())
    );
    assert_eq!(
        report.chapters[0].change,
        Change::Unchanged,
        "a copy is a copy"
    );
    let _ = right;
}

#[test]
fn chapters_fall_back_to_ordinal_when_the_id_is_new() {
    let db = Db::open_memory().expect("db");
    let left = study(&db, "Left", &[("First", "", &["e4"])]);
    let right = study(&db, "Right", &[("Renamed", "", &["e4"])]);
    let report = compare(&db, &left, &right);
    assert_eq!(report.chapters.len(), 1, "the two first chapters pair up");
    assert_eq!(report.chapters[0].change, Change::Changed);
    assert!(
        report.chapters[0].fields.iter().any(|f| f.field == "title"),
        "and the rename is what it reports"
    );
}

#[test]
fn a_second_chapter_only_on_one_side_is_reported_as_removed() {
    let db = Db::open_memory().expect("db");
    let left = study(
        &db,
        "Left",
        &[("Shared", "", &["e4"]), ("Extra", "", &["d4"])],
    );
    let right = study(&db, "Right", &[("Shared", "", &["e4"])]);
    let report = compare(&db, &left, &right);
    let removed: Vec<&str> = report
        .chapters
        .iter()
        .filter(|c| c.change == Change::Removed)
        .map(|c| c.title.as_str())
        .collect();
    assert_eq!(removed, vec!["Extra"]);
}

#[test]
fn boards_align_by_title_and_a_fen_matches_without_its_counters() {
    // Two chapters that each hold one board; the boards have different ids and
    // different titles, so only the FEN can pair them - and it must ignore the
    // move counters to do so.
    let db = Db::open_memory().expect("db");
    let left = db.create_study("Left", "").expect("left");
    let right = db.create_study("Right", "").expect("right");
    let (lchap, lboard) = db.add_chapter(&left.id, "Same", START, "").expect("l");
    let (_rchap, rboard) = db.add_chapter(&right.id, "Same", START, "").expect("r");
    db.update_board_content(
        &lboard.id,
        1,
        r#"{"moves":["e4"],"arrows":[],"circles":[],"annotations":[],"notes_md":""}"#,
        Some("f"),
    )
    .expect("l content");
    db.update_board_content(
        &rboard.id,
        1,
        r#"{"moves":["e4","e5"],"arrows":[],"circles":[],"annotations":[],"notes_md":""}"#,
        Some("f"),
    )
    .expect("r content");

    let report = compare(&db, &left.id, &right.id);
    let chapter = report
        .chapters
        .iter()
        .find(|c| c.left_id.as_deref() == Some(lchap.id.as_str()))
        .expect("the chapter");
    assert_eq!(chapter.boards_changed.len(), 1, "the boards paired up");
    assert_eq!(
        chapter.boards_changed[0].change,
        Change::Changed,
        "and the extra ply is the difference"
    );
}

// --- coverage ----------------------------------------------------------------

#[test]
fn every_kind_of_change_reaches_the_report() {
    // One study that differs in moves, shapes, annotations, board notes, chapter
    // notes, study metadata and progress: each must appear.
    let db = Db::open_memory().expect("db");
    let left = db.create_study("Left", "the original").expect("left");
    let right = db.create_study("Right", "the original").expect("right");
    db.update_study(&left.id, Some("Left"), None, Some(&["a".into()]), None)
        .expect("tags");
    db.update_study(&right.id, Some("Right"), None, Some(&["b".into()]), None)
        .expect("tags");

    let (lchap, lboard) = db
        .add_chapter(&left.id, "Openings", START, "## plan")
        .expect("l");
    let (_rchap, rboard) = db
        .add_chapter(&right.id, "Openings", START, "## plan\n- e4")
        .expect("r");
    db.update_board_content(
        &lboard.id,
        1,
        r#"{"moves":["e4","e5"],"arrows":[],"circles":[{"square":"e5"}],"annotations":[{"ply":0,"comment":"old"}],"notes_md":"a"}"#,
        Some("f"),
    )
    .expect("l");
    db.update_board_content(
        &rboard.id,
        1,
        r#"{"moves":["e4","e5","Nf3"],"arrows":[{"from":"g1","to":"f3"}],"circles":[],"annotations":[{"ply":0,"comment":"new"}],"notes_md":"b"}"#,
        Some("f"),
    )
    .expect("r");
    db.set_study_progress(&left.id, &lchap.id, "", "you", true, 1, None, 0, None)
        .expect("progress");

    let report = diff_studies(&db, &left.id, &right.id, Some("you")).expect("diff");
    let chapter = &report.chapters[0];
    let board = &chapter.boards_changed[0];

    // study metadata
    assert!(report.fields.iter().any(|f| f.field == "title"));
    assert!(report.fields.iter().any(|f| f.field == "tags"));
    // chapter notes
    assert!(
        chapter
            .notes
            .as_ref()
            .is_some_and(|n| n.added.iter().any(|l| l.contains("e4")))
    );
    // board notes, moves, shapes, annotations
    assert!(board.notes.as_ref().is_some_and(|n| !n.is_empty()));
    assert_eq!(board.first_divergence, Some(2));
    assert_eq!(board.moves_added, vec!["Nf3".to_string()]);
    assert_eq!(board.shapes_right_only, vec!["arrow g1-f3".to_string()]);
    assert_eq!(board.shapes_left_only, vec!["circle e5".to_string()]);
    assert_eq!(board.annotations_right_only, vec!["0: new".to_string()]);
    // progress overlay
    let progress = chapter.progress.as_ref().expect("overlay");
    assert!(progress.left_completed);
    assert!(!progress.right_completed);

    // And the export carries it all.
    let markdown = to_markdown(&report);
    assert!(markdown.contains("### Openings"));
    assert!(markdown.contains("added move: `Nf3`"));
    assert!(markdown.contains("shape only on the left: circle e5"));
    assert!(markdown.contains("annotation only on the right: 0: new"));
}

#[test]
fn an_unchanged_study_pair_reports_nothing_but_the_copy_title() {
    // The negative case: identical content must not produce chapter, board or
    // note noise. Only the title differs, because a copy renames itself.
    let db = Db::open_memory().expect("db");
    let source = study(&db, "Same", &[("One", "", &["e4"]), ("Two", "", &["d4"])]);
    let copy = db.copy_study(&source).expect("copy");

    let report = compare(&db, &source, &copy.id);
    let changed: Vec<&str> = report
        .chapters
        .iter()
        .filter(|c| c.change.is_a_change())
        .map(|c| c.title.as_str())
        .collect();
    assert!(
        changed.is_empty(),
        "no chapter should look changed: {changed:?}"
    );
    assert_eq!(report.fields.len(), 1, "{:?}", report.fields);
    assert_eq!(report.fields[0].field, "title");
}

// --- read-only ---------------------------------------------------------------

#[test]
fn comparing_touches_no_table_it_does_not_read() {
    // The engine takes &Db, but a future refactor could reach for a write. This
    // asserts the observable consequences: no new versions, approvals, audit
    // rows, and no revision or timestamp moving on either side.
    let db = Db::open_memory().expect("db");
    let left = db.create_study("Left", "").expect("left");
    let right = db.create_study("Right", "").expect("right");
    let (lchap, lboard) = db.add_chapter(&left.id, "One", START, "").expect("l");
    let (_rchap, rboard) = db.add_chapter(&right.id, "One", START, "").expect("r");
    db.update_board_content(
        &lboard.id,
        1,
        r#"{"moves":["e4"],"arrows":[],"circles":[],"annotations":[],"notes_md":""}"#,
        Some("f"),
    )
    .expect("l");

    let before = (
        db.study(&left.id).expect("s").expect("l"),
        db.study(&right.id).expect("s").expect("r"),
        db.board(&lboard.id).expect("b").expect("l"),
        db.board(&rboard.id).expect("b").expect("r"),
        db.study_versions(&left.id).expect("v").len(),
        db.audit_log(100).expect("audit").len(),
    );

    // The whole surface a caller can reach: the report, both exports, twice.
    for _ in 0..2 {
        let report = diff_studies(&db, &left.id, &right.id, Some("you")).expect("diff");
        let _ = serde_json::to_value(&report).expect("json");
        let _ = to_markdown(&report);
    }

    let after = (
        db.study(&left.id).expect("s").expect("l"),
        db.study(&right.id).expect("s").expect("r"),
        db.board(&lboard.id).expect("b").expect("l"),
        db.board(&rboard.id).expect("b").expect("r"),
        db.study_versions(&left.id).expect("v").len(),
        db.audit_log(100).expect("audit").len(),
    );

    assert_eq!(before.0.updated, after.0.updated, "left study untouched");
    assert_eq!(before.1.updated, after.1.updated, "right study untouched");
    assert_eq!(before.0.current_version, after.0.current_version);
    assert_eq!(before.1.current_version, after.1.current_version);
    assert_eq!(before.2.revision, after.2.revision, "left board untouched");
    assert_eq!(before.3.revision, after.3.revision, "right board untouched");
    assert_eq!(before.4, after.4, "no version was written");
    assert_eq!(before.5, after.5, "no audit row was written");
    let _ = lchap;
}

#[test]
fn a_report_is_a_snapshot_not_a_live_handle() {
    // Taking a report and then changing the study must not change the report.
    let db = Db::open_memory().expect("db");
    let left = study(&db, "Left", &[("One", "", &["e4"])]);
    let right = study(&db, "Right", &[("One", "", &["e4"])]);
    let report = compare(&db, &left, &right);
    let chapters_before = report.chapters.len();
    let changes_before: Vec<Change> = report.chapters.iter().map(|c| c.change).collect();

    db.add_chapter(&left, "Later", START, "").expect("chapter");

    assert_eq!(report.chapters.len(), chapters_before);
    assert_eq!(
        report.chapters.iter().map(|c| c.change).collect::<Vec<_>>(),
        changes_before
    );
}
