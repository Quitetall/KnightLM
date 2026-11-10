use axum::{
    Json,
    extract::{Path, State},
};
use rusty_brick_chess::{api::AppState, domain_api};

fn state() -> AppState {
    AppState {
        db: rbc_db::Db::open_memory().unwrap(),
        db_dir: std::env::temp_dir(),
        data_dir: std::env::temp_dir(),
        engine_path: None,
        model_url: None,
        lamu_child: Default::default(),
        tournament_actions: Default::default(),
        default_depth: 5,
        console: Default::default(),
        turn_cancels: Default::default(),
    }
}

async fn create(state: &AppState) -> String {
    let body = serde_json::from_value(serde_json::json!({
        "name": "Retry fixture",
        "format": "double-round-robin",
        "competitors": ["llm:fixture:One:v1", "llm:fixture:Two:v1"],
    }))
    .unwrap();
    domain_api::create_tournament(State(state.clone()), Json(body))
        .await
        .unwrap()
        .0
        .id
}

async fn step(state: &AppState, id: &str) -> serde_json::Value {
    domain_api::tournament_action(State(state.clone()), Path((id.to_string(), "step".into())))
        .await
        .unwrap()
        .0
}

#[tokio::test]
async fn retry_and_concurrent_steps_reuse_the_stored_match() {
    let state = state();
    let id = create(&state).await;
    let (first, second) = tokio::join!(step(&state, &id), step(&state, &id));
    assert_eq!(
        first["started"].as_array().unwrap().len() + second["started"].as_array().unwrap().len(),
        1
    );
    let matches = state.db.ai_matches(200).unwrap();
    assert_eq!(matches.len(), 1);
    let original = &matches[0];
    let game: rbc_ai::game::MatchSnapshot = serde_json::from_str(&original.state_json).unwrap();
    assert!(game.moves.is_empty());
    assert!(!original.pgn.is_empty());

    // Reloaded action state can retry after an interrupted request.
    let restarted = AppState {
        tournament_actions: Default::default(),
        ..state.clone()
    };
    let reply = step(&restarted, &id).await;
    assert!(reply["started"].as_array().unwrap().is_empty());
    assert_eq!(reply["errors"].as_array().unwrap().len(), 1);
    let retried = state.db.ai_matches(200).unwrap();
    assert_eq!(retried.len(), 1);
    assert_eq!(retried[0].id, original.id);
    assert_eq!(retried[0].state_json, original.state_json);

    // Tournament ownership prevents a stale Play control from resetting it.
    assert!(
        domain_api::watch_reset(State(state.clone()), Path(original.id.clone()))
            .await
            .is_err()
    );
    assert_eq!(
        state.db.ai_match(&original.id).unwrap().unwrap().state_json,
        original.state_json
    );
}

#[tokio::test]
async fn reset_clears_the_ended_marker_when_a_match_reopens() {
    let state = state();
    let body = serde_json::from_value(serde_json::json!({
        "config": rbc_ai::game::MatchConfig::default(),
    }))
    .unwrap();
    let created = domain_api::create_match(State(state.clone()), Json(body))
        .await
        .unwrap()
        .0;
    let id = created["id"].as_str().unwrap().to_owned();
    let resign = serde_json::from_value(serde_json::json!({"color":"white"})).unwrap();
    let resigned = domain_api::resign_match(State(state.clone()), Path(id.clone()), Json(resign))
        .await
        .unwrap();
    assert_eq!(resigned.0["result"]["score"], "0-1");
    assert!(state.db.ai_match(&id).unwrap().unwrap().ended.is_some());
    let reset = domain_api::watch_reset(State(state.clone()), Path(id.clone()))
        .await
        .unwrap();
    assert!(reset.0["match"]["moves"].as_array().unwrap().is_empty());
    let row = state.db.ai_match(&id).unwrap().unwrap();
    assert!(row.ended.is_none());
    assert!(row.result_json.is_none());
}
