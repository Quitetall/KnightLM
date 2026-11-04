use std::collections::HashMap;

use rbc_context::{
    Annotation, Annotations, Arrow, BoardSummary, ContextInput, ContextSpec, DatabaseEvidence,
    DatabaseGame, EngineEvidence, EngineLine, Mode, OpeningRow, OpeningsEvidence, Section,
    SectionId, Turn, TurnRole, compile,
};

fn section<'a>(context: &'a rbc_context::CompiledContext, id: &str) -> &'a Section {
    context
        .sections
        .iter()
        .find(|section| section.id == id)
        .unwrap_or_else(|| panic!("missing section `{id}`"))
}

fn start_history() -> Vec<String> {
    ["e4", "e5", "Nf3", "Nc6", "Bb5"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

#[test]
fn same_input_twenty_times_is_byte_identical() {
    let spec = ContextSpec::chat_v1();
    let mut input = ContextInput::start();
    input.history = start_history();

    let first = compile(&spec, &input).expect("compiles");
    assert!(first.verify_hash().expect("hash is representable"));
    for _ in 0..19 {
        let next = compile(&spec, &input).expect("compiles");
        assert_eq!(first.sha256, next.sha256);
        assert_eq!(first.prompt, next.prompt);
        assert_eq!(
            first.canonical_bytes().expect("canonical"),
            next.canonical_bytes().expect("canonical")
        );
    }
}

#[test]
fn input_json_key_order_does_not_matter() {
    let spec = ContextSpec::play_v1();
    let first: ContextInput = serde_json::from_str(
        r#"{
            "fen": null,
            "history": ["e4", "e5"],
            "mode": "play",
            "side": "white",
            "annotations": {"items": [{"ply": 1, "san": "e4", "comment": "good"}]},
            "turns": [{"role": "user", "content": "hello"}],
            "evidence": {"siblings": []}
        }"#,
    )
    .expect("valid input");
    let second: ContextInput = serde_json::from_str(
        r#"{
            "evidence": {"siblings": []},
            "turns": [{"role": "user", "content": "hello"}],
            "annotations": {"items": [{"ply": 1, "san": "e4", "comment": "good"}]},
            "side": "white",
            "mode": "play",
            "history": ["e4", "e5"],
            "fen": null
        }"#,
    )
    .expect("valid input");

    let first = compile(&spec, &first).expect("compiles");
    let second = compile(&spec, &second).expect("compiles");
    assert_eq!(first.sha256, second.sha256);
    assert_eq!(first.prompt, second.prompt);
}

#[test]
fn legal_moves_are_sorted_by_san_then_uci() {
    let spec = ContextSpec {
        sections: vec![
            SectionId::System,
            SectionId::Position,
            SectionId::Ascii,
            SectionId::LegalMoves,
        ],
        ..ContextSpec::chat_v1()
    };
    let context = compile(&spec, &ContextInput::start()).expect("compiles");
    let section = section(&context, "legal_moves");
    let rows = section
        .data
        .as_ref()
        .expect("data")
        .as_array()
        .expect("array");
    let pairs: Vec<(String, String)> = rows
        .iter()
        .map(|row| {
            (
                row["san"].as_str().expect("san").to_owned(),
                row["uci"].as_str().expect("uci").to_owned(),
            )
        })
        .collect();
    let mut sorted = pairs.clone();
    sorted.sort();
    assert_eq!(pairs, sorted);
    assert_eq!(pairs.len(), 20);
}

#[test]
fn required_section_budget_failure_names_sections() {
    let mut spec = ContextSpec::play_v1();
    spec.token_budget = 1;
    let mut input = ContextInput::start();
    input.mode = Mode::Play;
    let error = compile(&spec, &input)
        .expect_err("a one-token budget cannot hold the required sections")
        .to_string();
    assert!(error.contains("required sections exceed token budget"));
    assert!(error.contains("system"));
    assert!(error.contains("position"));
    assert!(error.contains("ascii"));
    assert!(error.contains("legal_moves"));
}

#[test]
fn evidence_fingerprint_changes_hash() {
    let spec = ContextSpec::training_v1();
    let mut input = ContextInput::start();
    input.evidence.engine = Some(EngineEvidence {
        name: "Stockfish".to_owned(),
        binary_sha256: "abc123".to_owned(),
        depth: 10,
        threads: 1,
        lines: vec![EngineLine {
            rank: 1,
            score_cp: Some(32),
            mate: None,
            pv: vec!["e2e4".to_owned(), "e7e5".to_owned()],
        }],
    });

    let at_depth_ten = compile(&spec, &input).expect("compiles");
    let repeated = compile(&spec, &input).expect("compiles");
    assert_eq!(at_depth_ten.sha256, repeated.sha256);
    assert!(section(&at_depth_ten, "engine").included);
    let fingerprints = at_depth_ten.data["evidence_fingerprints"]
        .as_object()
        .expect("fingerprints");
    assert!(fingerprints.contains_key("engine"));

    input.evidence.engine.as_mut().expect("engine").depth = 11;
    let at_depth_eleven = compile(&spec, &input).expect("compiles");
    assert_ne!(at_depth_ten.sha256, at_depth_eleven.sha256);

    let before = at_depth_ten.data["evidence_fingerprints"]["engine"]
        .as_str()
        .expect("fingerprint");
    let after = at_depth_eleven.data["evidence_fingerprints"]["engine"]
        .as_str()
        .expect("fingerprint");
    assert_ne!(before, after);
}

#[test]
fn ascii_and_fen_are_stable_for_start_and_midgame() {
    let spec = ContextSpec::chat_v1();
    let start = compile(&spec, &ContextInput::start()).expect("compiles");
    let start_position = section(&start, "position");
    assert!(
        start_position
            .content
            .contains("fen: rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
    );
    let start_ascii = section(&start, "ascii");
    let expected_start = concat!(
        "8 r n b q k b n r\n",
        "7 p p p p p p p p\n",
        "6 . . . . . . . .\n",
        "5 . . . . . . . .\n",
        "4 . . . . . . . .\n",
        "3 . . . . . . . .\n",
        "2 P P P P P P P P\n",
        "1 R N B Q K B N R\n",
        "  a b c d e f g h",
    );
    assert!(start_ascii.content.contains(expected_start));

    let mut midgame = ContextInput::start();
    midgame.history = start_history();
    let midgame = compile(&spec, &midgame).expect("compiles");
    let position = section(&midgame, "position");
    assert!(
        position
            .content
            .contains("fen: r1bqkbnr/pppp1ppp/2n5/1B2p3/4P3/5N2/PPPP1PPP/RNBQK2R b KQkq - 3 3")
    );
    let ascii = section(&midgame, "ascii");
    let expected_midgame = concat!(
        "8 r . b q k b n r\n",
        "7 p p p p . p p p\n",
        "6 . . n . . . . .\n",
        "5 . B . . p . . .\n",
        "4 . . . . P . . .\n",
        "3 . . . . . N . .\n",
        "2 P P P P . P P P\n",
        "1 R N B Q K . . R\n",
        "  a b c d e f g h",
    );
    assert!(ascii.content.contains(expected_midgame));

    let midgame_fen = "r1bqkbnr/pppp1ppp/2n5/1B2p3/4P3/5N2/PPPP1PPP/RNBQK2R b KQkq - 3 3";
    let midgame_only = compile(&spec, &ContextInput::from_fen(midgame_fen)).expect("compiles");
    assert!(
        section(&midgame_only, "position")
            .content
            .contains(&format!("fen: {midgame_fen}"))
    );
    assert!(
        section(&midgame_only, "ascii")
            .content
            .contains(expected_midgame)
    );
}

#[test]
fn section_order_does_not_depend_on_hashmap_iteration() {
    let ids = [
        SectionId::System,
        SectionId::Position,
        SectionId::Ascii,
        SectionId::LegalMoves,
        SectionId::History,
        SectionId::Annotations,
    ];
    let mut first_map = HashMap::new();
    for id in ids {
        first_map.insert(id.as_str().to_owned(), id);
    }
    let mut second_map = HashMap::new();
    for id in ids.iter().rev() {
        second_map.insert(id.as_str().to_owned(), *id);
    }

    let mut input = ContextInput::start();
    input.history = start_history();
    input.annotations.items.push(rbc_context::Annotation {
        ply: 1,
        san: Some("e4".to_owned()),
        comment: Some("king pawn".to_owned()),
        ..rbc_context::Annotation::default()
    });

    let first_spec = ContextSpec {
        sections: first_map.values().copied().collect(),
        ..ContextSpec::chat_v1()
    };
    let second_spec = ContextSpec {
        sections: second_map.values().copied().collect(),
        ..ContextSpec::chat_v1()
    };
    let first = compile(&first_spec, &input).expect("compiles");
    let second = compile(&second_spec, &input).expect("compiles");
    assert_eq!(first.sha256, second.sha256);

    let expected: Vec<String> = SectionId::ALL
        .iter()
        .filter(|id| first_map.contains_key(id.as_str()))
        .map(|id| id.as_str().to_owned())
        .collect();
    let actual: Vec<String> = first
        .sections
        .iter()
        .map(|section| section.id.clone())
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn optional_evidence_sections_are_rendered_and_fingerprinted() {
    let spec = ContextSpec::training_v1();
    let mut input = ContextInput::start();
    input.evidence.engine = Some(EngineEvidence {
        name: "Stockfish".to_owned(),
        binary_sha256: "abc123".to_owned(),
        depth: 12,
        threads: 1,
        lines: vec![EngineLine {
            rank: 1,
            score_cp: Some(20),
            mate: None,
            pv: vec!["e2e4".to_owned()],
        }],
    });
    input.evidence.openings = Some(OpeningsEvidence {
        source: "masters".to_owned(),
        query: "query".to_owned(),
        rows: vec![OpeningRow {
            move_san: "e4".to_owned(),
            games: 100,
            white_wins: 40,
            draws: 30,
            black_wins: 30,
            win_pct: Some(45.0),
        }],
    });
    input.evidence.database = Some(DatabaseEvidence {
        query: "query".to_owned(),
        total_games: 1,
        white_wins: 1,
        draws: 0,
        black_wins: 0,
        sample: vec![DatabaseGame {
            id: "g1".to_owned(),
            white: "Alice".to_owned(),
            black: "Bob".to_owned(),
            result: "1-0".to_owned(),
            date: Some("2020.01.01".to_owned()),
        }],
    });
    input.evidence.annotations = Some(Annotations {
        items: vec![Annotation {
            ply: 1,
            san: Some("e4".to_owned()),
            nag: Some(1),
            comment: Some("good".to_owned()),
            arrows: vec![Arrow {
                from: "e2".to_owned(),
                to: "e4".to_owned(),
            }],
            circles: vec!["d5".to_owned()],
        }],
    });
    input.evidence.notes = Some("First paragraph.\n\nSecond paragraph.".to_owned());
    let board = BoardSummary {
        id: "b1".to_owned(),
        title: "Sibling".to_owned(),
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".to_owned(),
        ply: Some(3),
        note: Some("note".to_owned()),
    };
    input.evidence.siblings = vec![board.clone()];
    input.evidence.compare = vec![board];

    let context = compile(&spec, &input).expect("compiles");
    for id in [
        "engine",
        "openings",
        "database",
        "annotations",
        "notes",
        "siblings",
        "compare",
    ] {
        assert!(section(&context, id).included, "section `{id}` is included");
        assert!(
            context.data["evidence_fingerprints"]
                .as_object()
                .expect("fingerprints")
                .contains_key(id),
            "fingerprint for `{id}`"
        );
    }

    let mut changed = input.clone();
    changed.evidence.notes = Some("Different notes.".to_owned());
    assert_ne!(
        context.sha256,
        compile(&spec, &changed).expect("compiles").sha256
    );
}

#[test]
fn evidence_array_order_does_not_matter() {
    let spec = ContextSpec::training_v1();
    let mut input = ContextInput::start();
    input.evidence.engine = Some(EngineEvidence {
        name: "Stockfish".to_owned(),
        binary_sha256: "abc123".to_owned(),
        depth: 12,
        threads: 1,
        lines: vec![
            EngineLine {
                rank: 2,
                score_cp: Some(10),
                mate: None,
                pv: vec!["d2d4".to_owned()],
            },
            EngineLine {
                rank: 1,
                score_cp: Some(20),
                mate: None,
                pv: vec!["e2e4".to_owned()],
            },
        ],
    });
    input.evidence.openings = Some(OpeningsEvidence {
        source: "masters".to_owned(),
        query: "query".to_owned(),
        rows: vec![
            OpeningRow {
                move_san: "d4".to_owned(),
                games: 50,
                white_wins: 20,
                draws: 15,
                black_wins: 15,
                win_pct: Some(40.0),
            },
            OpeningRow {
                move_san: "e4".to_owned(),
                games: 100,
                white_wins: 40,
                draws: 30,
                black_wins: 30,
                win_pct: Some(45.0),
            },
        ],
    });
    input.evidence.database = Some(DatabaseEvidence {
        query: "query".to_owned(),
        total_games: 2,
        white_wins: 1,
        draws: 1,
        black_wins: 0,
        sample: vec![
            DatabaseGame {
                id: "g2".to_owned(),
                white: "Carol".to_owned(),
                black: "Dave".to_owned(),
                result: "1/2-1/2".to_owned(),
                date: None,
            },
            DatabaseGame {
                id: "g1".to_owned(),
                white: "Alice".to_owned(),
                black: "Bob".to_owned(),
                result: "1-0".to_owned(),
                date: Some("2020.01.01".to_owned()),
            },
        ],
    });
    input.evidence.annotations = Some(Annotations {
        items: vec![
            Annotation {
                ply: 2,
                san: Some("e5".to_owned()),
                ..Annotation::default()
            },
            Annotation {
                ply: 1,
                san: Some("e4".to_owned()),
                ..Annotation::default()
            },
        ],
    });
    input.evidence.siblings = vec![
        BoardSummary {
            id: "b2".to_owned(),
            ..BoardSummary::default()
        },
        BoardSummary {
            id: "b1".to_owned(),
            ..BoardSummary::default()
        },
    ];

    let first = compile(&spec, &input).expect("compiles");
    let mut reversed = input.clone();
    reversed
        .evidence
        .engine
        .as_mut()
        .expect("engine")
        .lines
        .reverse();
    reversed
        .evidence
        .openings
        .as_mut()
        .expect("openings")
        .rows
        .reverse();
    reversed
        .evidence
        .database
        .as_mut()
        .expect("database")
        .sample
        .reverse();
    reversed
        .evidence
        .annotations
        .as_mut()
        .expect("annotations")
        .items
        .reverse();
    reversed.evidence.siblings.reverse();

    let second = compile(&spec, &reversed).expect("compiles");
    assert_eq!(first.sha256, second.sha256);
    assert_eq!(first.prompt, second.prompt);
}

#[test]
fn turns_truncate_oldest_first_and_never_split_a_turn() {
    let mut spec = ContextSpec {
        id: "turns-test".to_owned(),
        sections: vec![
            SectionId::System,
            SectionId::Position,
            SectionId::Ascii,
            SectionId::Turns,
        ],
        legal_moves: false,
        turn_window: 12,
        token_budget: 100_000,
        ..ContextSpec::chat_v1()
    };
    let mut input = ContextInput::start();
    input.turns = vec![
        Turn {
            role: TurnRole::User,
            content: "oldest message one with enough words to cost several tokens".to_owned(),
        },
        Turn {
            role: TurnRole::Assistant,
            content: "middle message two with enough words to cost several tokens".to_owned(),
        },
        Turn {
            role: TurnRole::User,
            content: "newest message three".to_owned(),
        },
    ];

    let full = compile(&spec, &input).expect("compiles");
    let required_tokens: u32 = full
        .sections
        .iter()
        .filter(|section| section.required)
        .map(|section| section.tokens)
        .sum();
    let turns_tokens = section(&full, "turns").tokens;
    assert!(turns_tokens > 1);

    let mut truncated = None;
    for budget in (required_tokens + 1)..(required_tokens + turns_tokens) {
        spec.token_budget = budget;
        let context = compile(&spec, &input).expect("compiles");
        let turns = section(&context, "turns");
        if turns.included && turns.truncated {
            truncated = Some(context);
            break;
        }
    }
    let context = truncated.expect("some budget leaves room for a suffix of turns");
    let turns = section(&context, "turns");
    let rows = turns
        .data
        .as_ref()
        .expect("data")
        .as_array()
        .expect("array");
    assert!(!rows.is_empty());
    assert!(rows.len() < input.turns.len());
    assert_eq!(
        rows.last().expect("last turn")["content"],
        "newest message three"
    );
    assert!(!turns.content.contains("oldest message one"));
}
