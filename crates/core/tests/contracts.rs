use knightlm_core::{SCHEMA_VERSION, data::*, experiment::*, state::*};

fn setup(fen: &str, complete: bool) -> GameState {
    GameState::replay(&GameRecord {
        schema_version: SCHEMA_VERSION,
        game_id: "fixture".into(),
        source: "synthetic-test:explicit-history-assertion".into(),
        initial_fen: fen.into(),
        history_complete: complete,
        moves: vec![],
    })
    .unwrap()
}
fn start() -> GameState {
    setup(START_FEN, true)
}

#[test]
fn standard_rules_reject_chess960_castling() {
    for fen in [
        "rk5r/8/8/8/8/8/8/RK5R w HAha - 0 1",
        "rk5r/8/8/8/8/8/8/RK5R w KQkq - 0 1",
    ] {
        let record = GameRecord {
            initial_fen: fen.into(),
            ..start().record().clone()
        };
        assert!(GameState::replay(&record).is_err(), "accepted {fen}");
    }
}
fn cycle(s: &mut GameState) {
    for m in ["g1f3", "g8f6", "f3g1", "f6g8"] {
        s.play(m).unwrap();
    }
}

#[test]
fn repetition_is_claimable_then_automatic() {
    let mut s = start();
    cycle(&mut s);
    cycle(&mut s);
    assert_eq!(s.facts().repetition_count, Some(3));
    assert_eq!(s.facts().threefold_claim_now, Some(true));
    assert_eq!(s.facts().terminal_reason, None);
    cycle(&mut s);
    cycle(&mut s);
    assert_eq!(
        s.facts().terminal_reason.as_deref(),
        Some("fivefold_repetition")
    );
    assert!(s.play("e2e4").is_err());
    assert!(s.legal_moves().is_empty());
}

#[test]
fn future_repetition_requires_ledger() {
    let mut historical = start();
    cycle(&mut historical);
    for m in ["g1f3", "g8f6", "f3g1"] {
        historical.play(m).unwrap();
    }
    let isolated = setup(&historical.fen(), true);
    let mut a = RuleSession::new(historical, 8);
    let mut b = RuleSession::new(isolated, 8);
    assert_eq!(
        a.simulate(0, "f6g8")
            .unwrap()
            .state
            .unwrap()
            .facts
            .threefold_claim_now,
        Some(true)
    );
    assert_eq!(
        b.simulate(0, "f6g8")
            .unwrap()
            .state
            .unwrap()
            .facts
            .threefold_claim_now,
        Some(false)
    );
}

#[test]
fn no_invented_history() {
    let s = setup(START_FEN, false);
    assert_eq!(s.facts().repetition_count, None);
    assert!(
        prepare(
            &s,
            &ExperimentConfig::fixture(Arm::FreshState),
            &[],
            &FixtureTokenizer
        )
        .is_err()
    );
}

#[test]
fn halfmove_counters_survive_library_cap_and_reset() {
    let mut s = setup("7k/8/8/8/8/8/8/KR6 w - - 99 51", true);
    s.play("b1b2").unwrap();
    assert!(s.facts().fifty_move_claim_now);
    assert!(s.facts().terminal_reason.is_none());
    s.play("h8g8").unwrap();
    assert!(s.fen().ends_with("101 52"));
    let mut s = setup("7k/8/8/8/8/8/8/KR6 w - - 149 76", true);
    s.play("b1b2").unwrap();
    assert_eq!(
        s.facts().terminal_reason.as_deref(),
        Some("seventy_five_move")
    );
    let mut s = setup("7k/8/8/8/8/8/P7/KR6 w - - 99 51", true);
    s.play("a2a3").unwrap();
    assert!(s.fen().ends_with("0 51"));
}

#[test]
fn checkmate_has_priority_over_seventy_five_moves() {
    let s = setup("7k/6Q1/6K1/8/8/8/8/8 b - - 150 76", true);
    assert!(s.facts().checkmate);
    assert!(!s.facts().seventy_five_move_automatic);
    assert_eq!(s.facts().terminal_reason.as_deref(), Some("checkmate"));
    let stale = setup("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1", true);
    assert!(stale.facts().stalemate);
}

#[test]
fn castling_and_irreversible_rights() {
    let mut s = setup("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1", true);
    assert!(s.legal_moves().contains(&"e1g1".to_string()));
    assert!(s.legal_moves().contains(&"e1c1".to_string()));
    s.play("h1h2").unwrap();
    s.play("h8h7").unwrap();
    s.play("h2h1").unwrap();
    s.play("h7h8").unwrap();
    assert!(!s.is_legal("e1g1"));
    assert!(s.fen().contains(" w Qq "));
    let mut castle = setup("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1", true);
    castle.play("e1g1").unwrap();
    assert!(castle.fen().starts_with("r3k2r/8/8/8/8/8/8/R4RK1"));
    assert!(castle.fen().ends_with("1 1")); // Own rook must not count as captured.
}

#[test]
fn ep_identity_uses_legality_not_raw_fen() {
    let unusable = setup(
        "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
        true,
    );
    let absent = setup(
        "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
        true,
    );
    assert_eq!(unusable.position_key(), absent.position_key());
    let mut usable = setup("7k/8/8/3pP3/8/8/8/K7 w - d6 0 2", true);
    let no_ep = setup("7k/8/8/3pP3/8/8/8/K7 w - - 0 2", true);
    assert_ne!(usable.position_key(), no_ep.position_key());
    usable.play("e5d6").unwrap();
    assert!(usable.fen().starts_with("7k/8/3P4/8/8/8/8/K7"));
    let pinned = setup("k3r3/8/8/3pP3/8/8/8/4K3 w - d6 0 2", true);
    assert!(!pinned.is_legal("e5d6"));
    assert!(pinned.position_key().ends_with(" -"));
}

#[test]
fn promotion_and_pinned_piece_attacks() {
    let mut s = setup("7k/P7/8/8/8/8/8/7K w - - 0 1", true);
    for p in ["q", "r", "b", "n"] {
        assert!(s.is_legal(&format!("a7a8{p}")));
    }
    s.play("a7a8n").unwrap();
    assert!(s.fen().starts_with("N6k"));
    assert_eq!(s.facts().dead_position, Some(true));
    let pinned = setup("k3r3/8/8/8/8/8/4R3/4K3 w - - 0 1", true);
    assert_eq!(pinned.features().pinned_to_move, ["e2"]);
    assert!(pinned.features().attacks["white"].contains(&"a2".into()));
    assert!(!pinned.is_legal("e2a2"));
}

#[test]
fn replay_and_compaction_preserve_state() {
    let mut s = start();
    cycle(&mut s);
    s.play("e2e4").unwrap();
    assert_eq!(
        s.compile(PacketOptions::default()).repetition_ledger.len(),
        1
    );
    let replayed = GameState::replay(s.record()).unwrap();
    assert_eq!(
        s.compile(PacketOptions::default()),
        replayed.compile(PacketOptions::default())
    );
}

#[test]
fn observations_never_silently_repair() {
    let s = start();
    let mut o = BoardObservation {
        schema_version: 1,
        image_id: "board.png".into(),
        placement: START_FEN.split_whitespace().next().unwrap().into(),
    };
    s.verify_observation(&o).unwrap();
    o.placement = "8/8/8/8/8/8/8/8".into();
    assert!(s.verify_observation(&o).is_err());
    assert_eq!(s.fen(), START_FEN);
}

#[test]
fn rule_tools_are_bounded_and_branch_isolated() {
    let s = start();
    let mut tools = RuleSession::new(s.clone(), 3);
    assert!(!tools.simulate(0, "e2e5").unwrap().legal);
    assert!(tools.simulate(0, "e2e4").unwrap().legal);
    assert!(tools.simulate(0, "d2d4").unwrap().legal);
    assert_eq!(tools.calls(), 3);
    assert!(tools.simulate(0, "a2a4").is_err());
    assert_eq!(s.fen(), START_FEN);
}

#[test]
fn fresh_prompts_ignore_prior_model_text() {
    let s = start();
    let mut config = ExperimentConfig::fixture(Arm::FreshState);
    let a = prepare(
        &s,
        &config,
        &["queen disappeared".into()],
        &FixtureTokenizer,
    )
    .unwrap();
    let b = prepare(&s, &config, &["knight on moon".into()], &FixtureTokenizer).unwrap();
    assert_eq!(a.prompt, b.prompt);
    config.prior_model_text = true;
    assert!(config.validate().is_err());
    let mut history = ExperimentConfig::fixture(Arm::HistoryState);
    history.prior_model_text = true;
    assert!(
        prepare(&s, &history, &["CONTAMINATION".into()], &FixtureTokenizer)
            .unwrap()
            .prompt
            .contains("CONTAMINATION")
    );
}

#[test]
fn arms_expose_only_declared_assistance_and_fail_overflow() {
    let s = start();
    for arm in Arm::ALL {
        let c = ExperimentConfig::fixture(arm);
        let r = prepare(&s, &c, &[], &FixtureTokenizer).unwrap();
        assert_eq!(r.legal_constraint.is_some(), arm.masking());
        assert_eq!(
            r.prompt.contains("\"legal_moves\""),
            matches!(arm, Arm::LegalChoices | Arm::Combined)
        );
        assert_eq!(
            r.prompt.contains("\"features\""),
            matches!(arm, Arm::DerivedFeatures | Arm::Combined)
        );
        let mut short = c;
        short.max_input_tokens = 1;
        assert!(prepare(&s, &short, &[], &FixtureTokenizer).is_err());
    }
}

fn decision() -> Decision {
    Decision {
        schema_version: 1,
        uci_move: "e2e4".into(),
        unmasked_uci_move: None,
        explanation: "Free prose is not automatically scored.".into(),
        claims: vec![FactClaim::LegalMove {
            uci: "e2e4".into(),
            legal: true,
        }],
        output_tokens: 12,
        diagnostic_output_tokens: 0,
        tool_calls: 0,
        interventions: 0,
        latency_ms: 0,
    }
}

#[test]
fn legal_mask_cannot_hide_raw_failure() {
    let s = start();
    let c = ExperimentConfig::fixture(Arm::LegalMasking);
    let r = prepare(&s, &c, &[], &FixtureTokenizer).unwrap();
    assert!(score_decision(&s, &c, &r, decision()).is_err());
    let mut d = decision();
    d.unmasked_uci_move = Some("e2e5".into());
    d.diagnostic_output_tokens = 12;
    d.interventions = 1;
    let scored = score_decision(&s, &c, &r, d).unwrap();
    assert!(!scored.raw_legal && scored.selected_legal);
    assert!(!scored.explanation_scored);
    assert_eq!(scored.evidence, "fixture_only");
}

#[test]
fn impossible_telemetry_and_hidden_interventions_are_rejected() {
    let s = start();
    let c = ExperimentConfig::fixture(Arm::FreshState);
    let r = prepare(&s, &c, &[], &FixtureTokenizer).unwrap();
    let mut d = decision();
    d.tool_calls = 1;
    assert!(score_decision(&s, &c, &r, d).is_err());
    let mut d = decision();
    d.interventions = 1;
    assert!(score_decision(&s, &c, &r, d).is_err());
    let mut d = decision();
    d.output_tokens = 1025;
    assert!(score_decision(&s, &c, &r, d).is_err());
}

#[test]
fn teacher_labels_cannot_be_deserialized_as_game_inputs() {
    let mut json = serde_json::to_value(start().record()).unwrap();
    json["engine_cp"] = 900.into();
    assert!(serde_json::from_value::<GameRecord>(json).is_err());
    let mut packet = serde_json::to_value(start().compile(PacketOptions::default())).unwrap();
    packet["best_move"] = "e2e4".into();
    assert!(serde_json::from_value::<ContextPacket>(packet).is_err());
}

#[test]
fn split_audit_catches_transpositions_and_variant_families() {
    let a = SplitItem {
        split: "train".into(),
        game_id: "a".into(),
        family_id: "fa".into(),
        fen: START_FEN.into(),
    };
    let mut b = a.clone();
    b.split = "test".into();
    b.game_id = "b".into();
    b.family_id = "fb".into();
    assert!(audit_splits(&[a.clone(), b.clone()]).is_err());
    let mut s = start();
    s.play("e2e4").unwrap();
    b.fen = s.fen();
    audit_splits(&[a.clone(), b.clone()]).unwrap();
    b.family_id = a.family_id.clone();
    assert!(audit_splits(&[a, b]).is_err());
}

#[test]
fn probe_target_is_not_in_prompt_and_claims_are_checked() {
    let s = start();
    let probes = generate_probes(&s).unwrap();
    assert_eq!(probes.len(), 61);
    for p in probes {
        if p.kind == "move_effect_fen" {
            assert!(!p.inference_prompt().unwrap().contains(&p.expected));
        }
        assert!(!p.inference_prompt().unwrap().contains("\"in_check\""));
    }
    assert!(
        !FactClaim::NextFen {
            uci: "e2e5".into(),
            fen: s.fen()
        }
        .verify(&s)
    );
    assert!(!FactClaim::InCheck { value: true }.verify(&s));
}

#[test]
fn teacher_evaluation_depth_and_score_validation() {
    let raw = r#"{"fen":"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -","evals":[{"depth":20,"knodes":100,"pvs":[{"cp":25,"line":"e2e4 e7e5"}]},{"depth":30,"knodes":200,"pvs":[{"cp":18,"line":"d2d4 d7d5"}]}]}"#;
    let mut row: EvaluationRow = serde_json::from_str(raw).unwrap();
    assert_eq!(row.strongest().unwrap().depth, 30);
    row.evals[0].pvs[0].mate = Some(2);
    assert!(row.validate().is_err());
}
