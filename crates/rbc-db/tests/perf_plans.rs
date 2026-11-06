//! The transcript's hot queries must use their index.
//!
//! A missing index does not fail a test - it just makes the app slow once a
//! conversation is long, which is the hardest kind of slowness to notice and the
//! easiest to reintroduce. These assert the query *plan* rather than a timing:
//! a plan is deterministic, so this cannot flake on a busy machine.
#![allow(clippy::unwrap_used)]

use rbc_db::{AiTurnInput, Db, SessionProfile};

fn session(db: &Db) -> String {
    db.create_ai_session(
        "plans",
        "chat",
        None,
        None,
        SessionProfile {
            model_json: r#"{"model":"bench"}"#,
            context_spec_json: r#"{"id":"chat-v1"}"#,
            permissions_json: "{}",
        },
    )
    .unwrap()
    .id
}

fn plan(db: &Db, sql: &str) -> String {
    let conn = db.connection_for_test();
    let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(3))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    rows.join(" | ")
}

#[test]
fn reading_a_conversation_walks_its_index() {
    let db = Db::open_memory().unwrap();
    let id = session(&db);
    let detail = plan(
        &db,
        "SELECT id, role, content FROM ai_turns WHERE session_id = 'x' ORDER BY seq",
    );
    assert!(
        detail.contains("idx_ai_turns_session"),
        "a session's turns should come from the index, not a scan: {detail}"
    );
    assert!(!detail.contains("SCAN ai_turns"), "{detail}");
    let _ = id;
}

#[test]
fn tearing_down_a_conversation_walks_its_index() {
    // Deleting a session cascades by session_id; without the index that is a
    // full scan of every turn ever written.
    let db = Db::open_memory().unwrap();
    let _ = session(&db);
    let detail = plan(&db, "DELETE FROM ai_turns WHERE session_id = 'x'");
    assert!(!detail.contains("SCAN ai_turns"), "{detail}");
}

#[test]
fn replaying_events_after_a_sequence_walks_the_stream_index() {
    let db = Db::open_memory().unwrap();
    // The stream is keyed by (stream_kind, stream_id, event_id): the client's
    // reconnect cursor is the third column, which is what the index is for.
    let detail = plan(
        &db,
        "SELECT event_id, payload_json FROM ai_events
         WHERE stream_kind = 'session' AND stream_id = 'x' AND event_id > 4
         ORDER BY event_id",
    );
    assert!(
        detail.contains("idx_ai_events_stream") || !detail.contains("SCAN ai_events"),
        "an event replay should not scan the stream table: {detail}"
    );
}

#[test]
fn finding_a_stored_turn_by_sequence_does_not_scan() {
    let db = Db::open_memory().unwrap();
    let id = session(&db);
    db.append_ai_turn(
        &id,
        &AiTurnInput {
            role: "user".into(),
            content: "hello".into(),
            ..AiTurnInput::default()
        },
    )
    .unwrap();
    // The composer asks "what is the highest seq" on every send.
    let detail = plan(
        &db,
        "SELECT MAX(seq) FROM ai_turns WHERE session_id = 'x'",
    );
    assert!(!detail.contains("SCAN ai_turns"), "{detail}");
}
