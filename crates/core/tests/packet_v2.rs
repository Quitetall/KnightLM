//! Packet v2 (docs/model/modes-and-tools.md section 6): per-legal-move rule
//! consequences behind `PacketOptions::legal_move_effects`, a labelled
//! experiment arm. Off by default; the v1 packet bytes must not change.

use knightlm_core::SCHEMA_VERSION;
use knightlm_core::state::{GameRecord, GameState, PacketOptions, START_FEN};

fn record(moves: &[&str], history_complete: bool) -> GameRecord {
    GameRecord {
        schema_version: SCHEMA_VERSION,
        game_id: "v2".into(),
        source: "test".into(),
        initial_fen: START_FEN.into(),
        history_complete,
        moves: moves.iter().map(|m| m.to_string()).collect(),
        variant: "standard".into(),
    }
}

fn state(moves: &[&str]) -> GameState {
    GameState::replay(&record(moves, true)).unwrap()
}

const V2: PacketOptions = PacketOptions {
    legal_choices: false,
    derived_features: false,
    legal_move_effects: true,
};

#[test]
fn default_packet_bytes_are_v1_and_carry_no_effects() {
    let s = state(&[]);
    let json = serde_json::to_string(&s.compile(PacketOptions::default())).unwrap();
    assert_eq!(
        json,
        r#"{"schema_version":1,"ruleset":"standard-untimed-v1","fen":"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1","history_complete":true,"repetition_ledger":{"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -":1},"facts":{"in_check":false,"checkmate":false,"stalemate":false,"repetition_count":1,"threefold_claim_now":false,"fifty_move_claim_now":false,"fivefold_automatic":false,"seventy_five_move_automatic":false,"dead_position":null,"terminal_reason":null}}"#
    );
    // Every other v1 combination also has no effects key.
    for (legal_choices, derived_features) in [(true, false), (false, true), (true, true)] {
        let packet = s.compile(PacketOptions {
            legal_choices,
            derived_features,
            ..PacketOptions::default()
        });
        assert_eq!(packet.schema_version, 1);
        assert!(
            !serde_json::to_string(&packet)
                .unwrap()
                .contains("legal_move_effects")
        );
    }
}

#[test]
fn v2_marks_repetitions_and_clock_resets() {
    // 1.Nf3 Nf6 2.Ng1 : Black's Ng8 would recreate the start position a second time.
    let s = state(&["g1f3", "g8f6", "f3g1"]);
    let packet = s.compile(V2);
    assert_eq!(packet.schema_version, 2);
    let effects = packet.legal_move_effects.clone().unwrap();
    assert_eq!(effects["f6g8"], ["repeats:2"]);
    assert_eq!(
        effects["e7e5"],
        ["resets_clock"],
        "a pawn move resets the clock"
    );
    assert!(
        !effects.contains_key("b8c6"),
        "a quiet move has no consequence"
    );
    // The serialized key appears only in v2.
    assert!(
        serde_json::to_string(&packet)
            .unwrap()
            .contains(r#""legal_move_effects":{"#)
    );
}

#[test]
fn effects_agree_with_a_replay_of_every_legal_move() {
    for moves in [
        vec![],
        vec!["g1f3", "g8f6", "f3g1"],
        vec!["e2e4", "d7d5", "e4d5", "d8d5", "b1c3"],
        vec!["g1f3", "g8f6", "f3g1", "f6g8", "g1f3", "g8f6", "f3g1"],
    ] {
        let s = state(&moves);
        let effects = s.legal_move_effects();
        for legal in s.legal_moves() {
            let mut next = moves.clone();
            next.push(&legal);
            let replayed = state(&next);
            let fen_clock: u32 = replayed.fen().split(' ').nth(4).unwrap().parse().unwrap();
            let repeats = replayed.facts().repetition_count.unwrap();
            let mut expected = Vec::new();
            if repeats >= 2 {
                expected.push(format!("repeats:{repeats}"));
            }
            if fen_clock == 0 {
                expected.push("resets_clock".to_string());
            }
            let got = effects.get(&legal).cloned().unwrap_or_default();
            assert_eq!(got, expected, "{legal} after {moves:?}");
        }
        assert!(effects.values().all(|tags| !tags.is_empty()));
    }
}

#[test]
fn unproven_history_gets_no_repetition_claims_but_keeps_clock_effects() {
    let s = GameState::replay(&record(&["g1f3", "g8f6", "f3g1"], false)).unwrap();
    let effects = s.legal_move_effects();
    assert!(
        effects
            .values()
            .flatten()
            .all(|tag| !tag.starts_with("repeats"))
    );
    assert_eq!(effects["e7e5"], ["resets_clock"]);
}
