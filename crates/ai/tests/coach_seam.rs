//! The coach turn across the seam, with no model: the server's checked-in coach completion
//! (`chat_completion_coach_example.json`, which the Python server tests reproduce) is read the way
//! RBC reads it. The move is gated like any provisional line, the coach tool calls run on the Rust
//! executors, and the marks come back as structured data. The real game never changes.
#![cfg(feature = "core")]

use knightlm_contract::openai::ChatCompletion;
use knightlm_contract::{CHAT_COMPLETION_COACH_EXAMPLE_JSON, PACKET_FORMAT_VERSION};
use knightlm_core::state::{GameRecord, START_FEN};
use rbc_ai::coach::{CoachSession, Mark, MarkColor, UiEffect};
use rbc_ai::model_turn::provisional_token;
use rbc_ai::tools::{ModelCall, ModelMode, ModelToolbox};

#[test]
fn a_served_coach_turn_runs_on_the_rust_executors() {
    let done: ChatCompletion = serde_json::from_str(CHAT_COMPLETION_COACH_EXAMPLE_JSON).unwrap();
    let coach = done.knightlm.as_ref().unwrap().coach.as_ref().unwrap();
    assert!(!coach.fallback);

    let mut session = CoachSession::new(GameRecord {
        schema_version: PACKET_FORMAT_VERSION,
        game_id: "seam".into(),
        source: "test".into(),
        initial_fen: START_FEN.into(),
        history_complete: true,
        moves: vec![],
        variant: "standard".into(),
    })
    .unwrap();
    let real = session.real().fen();
    let legal = session.real().legal_moves();

    // The committed move is the last legal provisional line, and it is legal here.
    assert_eq!(coach.provisional.last(), coach.mv.as_ref());
    assert!(coach.provisional.iter().all(|m| legal.contains(m)));
    assert_eq!(provisional_token("Provisional: g1f3"), coach.mv.as_deref());

    // Every forwarded tool call is in the coach schema and runs.
    let toolbox = ModelToolbox::new(ModelMode::Coach);
    let calls = done.choices[0].message.tool_calls.as_ref().unwrap();
    let mut effects = Vec::new();
    for call in calls {
        let call = ModelCall {
            name: call.function.name.clone(),
            arguments: serde_json::from_str(&call.function.arguments).unwrap(),
        };
        toolbox
            .check_call(&call)
            .expect("forwarded calls are in the coach schema");
        effects.push(session.execute(&call).expect("runs").ui);
    }
    assert_eq!(
        session.marks(),
        [Mark::Arrow {
            from: "g1".into(),
            to: "f3".into(),
            color: MarkColor::Blue
        }]
    );
    assert_eq!(
        effects[0],
        Some(UiEffect::AddMark {
            mark: session.marks()[0].clone()
        })
    );
    assert_eq!(effects[1], None, "scratch_step has no UI effect");
    assert_ne!(session.scratch().fen(), real);
    assert_eq!(session.real().fen(), real, "the real game is untouched");
}
