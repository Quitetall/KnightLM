//! Authoring a study through the permission gate: a study write is not a board
//! write, and it must not slip through on a board grant.
#![cfg(feature = "host")]

use rbc_ai::policy::{Origin, PermissionProfile, PermissionScope, PolicyDecision, decide};
use rbc_ai::protocol::SessionMode;
use rbc_ai::tools::ToolRegistry;

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

// A copy of the spec is enough: `decide` only reads it.
fn spec(name: &str) -> rbc_ai::tools::ToolSpec {
    ToolRegistry::builtin()
        .get(name)
        .expect("a registered tool")
        .clone()
}

#[test]
fn a_study_write_needs_the_study_scope_not_the_board_one() {
    let create = spec("create_study");
    // No grant: it waits for a person.
    assert_eq!(
        decide(&create, SessionMode::Chat, Origin::InApp, None),
        PolicyDecision::RequiresApproval
    );
    // A board-write grant does not let study content through: they are different
    // scopes because they are different promises.
    assert_eq!(
        decide(
            &create,
            SessionMode::Chat,
            Origin::InApp,
            Some(PermissionScope::BoardWrite)
        ),
        PolicyDecision::RequiresApproval,
        "a board grant is not a study grant"
    );
    assert_eq!(
        decide(
            &create,
            SessionMode::Chat,
            Origin::InApp,
            Some(PermissionScope::StudyContent)
        ),
        PolicyDecision::Allow
    );
}

#[test]
fn a_chapter_write_is_gated_the_same_way() {
    let chapter = spec("add_chapter");
    assert_eq!(
        decide(&chapter, SessionMode::Chat, Origin::InApp, None),
        PolicyDecision::RequiresApproval
    );
    let text = spec("write_study_text");
    assert_eq!(
        decide(
            &text,
            SessionMode::Chat,
            Origin::InApp,
            Some(PermissionScope::BoardWrite)
        ),
        PolicyDecision::RequiresApproval,
        "text is study content"
    );
}

#[test]
fn a_granted_profile_is_what_changes_the_answer() {
    let mut profile = PermissionProfile::default();
    assert!(!profile.grants(Origin::InApp, PermissionScope::StudyContent));
    profile.grant(Origin::InApp, PermissionScope::StudyContent);
    assert!(profile.grants(Origin::InApp, PermissionScope::StudyContent));
    // And granting to the app does not grant to MCP.
    assert!(!profile.grants(Origin::Mcp, PermissionScope::StudyContent));
}

#[test]
fn an_approved_study_write_then_applies() {
    // The shape of the approval loop: the call is refused with an id, a person
    // approves, and the same call applies because the study is not locked.
    use rbc_ai::agent::ToolHost;
    use rbc_ai::approval::ApprovalStore;
    use rbc_ai::board::BoardWriteHost;
    use rbc_ai::protocol::{ApprovalStatus, ToolCall};

    let db = rbc_db::Db::open_memory().expect("db");
    let mut host = BoardWriteHost::new(&db, "test");

    // Nobody approved anything yet, so the tool is refused by policy; the app's
    // loop is what queues the request, and here we queue it the same way.
    let call = ToolCall::new(
        "c1",
        "create_study",
        serde_json::json!({"title": "Approved"}),
    );
    let store = ApprovalStore::new(&db);
    let changeset = rbc_ai::approval::Changeset {
        summary: "create_study needs approval".into(),
        board: None,
        notes: Vec::new(),
        arguments: Some(call.arguments.clone()),
    };
    let request = store
        .request(Origin::InApp, None, &call, &changeset, 0)
        .expect("queued");
    assert_eq!(
        store.status(&request.id).expect("status"),
        Some(ApprovalStatus::Pending)
    );

    // A person approves, then the write runs.
    store.approve(&request.id, 1).expect("approve");
    assert_eq!(
        store.status(&request.id).expect("status"),
        Some(ApprovalStatus::Approved)
    );
    let applied = ToolHost::call(&mut host, &call);
    assert!(applied.ok, "{}", applied.summary);
    let study_id = applied.data.as_ref().expect("data")["study_id"]
        .as_str()
        .expect("id")
        .to_string();
    // And the same call no longer applies: the study it made is still a draft, so
    // a second create is a second study, not a duplicate.
    let again = ToolHost::call(&mut host, &call);
    assert!(again.ok);
    assert_ne!(
        again.data.as_ref().expect("data")["study_id"],
        serde_json::json!(study_id),
        "each approved create makes its own study"
    );

    // A chapter lands in the approved study.
    let chapter = ToolHost::call(
        &mut host,
        &ToolCall::new(
            "c2",
            "add_chapter",
            serde_json::json!({"study_id": study_id, "title": "From an approval", "start_fen": START}),
        ),
    );
    assert!(chapter.ok, "{}", chapter.summary);
}
