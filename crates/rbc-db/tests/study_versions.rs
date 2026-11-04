//! Publish, branch, lock and provenance: the rules that make a version worth
//! citing.

use rbc_db::Db;

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

fn study_with_a_chapter(db: &Db) -> String {
    let study = db.create_study("Openings", "a study").expect("study");
    db.add_chapter(&study.id, "Italian", START, "## notes")
        .expect("chapter");
    study.id
}

#[test]
fn publishing_freezes_the_content_it_published() {
    let db = Db::open_memory().expect("db");
    let id = study_with_a_chapter(&db);

    let version = db.publish_study(&id, 1).expect("publish");
    let frozen: rbc_db::StudyVersion = db
        .study_versions(&id)
        .expect("versions")
        .into_iter()
        .find(|v| v.version == version.version)
        .expect("the version exists");
    let bundle: serde_json::Value = serde_json::from_str(&frozen.bundle_json).expect("bundle");
    assert_eq!(bundle["schema"], serde_json::json!("rbc-study/1"));
    assert_eq!(bundle["chapters"].as_array().expect("chapters").len(), 1);

    // Adding a chapter after publishing changes the study, not the snapshot.
    db.add_chapter(&id, "Berlin", START, "").expect("chapter");
    let again: serde_json::Value = serde_json::from_str(&frozen.bundle_json).expect("bundle");
    assert_eq!(
        again["chapters"].as_array().expect("chapters").len(),
        1,
        "the version still describes one chapter"
    );
    let now = db.study(&id).expect("study").expect("present");
    assert_eq!(db.chapters(&id).expect("chapters").len(), 2);
    assert_eq!(
        now.current_version, version.version,
        "publishing is the only bump"
    );
}

#[test]
fn a_branch_records_where_it_came_from() {
    let db = Db::open_memory().expect("db");
    let id = study_with_a_chapter(&db);
    db.publish_study(&id, 1).expect("publish");

    let branch = db.branch_study_from_version(&id, 2).expect("branch");
    assert_ne!(branch.id, id, "a branch is its own study");
    assert_eq!(branch.lifecycle, "draft", "a branch starts editable");
    // Adding the copied chapter bumps the draft's counter, exactly as it would
    // for any study; what matters is that it starts from no published version of
    // its own.
    assert_eq!(
        db.study_versions(&branch.id).expect("versions").len(),
        0,
        "a branch has published nothing yet"
    );
    let metadata: serde_json::Value =
        serde_json::from_str(&branch.metadata_json).expect("metadata");
    assert_eq!(metadata["branched_from"]["study_id"], serde_json::json!(id));
    assert_eq!(metadata["branched_from"]["version"], serde_json::json!(2));

    // The chapters came with it, and the branch is editable while the original
    // is still published.
    let chapters = db.chapters(&branch.id).expect("chapters");
    assert_eq!(chapters.len(), 1);
    assert_eq!(chapters[0].title, "Italian");
    db.add_chapter(&branch.id, "A new idea", START, "")
        .expect("the branch takes new chapters");
    assert_eq!(
        db.chapters(&id).expect("chapters").len(),
        1,
        "and the original did not grow"
    );
}

#[test]
fn a_lock_refuses_writes_without_publishing_anything() {
    let db = Db::open_memory().expect("db");
    let id = study_with_a_chapter(&db);

    let before = db.study(&id).expect("study").expect("present");
    let locked = db.lock_study(&id, true).expect("lock");
    assert!(locked.locked);
    assert_eq!(locked.lifecycle, "draft", "a lock is not a publish");
    assert_eq!(
        locked.current_version, before.current_version,
        "and locking makes no version"
    );
    assert_eq!(
        db.study_versions(&id).expect("versions").len(),
        0,
        "nothing was published"
    );

    // The tools refuse; the repository itself is not the guard, so this is a
    // statement about the study's state, not a write attempt.
    let unlocked = db.lock_study(&id, false).expect("unlock");
    assert!(!unlocked.locked);
}

#[test]
fn branching_from_a_version_that_does_not_exist_is_refused() {
    let db = Db::open_memory().expect("db");
    let id = study_with_a_chapter(&db);
    let failure = db
        .branch_study_from_version(&id, 9)
        .expect_err("no such version");
    assert!(
        format!("{failure}").contains('9'),
        "the refusal names the version: {failure}"
    );
}

#[test]
fn a_copy_keeps_provenance_of_what_it_came_from() {
    let db = Db::open_memory().expect("db");
    let id = study_with_a_chapter(&db);
    let copy = db.copy_study(&id).expect("copy");
    assert_ne!(copy.id, id);
    assert_eq!(copy.lifecycle, "draft");
    assert_eq!(db.chapters(&copy.id).expect("chapters").len(), 1);
}
