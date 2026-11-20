use knightlm_contract::openai::*;
use knightlm_contract::*;
use serde_json::{Value, json};

#[test]
fn the_example_completion_parses_and_round_trips() {
    let parsed: ChatCompletion = serde_json::from_str(CHAT_COMPLETION_EXAMPLE_JSON).unwrap();
    assert_eq!(parsed.object, "chat.completion");
    assert_eq!(
        parsed.submit_move(),
        Some(("White to move. I choose e2e4.".into(), "e2e4".into()))
    );
    let again: Value = serde_json::to_value(&parsed).unwrap();
    let original: Value = serde_json::from_str(CHAT_COMPLETION_EXAMPLE_JSON).unwrap();
    assert_eq!(again, original);
}

#[test]
fn our_request_refuses_an_unknown_field_and_a_foreign_response_field_is_ignored() {
    let ok = json!({"model": "knightlm", "messages": [{"role": "user", "content": "x"}], "stream": true});
    let request: ChatCompletionRequest = serde_json::from_value(ok.clone()).unwrap();
    assert_eq!(serde_json::to_value(&request).unwrap(), ok);
    let mut bad = ok;
    bad["frobnicate"] = json!(1);
    assert!(serde_json::from_value::<ChatCompletionRequest>(bad).is_err());

    let mut foreign: Value = serde_json::from_str(CHAT_COMPLETION_EXAMPLE_JSON).unwrap();
    foreign["system_fingerprint"] = json!("fp_1");
    assert!(serde_json::from_value::<ChatCompletion>(foreign.clone()).is_ok());
    foreign["knightlm"]["surprise"] = json!(1);
    assert!(serde_json::from_value::<ChatCompletion>(foreign).is_err());
}

#[test]
fn schemas_are_valid_json_with_both_modes() {
    let tools: Value = serde_json::from_str(MODEL_TOOLS_JSON).unwrap();
    assert!(tools["modes"]["play"].is_array() && tools["modes"]["coach"].is_array());
    for text in [
        MODEL_PROMPTS_JSON,
        PLAY_COMPLETION_CASES_JSON,
        CHAT_SUMMARY_CASES_JSON,
    ] {
        serde_json::from_str::<Value>(text).unwrap();
    }
    assert_eq!(PACKET_FORMAT_VERSION, 1);
}
