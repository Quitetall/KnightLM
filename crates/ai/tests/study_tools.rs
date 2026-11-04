//! Study authoring over the tool host: create, add chapters and boards, reorder,
//! write text, publish — and the refusals that make publishing mean something.
#![cfg(feature = "host")]

use rbc_ai::agent::ToolHost;
use rbc_ai::board::BoardWriteHost;
use rbc_ai::protocol::{ToolCall, ToolResult, result_code};

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

fn call(host: &mut BoardWriteHost<'_>, name: &str, arguments: serde_json::Value) -> ToolResult {
    ToolHost::call(host, &ToolCall::new("c1", name, arguments))
}

fn data(result: &ToolResult) -> &serde_json::Value {
    result.data.as_ref().expect("a result carries data")
}

#[test]
fn a_study_is_authored_end_to_end_over_the_tools() {
    let db = rbc_db::Db::open_memory().expect("db");
    let mut host = BoardWriteHost::new(&db, "test");

    let created = call(
        &mut host,
        "create_study",
        serde_json::json!({"title": "Ruy Lopez", "description": "a study"}),
    );
    assert!(created.ok, "{}", created.summary);
    let study_id = data(&created)["study_id"].as_str().expect("id").to_string();

    let first = call(
        &mut host,
        "add_chapter",
        serde_json::json!({"study_id": study_id, "title": "Morphy", "start_fen": START}),
    );
    assert!(first.ok, "{}", first.summary);
    let first_chapter = data(&first)["chapter_id"]
        .as_str()
        .expect("chapter")
        .to_string();

    let second = call(
        &mut host,
        "add_chapter",
        serde_json::json!({"study_id": study_id, "title": "Berlin", "start_fen": START}),
    );
    assert!(second.ok, "{}", second.summary);

    // A board lands inside a chapter and comes back with its own revision.
    let board = call(
        &mut host,
        "add_board_to_chapter",
        serde_json::json!({
            "study_id": study_id, "chapter_id": first_chapter,
            "title": "Main line", "fen": START
        }),
    );
    assert!(board.ok, "{}", board.summary);
    assert_eq!(data(&board)["revision"], serde_json::json!(1));

    // Notes go on a chapter, and the description on the study.
    let notes = call(
        &mut host,
        "write_study_text",
        serde_json::json!({"study_id": study_id, "chapter_id": first_chapter, "markdown": "## Plan"}),
    );
    assert!(notes.ok, "{}", notes.summary);
    let chapters = db.chapters(&study_id).expect("chapters");
    assert_eq!(chapters[0].notes_md, "## Plan");

    let description = call(
        &mut host,
        "write_study_text",
        serde_json::json!({"study_id": study_id, "markdown": "An introduction"}),
    );
    assert!(description.ok, "{}", description.summary);
    assert_eq!(
        db.study(&study_id)
            .expect("study")
            .expect("present")
            .description_md,
        "An introduction"
    );

    // Reorder, and the order sticks.
    let ids: Vec<String> = chapters.iter().map(|c| c.id.clone()).collect();
    let reversed: Vec<String> = ids.iter().rev().cloned().collect();
    let reordered = call(
        &mut host,
        "reorder_chapters",
        serde_json::json!({"study_id": study_id, "chapter_ids": reversed}),
    );
    assert!(reordered.ok, "{}", reordered.summary);
    let after = db.chapters(&study_id).expect("chapters");
    assert_eq!(after[0].id, ids[1], "the second chapter is first now");

    // Reading one chapter back works, and an unknown one is not found.
    let read = call(
        &mut host,
        "get_chapter",
        serde_json::json!({"study_id": study_id, "chapter_id": first_chapter}),
    );
    assert!(read.ok, "{}", read.summary);
    let missing = call(
        &mut host,
        "get_chapter",
        serde_json::json!({"study_id": study_id, "chapter_id": "nope"}),
    );
    assert_eq!(missing.code, result_code::NOT_FOUND);
}

#[test]
fn publishing_a_study_closes_it_to_further_writes() {
    let db = rbc_db::Db::open_memory().expect("db");
    let mut host = BoardWriteHost::new(&db, "test");
    let created = call(
        &mut host,
        "create_study",
        serde_json::json!({"title": "Published", "description": ""}),
    );
    let study_id = data(&created)["study_id"].as_str().expect("id").to_string();
    call(
        &mut host,
        "add_chapter",
        serde_json::json!({"study_id": study_id, "title": "One", "start_fen": START}),
    );

    let published = call(
        &mut host,
        "publish_study",
        serde_json::json!({"study_id": study_id, "expected_version": 1}),
    );
    assert!(published.ok, "{}", published.summary);
    assert_eq!(data(&published)["version"], serde_json::json!(2));

    // Every write is refused now, and the refusal says why and what to do.
    for (name, arguments) in [
        (
            "add_chapter",
            serde_json::json!({"study_id": study_id, "title": "Two", "start_fen": START}),
        ),
        (
            "write_study_text",
            serde_json::json!({"study_id": study_id, "markdown": "later"}),
        ),
        (
            "reorder_chapters",
            serde_json::json!({"study_id": study_id, "chapter_ids": []}),
        ),
        ("publish_study", serde_json::json!({"study_id": study_id})),
    ] {
        let refusal = call(&mut host, name, arguments);
        assert!(!refusal.ok, "{name} must be refused on a published study");
        assert_eq!(
            refusal.code,
            result_code::CONFLICT,
            "{name}: {}",
            refusal.summary
        );
        assert!(
            refusal.summary.contains("copy"),
            "{name} must offer a way forward: {}",
            refusal.summary
        );
    }
}

#[test]
fn a_study_write_needs_an_id_and_a_title() {
    let db = rbc_db::Db::open_memory().expect("db");
    let mut host = BoardWriteHost::new(&db, "test");
    let no_id = call(&mut host, "add_chapter", serde_json::json!({"title": "x"}));
    assert_eq!(no_id.code, result_code::INVALID_ARGUMENTS);
    let unknown = call(
        &mut host,
        "add_chapter",
        serde_json::json!({"study_id": "nope", "title": "x"}),
    );
    assert_eq!(unknown.code, result_code::NOT_FOUND);
}

#[test]
fn every_study_tool_the_registry_advertises_is_implemented() {
    // A tool in tools/list that answers "no tool called" is worse than one that
    // is absent: a client has no way to tell before it tries.
    let db = rbc_db::Db::open_memory().expect("db");
    let mut host = BoardWriteHost::new(&db, "test");
    let registry = rbc_ai::tools::ToolRegistry::builtin();
    for spec in registry.iter() {
        if !matches!(spec.effects, rbc_ai::tools::ToolEffect::SessionWrite) {
            continue;
        }
        let result = ToolHost::call(
            &mut host,
            &ToolCall::new("c1", spec.name.clone(), serde_json::json!({})),
        );
        assert_ne!(
            result.code,
            result_code::NOT_FOUND,
            "{} is advertised but unimplemented",
            spec.name
        );
    }
}
