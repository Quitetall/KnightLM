//! The structural diff: alignment, what counts as a change, and that comparing
//! never writes.
#![cfg(feature = "host")]

use rbc_ai::diff::{Change, diff_studies, line_diff, normalize_fen};
use rbc_db::Db;

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// Two studies that share a chapter and differ inside it.
fn pair(db: &Db) -> (String, String, String, String) {
    let left = db.create_study("Left", "the original").expect("left");
    let right = db.create_study("Right", "the original").expect("right");
    let (left_chapter, left_board) = db
        .add_chapter(&left.id, "Openings", START, "## plan")
        .expect("left chapter");
    let (right_chapter, right_board) = db
        .add_chapter(&right.id, "Openings", START, "## plan")
        .expect("right chapter");
    db.update_board_content(
        &left_board.id,
        1,
        r#"{"moves":["e4","e5"],"arrows":[],"circles":[],"annotations":[],"notes_md":""}"#,
        Some("test"),
    )
    .expect("left content");
    db.update_board_content(
        &right_board.id,
        1,
        r#"{"moves":["e4","e5","Nf3"],"arrows":[{"from":"g1","to":"f3"}],"circles":[],"annotations":[{"ply":2,"comment":"develops"}],"notes_md":""}"#,
        Some("test"),
    )
    .expect("right content");
    (left.id, right.id, left_chapter.id, right_chapter.id)
}

#[test]
fn two_identically_built_studies_are_identical_apart_from_the_study_fields() {
    let db = Db::open_memory().expect("db");
    let source = db.create_study("Same", "same").expect("study");
    db.add_chapter(&source.id, "One", START, "")
        .expect("chapter");
    let copy = db.copy_study(&source.id).expect("copy");

    let report = diff_studies(&db, &source.id, &copy.id, None).expect("diff");
    // The title differs (a copy says so), so the report has that one field and
    // no chapter-level noise.
    assert_eq!(report.fields.len(), 1, "{:?}", report.fields);
    assert_eq!(report.fields[0].field, "title");
    assert!(!report.identical, "a differing title is a difference");
}

#[test]
fn a_chapter_that_kept_its_title_lines_up_and_reports_its_board_change() {
    let db = Db::open_memory().expect("db");
    let (left, right, _, _) = pair(&db);
    let report = diff_studies(&db, &left, &right, None).expect("diff");

    assert_eq!(
        report.chapters.len(),
        1,
        "one chapter on each side lines up"
    );
    let chapter = &report.chapters[0];
    assert_eq!(chapter.title, "Openings");
    assert_eq!(chapter.change, Change::Changed);
    let board = &chapter.boards_changed[0];
    assert_eq!(board.change, Change::Changed);
    assert_eq!(
        board.first_divergence,
        Some(2),
        "the trees agree for two plies"
    );
    assert_eq!(board.moves_added, vec!["Nf3".to_string()]);
    assert!(
        board.moves_removed.is_empty(),
        "the right side only added a move"
    );
    assert_eq!(board.shapes_right_only, vec!["arrow g1-f3".to_string()]);
    assert_eq!(
        board.annotations_right_only,
        vec!["2: develops".to_string()]
    );
}

#[test]
fn chapters_at_the_same_ordinal_align_even_when_everything_else_differs() {
    // The documented rule is "by id, then ordinal, then title", so two chapters
    // that both come first pair up and the title change is the report. That is
    // deliberate: a renamed chapter keeps its place, and saying "renamed from X
    // to Y" is more useful than "one removed, one added".
    let db = Db::open_memory().expect("db");
    let left = db.create_study("Left", "").expect("left");
    let right = db.create_study("Right", "").expect("right");
    db.add_chapter(&left.id, "Only left", START, "")
        .expect("left chapter");
    db.add_chapter(&right.id, "Only right", START, "")
        .expect("right chapter");

    let report = diff_studies(&db, &left.id, &right.id, None).expect("diff");
    assert_eq!(report.chapters.len(), 1, "the two first chapters align");
    assert_eq!(report.chapters[0].change, Change::Changed);
    assert!(
        report.chapters[0]
            .fields
            .iter()
            .any(|field| field.field == "title" && field.left == "Only left"),
        "and the rename is what the chapter reports"
    );
}

#[test]
fn a_chapter_only_on_one_side_is_added_or_removed() {
    // A second chapter on one side has no counterpart by id, ordinal or title.
    let db = Db::open_memory().expect("db");
    let left = db.create_study("Left", "").expect("left");
    let right = db.create_study("Right", "").expect("right");
    db.add_chapter(&left.id, "Shared", START, "")
        .expect("shared");
    db.add_chapter(&left.id, "Extra", START, "").expect("extra");
    db.add_chapter(&right.id, "Shared", START, "")
        .expect("shared right");

    let report = diff_studies(&db, &left.id, &right.id, None).expect("diff");
    // "Extra" exists only in the left study, so from the left's point of view it
    // was removed; the right side has nothing there.
    let removed: Vec<&str> = report
        .chapters
        .iter()
        .filter(|chapter| chapter.change == Change::Removed)
        .map(|chapter| chapter.title.as_str())
        .collect();
    assert_eq!(
        removed,
        vec!["Extra"],
        "only the extra chapter is one-sided"
    );
}

#[test]
fn notes_are_compared_by_line() {
    let diff = line_diff("## plan\n- e4\n- d4", "## plan\n- e4\n- c4");
    assert_eq!(diff.removed, vec!["- d4".to_string()]);
    assert_eq!(diff.added, vec!["- c4".to_string()]);
    assert!(!diff.is_empty());

    // An empty diff is what "no change" looks like.
    assert!(line_diff("same", "same").is_empty());
}

#[test]
fn a_fen_is_the_same_position_without_its_counters() {
    let a = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    let b = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 12 40";
    assert_eq!(normalize_fen(a), normalize_fen(b));
    let different = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
    assert_ne!(normalize_fen(a), normalize_fen(different));
}

#[test]
fn progress_travels_with_the_chapter_when_a_user_is_given() {
    let db = Db::open_memory().expect("db");
    let (left, right, left_chapter, right_chapter) = pair(&db);
    db.set_study_progress(&left, &left_chapter, "", "you", true, 1, None, 0, None)
        .expect("left progress");
    db.set_study_progress(
        &right,
        &right_chapter,
        "",
        "you",
        false,
        2,
        Some(0.5),
        0,
        None,
    )
    .expect("right progress");

    let report = diff_studies(&db, &left, &right, Some("you")).expect("diff");
    let progress = report.chapters[0].progress.as_ref().expect("overlay");
    assert!(progress.left_completed);
    assert!(!progress.right_completed);
    assert_eq!(progress.right_score, Some(0.5));
    assert_eq!(progress.user, "you");
}

#[test]
fn comparing_writes_nothing() {
    let db = Db::open_memory().expect("db");
    let (left, right, _, _) = pair(&db);
    let before_left = db.study(&left).expect("study").expect("left");
    let before_right = db.study(&right).expect("study").expect("right");
    let left_boards = db.boards("study_chapter", &before_left.id).expect("boards");
    let right_boards = db
        .boards("study_chapter", &before_right.id)
        .expect("boards");

    let report = diff_studies(&db, &left, &right, Some("you")).expect("diff");
    assert!(!report.identical);

    let after_left = db.study(&left).expect("study").expect("left");
    let after_right = db.study(&right).expect("study").expect("right");
    assert_eq!(before_left.updated, after_left.updated);
    assert_eq!(before_right.updated, after_right.updated);
    assert_eq!(before_left.current_version, after_left.current_version);
    assert_eq!(before_right.current_version, after_right.current_version);
    // And the boards kept their revisions.
    let after_left_boards = db.boards("study_chapter", &after_left.id).expect("boards");
    let _ = left_boards;
    let _ = right_boards;
    assert!(after_left_boards.iter().all(|board| board.revision == 1));
}
