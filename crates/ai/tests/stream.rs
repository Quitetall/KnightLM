//! The event log and the replay diff.
//!
//! A stream is what a client reconnects to, so the properties that matter are
//! ordering, a cursor that never repeats or skips, and ids that survive a
//! restart. The diff is the other half of replay: it says what changed.
#![cfg(feature = "host")]

use rbc_ai::context_diff::diff_contexts;
use rbc_ai::events::{EventLog, StreamKey};
use rbc_ai::protocol::{AiEventPayload, ErrorInfo, StopReason, Usage};
use rbc_db::Db;

fn payloads() -> Vec<AiEventPayload> {
    vec![
        AiEventPayload::TurnStarted {
            turn_id: "turn-1".into(),
            seq: 2,
        },
        AiEventPayload::ContextReady {
            sha256: "sha256:abc".into(),
            tokens: 42,
            spec_version: "chat-v1@1".into(),
        },
        AiEventPayload::ContentDelta {
            text: "Play e4.".into(),
        },
        AiEventPayload::ToolCall {
            id: "call_1".into(),
            name: "get_board".into(),
            arguments: serde_json::json!({"board_id": "b1"}),
        },
        AiEventPayload::ToolResult {
            id: "call_1".into(),
            ok: true,
            code: "applied".into(),
            summary: "read the board".into(),
        },
        AiEventPayload::Usage {
            usage: Usage {
                tokens_in: 12,
                tokens_out: 3,
                latency_ms: 40,
                cost_estimate: None,
            },
        },
        AiEventPayload::TurnDone {
            turn_id: "turn-1".into(),
            stop_reason: StopReason::EndTurn,
        },
    ]
}

#[test]
fn stream_replays_events_in_order_with_monotonic_ids() {
    let db = Db::open_memory().expect("in-memory database");
    let log = EventLog::new(&db);
    let stream = StreamKey::session("session-1");

    let written = log.append_all(&stream, &payloads());
    assert_eq!(written.len(), 7);
    let ids: Vec<u64> = written.iter().map(|event| event.event_id).collect();
    assert_eq!(ids, vec![1, 2, 3, 4, 5, 6, 7]);

    let replayed = log.replay(&stream, 0).expect("replay");
    assert_eq!(replayed, written, "the log gives back what went in");

    // The payloads survive the trip through JSON unchanged.
    assert!(matches!(
        &replayed[2].payload,
        AiEventPayload::ContentDelta { text } if text == "Play e4."
    ));
    assert!(matches!(
        &replayed[5].payload,
        AiEventPayload::Usage { usage } if usage.tokens_in == 12 && usage.latency_ms == 40
    ));
}

#[test]
fn stream_reconnect_from_a_cursor_returns_only_what_is_new() {
    let db = Db::open_memory().expect("in-memory database");
    let log = EventLog::new(&db);
    let stream = StreamKey::session("session-1");

    log.append_all(&stream, &payloads()[..3]);
    let cursor = log.latest(&stream).expect("latest");
    assert_eq!(cursor, 3);
    assert!(log.replay(&stream, cursor).expect("replay").is_empty());
    assert!(!log.has_more(&stream, cursor).expect("has more"));

    // The next turn starts where the last one stopped.
    log.append_all(&stream, &payloads()[3..5]);
    assert!(log.has_more(&stream, cursor).expect("has more"));
    let missed = log.replay(&stream, cursor).expect("replay");
    assert_eq!(missed.len(), 2);
    assert_eq!(missed[0].event_id, 4);
    assert!(matches!(
        &missed[0].payload,
        AiEventPayload::ToolCall { name, .. } if name == "get_board"
    ));
}

#[test]
fn stream_ids_do_not_repeat_after_a_restart() {
    let path = format!("/tmp/rbc_stream_test_{}.db", std::process::id());
    let _ = std::fs::remove_file(&path);
    let stream = StreamKey::session("session-1");

    {
        let db = Db::open(&path).expect("file database");
        let log = EventLog::new(&db);
        log.append_all(&stream, &payloads()[..2]);
        assert_eq!(log.latest(&stream).expect("latest"), 2);
    }

    {
        // Reopening the same file is what a restart looks like from here.
        let db = Db::open(&path).expect("reopened database");
        let log = EventLog::new(&db);
        assert_eq!(log.latest(&stream).expect("latest"), 2);
        let next = log
            .append(&stream, &payloads()[2])
            .expect("append after restart");
        assert_eq!(next.event_id, 3, "the cursor keeps counting up");
        assert_eq!(log.replay(&stream, 0).expect("replay").len(), 3);
    }

    let _ = std::fs::remove_file(&path);
}

#[test]
fn streams_keep_separate_sequences() {
    let db = Db::open_memory().expect("in-memory database");
    let log = EventLog::new(&db);
    let first = StreamKey::session("session-1");
    let second = StreamKey::session("session-2");
    let match_stream = StreamKey::game("match-1");

    log.append(&first, &payloads()[0]).expect("first");
    log.append(&first, &payloads()[1]).expect("first");

    let other = log.append(&second, &payloads()[0]).expect("second");
    assert_eq!(other.event_id, 1, "a new session starts at 1");

    let a_match = log.append(&match_stream, &payloads()[0]).expect("match");
    assert_eq!(a_match.event_id, 1, "a match is its own stream");

    assert_eq!(log.replay(&first, 0).expect("first").len(), 2);
    assert_eq!(log.replay(&second, 0).expect("second").len(), 1);
    assert_eq!(log.replay(&match_stream, 0).expect("match").len(), 1);
}

#[test]
fn an_empty_stream_has_nothing_to_replay() {
    let db = Db::open_memory().expect("in-memory database");
    let log = EventLog::new(&db);
    let stream = StreamKey::session("never-used");

    assert_eq!(log.latest(&stream).expect("latest"), 0);
    assert!(log.replay(&stream, 0).expect("replay").is_empty());
    assert!(!log.has_more(&stream, 0).expect("has more"));
}

#[test]
fn stream_round_trips_every_payload_shape() {
    let db = Db::open_memory().expect("in-memory database");
    let log = EventLog::new(&db);
    let stream = StreamKey::session("session-1");
    let payloads = vec![
        AiEventPayload::TurnStarted {
            turn_id: "t".into(),
            seq: 9,
        },
        AiEventPayload::ContextReady {
            sha256: "s".into(),
            tokens: 1,
            spec_version: "v".into(),
        },
        AiEventPayload::ReasoningDelta {
            text: "because".into(),
        },
        AiEventPayload::ContentDelta { text: "ok".into() },
        AiEventPayload::ToolCall {
            id: "c".into(),
            name: "n".into(),
            arguments: serde_json::json!({"a": [1, 2]}),
        },
        AiEventPayload::ToolResult {
            id: "c".into(),
            ok: false,
            code: "illegal".into(),
            summary: "no".into(),
        },
        AiEventPayload::BoardAction {
            board_id: "b".into(),
            revision: 3,
            action: "play_move".into(),
        },
        AiEventPayload::ApprovalRequired {
            approval_id: "a".into(),
            changeset_summary: "write".into(),
        },
        AiEventPayload::Usage {
            usage: Usage::default(),
        },
        AiEventPayload::TurnDone {
            turn_id: "t".into(),
            stop_reason: StopReason::ToolCalls,
        },
        AiEventPayload::Error {
            error: ErrorInfo {
                code: "provider_timeout".into(),
                message: "slow".into(),
                retryable: true,
                provider_code: None,
            },
        },
    ];
    let written = log.append_all(&stream, &payloads);
    assert_eq!(written.len(), payloads.len());
    let replayed: Vec<AiEventPayload> = log
        .replay(&stream, 0)
        .expect("replay")
        .into_iter()
        .map(|event| event.payload)
        .collect();
    assert_eq!(replayed, payloads);
}

// --- the replay diff -------------------------------------------------------

#[test]
fn stream_diff_reports_a_rebuild_that_matches() {
    let stored = serde_json::json!({"system": {"mode": "chat"}, "position": {"fen": "start"}});
    let rebuilt = serde_json::json!({"position": {"fen": "start"}, "system": {"mode": "chat"}});
    let diff = diff_contexts(&stored, &rebuilt);
    assert!(diff.identical);
    assert!(diff.is_clean());
}

#[test]
fn stream_diff_names_the_section_that_drifted() {
    let stored = serde_json::json!({
        "system": {"mode": "chat"},
        "position": {"fen": "start"},
        "legal_moves": ["e4"],
    });
    let rebuilt = serde_json::json!({
        "system": {"mode": "chat"},
        "position": {"fen": "after e4"},
        "legal_moves": ["e4"],
    });
    let diff = diff_contexts(&stored, &rebuilt);
    assert!(!diff.identical);
    assert_eq!(diff.changed.len(), 1);
    assert_eq!(diff.changed[0].id, "position");
    assert_ne!(diff.changed[0].stored_sha256, diff.changed[0].rebuilt_sha256);
    assert!(diff.added.is_empty() && diff.removed.is_empty());
}

#[test]
fn stream_diff_of_a_turn_rebuilt_without_a_section_says_which_vanished() {
    let stored = serde_json::json!({"system": 1, "engine": {"score": 12}, "position": 2});
    let rebuilt = serde_json::json!({"system": 1, "position": 2});
    let diff = diff_contexts(&stored, &rebuilt);
    assert_eq!(diff.removed, vec!["engine"]);
    assert!(!diff.is_clean());
}
