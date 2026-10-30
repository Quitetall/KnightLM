use anyhow::Result;
use knightlm_core::{experiment::*, state::*};
use knightlm_model::*;

struct StatefulFixture {
    kv: Vec<u8>,
    recurrent: Vec<u8>,
    convolution: Vec<u8>,
    fail_observation: bool,
}
impl StatefulFixture {
    fn dirty() -> Self {
        Self {
            kv: vec![1],
            recurrent: vec![2],
            convolution: vec![3],
            fail_observation: false,
        }
    }
    fn clean(&self) -> bool {
        self.kv.is_empty() && self.recurrent.is_empty() && self.convolution.is_empty()
    }
}
impl ModelBackend for StatefulFixture {
    fn reset(&mut self) {
        self.kv.clear();
        self.recurrent.clear();
        self.convolution.clear();
    }
    fn observe(&mut self, image_id: &str, _: &[u8]) -> Result<BoardObservation> {
        assert!(self.clean(), "dirty state reached perception");
        self.kv.push(1);
        self.recurrent.push(2);
        self.convolution.push(3);
        anyhow::ensure!(!self.fail_observation, "fixture perception failure");
        Ok(BoardObservation {
            schema_version: 1,
            image_id: image_id.into(),
            placement: START_FEN.split_whitespace().next().unwrap().into(),
        })
    }
    fn generate(&mut self, _: &InferenceRequest) -> Result<Decision> {
        assert!(self.clean(), "perception state leaked into reasoning");
        self.kv.push(4);
        self.recurrent.push(5);
        self.convolution.push(6);
        Ok(Decision {
            schema_version: 1,
            uci_move: "e2e4".into(),
            unmasked_uci_move: None,
            explanation: "fixture response".into(),
            claims: vec![],
            output_tokens: 3,
            diagnostic_output_tokens: 0,
            tool_calls: 0,
            interventions: 0,
            latency_ms: 0,
        })
    }
}
fn start() -> GameState {
    GameState::replay(&GameRecord {
        schema_version: 1,
        game_id: "test".into(),
        source: "fixture".into(),
        initial_fen: START_FEN.into(),
        history_complete: true,
        moves: vec![],

        variant: "standard".into(),
    })
    .unwrap()
}

#[test]
fn two_pass_resets_all_state_even_on_error() {
    let mut backend = StatefulFixture::dirty();
    let config = ExperimentConfig::fixture(Arm::FreshState);
    let a = two_pass(
        &mut backend,
        &FixtureTokenizer,
        &start(),
        &config,
        "fixture",
        b"fixture-not-image-decoding",
    )
    .unwrap();
    assert!(backend.clean());
    backend.kv.push(99);
    backend.recurrent.push(88);
    backend.convolution.push(77);
    let b = two_pass(
        &mut backend,
        &FixtureTokenizer,
        &start(),
        &config,
        "fixture",
        b"fixture-not-image-decoding",
    )
    .unwrap();
    assert_eq!(a.decision.uci_move, b.decision.uci_move);
    assert_eq!(a.request.prompt, b.request.prompt);
    backend.fail_observation = true;
    assert!(
        two_pass(
            &mut backend,
            &FixtureTokenizer,
            &start(),
            &config,
            "fixture",
            b"fixture"
        )
        .is_err()
    );
    assert!(backend.clean());
}

#[test]
fn profiles_never_claim_pretrained_admission() {
    let p = ModelProfile {
        schema_version: 1,
        name: "small".into(),
        repository: "HuggingFaceTB/SmolVLM-500M-Instruct".into(),
        revision: Some("a".repeat(40)),
        role: "integration".into(),
        architecture: "idefics3".into(),
    };
    assert!(!verify_model(&p).unwrap().admitted);
}

#[test]
fn cpu_training_and_checkpoint_roundtrip() {
    let dir = std::env::temp_dir().join(format!(
        "knightlm-smoke-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("checkpoint.json");
    let report = smoke::train_smoke(&path).unwrap();
    assert!(report.final_loss < report.initial_loss * 0.01);
    assert!(report.nonzero_finite_gradient && report.parameters_changed);
    assert_eq!(report.reload_max_error, 0.0);
    assert!(smoke::train_smoke(&path).is_err());
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(dir).unwrap();
}
