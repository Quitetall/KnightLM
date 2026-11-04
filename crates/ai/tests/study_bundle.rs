//! `.rbcstudy` round-trips: export, parse, write back, and what happens to
//! fields this build has never seen.
#![cfg(feature = "host")]

use rbc_ai::study_bundle::{self, SCHEMA};
use rbc_db::Db;

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

fn study_with_content(db: &Db) -> String {
    let study = db
        .create_study("Ruy Lopez", "the main lines")
        .expect("study");
    // A chapter comes with a board of its own; that is the one a study usually
    // authors, so the fixture uses it rather than adding a second.
    let (_chapter, board) = db
        .add_chapter(&study.id, "Morphy", START, "## notes")
        .expect("chapter");
    db.update_board_content(
        &board.id,
        1,
        r#"{"moves":["e4","e5","Nf3"],"arrows":[{"from":"e2","to":"e4"}],"circles":[],"notes_md":"a note"}"#,
        Some("test"),
    )
    .expect("content");
    study.id
}

#[test]
fn a_study_exports_as_the_schema_the_design_fixes() {
    let db = Db::open_memory().expect("db");
    let id = study_with_content(&db);
    let bundle = study_bundle::export(&db, &id).expect("export");

    assert_eq!(bundle["schema"], serde_json::json!(SCHEMA));
    assert_eq!(bundle["title"], serde_json::json!("Ruy Lopez"));
    assert_eq!(bundle["lua"], serde_json::Value::Null, "Lua is R2");
    let chapters = bundle["chapters"].as_array().expect("chapters");
    assert_eq!(chapters.len(), 1);
    assert_eq!(chapters[0]["title"], serde_json::json!("Morphy"));
    assert_eq!(chapters[0]["notes_md"], serde_json::json!("## notes"));
    let boards = chapters[0]["boards"].as_array().expect("boards");
    assert_eq!(boards.len(), 1);
    assert_eq!(boards[0]["fen"], serde_json::json!(START));
    assert_eq!(boards[0]["notes_md"], serde_json::json!("a note"));
    let pgn = boards[0]["pgn"].as_str().expect("pgn");
    assert!(pgn.contains("1. e4 e5 2. Nf3"), "{pgn}");
    assert!(pgn.contains("[Round \"Morphy\"]"), "{pgn}");
}

#[test]
fn export_then_parse_then_write_is_lossless_for_what_we_know() {
    let db = Db::open_memory().expect("db");
    let id = study_with_content(&db);
    let exported = study_bundle::export(&db, &id).expect("export");
    let parsed = study_bundle::parse(&exported).expect("parse");
    let written = study_bundle::to_json(&parsed);

    assert_eq!(parsed.title, "Ruy Lopez");
    assert_eq!(parsed.chapters.len(), 1);
    assert_eq!(parsed.chapters[0].boards.len(), 1);
    // The two documents describe the same study.
    assert_eq!(written["title"], exported["title"]);
    assert_eq!(
        written["chapters"][0]["boards"][0]["pgn"],
        exported["chapters"][0]["boards"][0]["pgn"]
    );
    assert_eq!(
        written["chapters"][0]["notes_md"],
        exported["chapters"][0]["notes_md"]
    );
}

#[test]
fn a_field_from_a_newer_build_survives_a_round_trip() {
    // An old build that dropped unknown fields would corrupt a newer bundle by
    // editing it; keeping them is the whole point of `other`.
    let db = Db::open_memory().expect("db");
    let id = study_with_content(&db);
    let mut bundle = study_bundle::export(&db, &id).expect("export");
    bundle["future_field"] = serde_json::json!({"kept": true});
    bundle["chapters"][0]["future_chapter_field"] = serde_json::json!(7);
    bundle["chapters"][0]["boards"][0]["future_board_field"] = serde_json::json!(["a"]);

    let parsed = study_bundle::parse(&bundle).expect("parse");
    assert_eq!(
        parsed.other["future_field"],
        serde_json::json!({"kept": true})
    );
    assert_eq!(
        parsed.chapters[0].other["future_chapter_field"],
        serde_json::json!(7)
    );
    assert_eq!(
        parsed.chapters[0].boards[0].other["future_board_field"],
        serde_json::json!(["a"])
    );

    let written = study_bundle::to_json(&parsed);
    assert_eq!(written["future_field"], serde_json::json!({"kept": true}));
    assert_eq!(
        written["chapters"][0]["future_chapter_field"],
        serde_json::json!(7)
    );
    assert_eq!(
        written["chapters"][0]["boards"][0]["future_board_field"],
        serde_json::json!(["a"])
    );
}

#[test]
fn something_that_is_not_a_bundle_is_refused() {
    let not_a_bundle = serde_json::json!({"hello": "world"});
    let failure = study_bundle::parse(&not_a_bundle).expect_err("no schema");
    assert!(format!("{failure}").contains("schema"), "{failure}");

    let wrong = serde_json::json!({"schema": "rbc-tournament/1"});
    let failure = study_bundle::parse(&wrong).expect_err("wrong kind");
    assert!(
        format!("{failure}").contains("not a study bundle"),
        "{failure}"
    );
}

#[test]
fn lineage_travels_in_the_bundle() {
    let db = Db::open_memory().expect("db");
    let id = study_with_content(&db);
    db.publish_study(&id, 1).expect("publish");
    let branch = db.branch_study_from_version(&id, 2).expect("branch");

    let bundle = study_bundle::export(&db, &branch.id).expect("export");
    let parsed = study_bundle::parse(&bundle).expect("parse");
    let lineage = parsed.lineage.expect("lineage");
    assert_eq!(lineage.from, id);
    assert_eq!(lineage.version, 2);
}

#[test]
fn an_imported_bundle_creates_a_study_that_can_be_exported_again() {
    // The other half of a format: a bundle from another installation becomes a
    // study here, and survives the trip back out.
    let source = Db::open_memory().expect("source");
    let id = study_with_content(&source);
    let bundle = study_bundle::export(&source, &id).expect("export");

    // Import into a different database, as a person moving a study would.
    let target = Db::open_memory().expect("target");
    let parsed = study_bundle::parse(&bundle).expect("parse");
    let study = target
        .create_study(&parsed.title, &parsed.description_md)
        .expect("study");
    for chapter in &parsed.chapters {
        let (added, _) = target
            .add_chapter(&study.id, &chapter.title, START, &chapter.notes_md)
            .expect("chapter");
        for board in &chapter.boards {
            let moves: Vec<String> = board
                .pgn
                .lines()
                .filter(|line| !line.starts_with('[') && !line.trim().is_empty())
                .flat_map(|line| line.split_whitespace())
                .filter(|token| {
                    !token.chars().all(|c| c.is_ascii_digit() || c == '.')
                        && !matches!(*token, "1-0" | "0-1" | "1/2-1/2" | "*")
                })
                .map(str::to_string)
                .collect();
            let content = serde_json::json!({"moves": moves, "notes_md": board.notes_md});
            let created = target
                .add_board_to_chapter(&study.id, &added.id, "Imported", START, "{}")
                .expect("board");
            target
                .update_board_content(&created.id, 1, &content.to_string(), Some("import"))
                .expect("content");
        }
    }

    let again = study_bundle::export(&target, &study.id).expect("re-export");
    assert_eq!(again["title"], bundle["title"]);
    assert_eq!(
        again["chapters"][0]["notes_md"],
        bundle["chapters"][0]["notes_md"]
    );
    // The moves came across, even though they were carried as PGN text. A
    // chapter created by `add_chapter` brings an empty board of its own, so look
    // for the imported one rather than assuming it is first.
    let boards = again["chapters"][0]["boards"].as_array().expect("boards");
    // Movetext carries move numbers, so compare the SAN tokens rather than a
    // substring: "1. e4 e5 2. Nf3" does not contain "e4 e5 Nf3".
    let san_of = |pgn: &str| -> Vec<String> {
        pgn.lines()
            .filter(|line| !line.starts_with('['))
            .flat_map(|line| line.split_whitespace())
            .filter(|token| !token.chars().all(|c| c.is_ascii_digit() || c == '.'))
            .filter(|token| !matches!(*token, "1-0" | "0-1" | "1/2-1/2" | "*"))
            .map(str::to_string)
            .collect()
    };
    assert!(
        boards
            .iter()
            .any(|board| san_of(board["pgn"].as_str().unwrap_or_default())
                == vec!["e4", "e5", "Nf3"]),
        "an imported board keeps its moves: {boards:?}"
    );
}
