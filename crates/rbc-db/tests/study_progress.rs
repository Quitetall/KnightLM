//! Progress: per user, per chapter, and separate from the study's content.

use rbc_db::Db;

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

fn study_with_two_chapters(db: &Db) -> (String, String, String) {
    let study = db.create_study("Openings", "").expect("study");
    let (one, _) = db.add_chapter(&study.id, "One", START, "").expect("one");
    let (two, _) = db.add_chapter(&study.id, "Two", START, "").expect("two");
    (study.id, one.id, two.id)
}

#[test]
fn progress_is_per_user_and_does_not_leak_between_them() {
    let db = Db::open_memory().expect("db");
    let (study, one, _) = study_with_two_chapters(&db);

    db.set_study_progress(&study, &one, "", "alex", true, 1, None, 0, None)
        .expect("alex");
    db.set_study_progress(&study, &one, "", "sam", false, 3, Some(0.5), 1, Some(0.2))
        .expect("sam");

    let alex = db.study_progress(&study, "alex").expect("alex rows");
    let sam = db.study_progress(&study, "sam").expect("sam rows");
    let nobody = db.study_progress(&study, "nobody").expect("none");

    assert_eq!(alex.len(), 1);
    assert!(alex[0].completed, "alex finished it");
    assert_eq!(alex[0].attempts, 1);

    assert_eq!(sam.len(), 1);
    assert!(!sam[0].completed, "sam has not");
    assert_eq!(sam[0].attempts, 3, "and has his own attempt count");
    assert_eq!(sam[0].score, Some(0.5));

    assert!(
        nobody.is_empty(),
        "a user who has never opened it sees no progress"
    );
}

#[test]
fn progress_keeps_the_reserved_fields_for_later() {
    // Score, hints and mastery are recorded now and unused by the UI: a schema
    // that records them cannot be retrofitted without a migration.
    let db = Db::open_memory().expect("db");
    let (study, one, _) = study_with_two_chapters(&db);
    db.set_study_progress(&study, &one, "", "you", false, 4, Some(0.75), 2, Some(0.4))
        .expect("progress");
    let rows = db.study_progress(&study, "you").expect("rows");
    assert_eq!(rows[0].attempts, 4);
    assert_eq!(rows[0].score, Some(0.75));
    assert_eq!(rows[0].hints, 2);
    assert_eq!(rows[0].mastery, Some(0.4));
}

#[test]
fn progress_changes_nothing_about_the_study_or_its_versions() {
    // Progress is a reader's state, not the document: a published bundle must not
    // change because someone ticked a chapter.
    let db = Db::open_memory().expect("db");
    let (study, one, _) = study_with_two_chapters(&db);
    db.publish_study(&study, 1).expect("publish");
    let before = db.study(&study).expect("study").expect("present");
    let bundle_before = db
        .study_versions(&study)
        .expect("versions")
        .into_iter()
        .next()
        .expect("a version")
        .bundle_json;

    db.set_study_progress(&study, &one, "", "you", true, 1, None, 0, None)
        .expect("progress");

    let after = db.study(&study).expect("study").expect("present");
    let bundle_after = db
        .study_versions(&study)
        .expect("versions")
        .into_iter()
        .next()
        .expect("a version")
        .bundle_json;
    assert_eq!(before.updated, after.updated, "the study was not touched");
    assert_eq!(
        before.current_version, after.current_version,
        "and no version was made"
    );
    assert_eq!(
        bundle_before, bundle_after,
        "a published bundle is not affected by reading"
    );
    assert!(
        !bundle_after.contains("completed"),
        "progress is not part of the document"
    );
}

#[test]
fn progress_is_per_study_even_for_the_same_chapter_title() {
    let db = Db::open_memory().expect("db");
    let first = study_with_two_chapters(&db);
    let second = study_with_two_chapters(&db);
    db.set_study_progress(&first.0, &first.1, "", "you", true, 1, None, 0, None)
        .expect("first");
    let other = db.study_progress(&second.0, "you").expect("second");
    assert!(
        other.is_empty(),
        "one study's progress says nothing about another's"
    );
}
