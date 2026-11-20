use knightlm_notation::epd::{Epd, Operation, parse_epd, parse_epd_suite, write_epd};
use knightlm_notation::fen::CastlingStyle;

/// First positions of the Win-at-Chess (WAC) suite.
const WAC: &str = r#"2rr3k/pp3pp1/1nnqbN1p/3pN3/2pP4/2P3Q1/PPB4P/R4RK1 w - - bm Qg6; id "WAC.001";
8/7p/5k2/5p2/p1p2P2/Pr1pPK2/1P1R3P/8 b - - bm Rxb2; id "WAC.002";
5rk1/1ppb3p/p1pb4/6q1/3P1p1r/2P1R2P/PP1BQ1P1/5RKN w - - bm Rg3; id "WAC.003";
"#;

#[test]
fn wac_suite_parses_validates_and_round_trips() {
    let suite = parse_epd_suite(WAC).unwrap();
    assert_eq!(suite.len(), 3);
    let lines: Vec<&str> = WAC.lines().collect();
    for (e, line) in suite.iter().zip(&lines) {
        let out = write_epd(e, CastlingStyle::XFen).unwrap();
        assert_eq!(&out, line, "byte-identical rewrite");
        assert_eq!(&parse_epd(&out).unwrap(), e);
        // bm is legal SAN in the position (checked by playing it).
        assert_eq!(e.best_moves().unwrap().len(), 1);
    }
    assert_eq!(suite[0].id().as_deref(), Some("WAC.001"));
    assert_eq!(suite[0].best_moves().unwrap(), vec!["g3g6"]);
    assert_eq!(suite[1].best_moves().unwrap(), vec!["b3b2"]);
}

#[test]
fn multiple_operands_comments_and_quoted_semicolons() {
    let line = r#"4k3/8/8/8/8/8/8/R3K3 w Q - bm Ra8+ Rb1; am Rd1; c0 "a; b ""#;
    // Unterminated final op => refused.
    assert!(parse_epd(line).is_err());
    let ok = r#"4k3/8/8/8/8/8/8/R3K3 w Q - bm Ra8+ Rb1; am Rd1; c0 "a; b"; ce 35; hmvc 0; fmvn 1;"#;
    let e = parse_epd(ok).unwrap();
    assert_eq!(e.op("c0").unwrap(), [r#""a; b""#]);
    assert_eq!(e.op("ce").unwrap(), ["35"]);
    assert_eq!(e.best_moves().unwrap(), vec!["a1a8", "a1b1"]);
    assert_eq!(e.avoid_moves().unwrap(), vec!["a1d1"]);
    assert_eq!(write_epd(&e, CastlingStyle::XFen).unwrap(), ok);
}

#[test]
fn position_only_epd_and_shredder_castling() {
    let e = parse_epd("bqnb1rkr/pp3ppp/3ppn2/2p5/5P2/P2P4/NPP1P1PP/BQ1BNRKR w HFhf -").unwrap();
    assert!(e.operations.is_empty());
    let out = write_epd(&e, CastlingStyle::Shredder).unwrap();
    assert_eq!(
        out,
        "bqnb1rkr/pp3ppp/3ppn2/2p5/5P2/P2P4/NPP1P1PP/BQ1BNRKR w HFhf -"
    );
    assert_eq!(
        write_epd(&e, CastlingStyle::XFen).unwrap(),
        "bqnb1rkr/pp3ppp/3ppn2/2p5/5P2/P2P4/NPP1P1PP/BQ1BNRKR w KQkq -"
    );
}

#[test]
fn set_best_moves_from_uci_writes_san() {
    let mut e = parse_epd("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -").unwrap();
    e.set_best_moves(&["g1f3", "e2e4"]).unwrap();
    e.set_op(Operation::new("id", ["\"start\""]).unwrap());
    assert_eq!(
        write_epd(&e, CastlingStyle::XFen).unwrap(),
        r#"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - bm Nf3 e4; id "start";"#
    );
    assert!(e.set_best_moves(&["e2e5"]).is_err());
}

#[test]
fn rejects_malformed_epd() {
    let pos = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -";
    let bad: Vec<(String, &str)> = vec![
        (String::new(), "4 position fields"),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq".into(),
            "4 position fields",
        ),
        (format!("{pos} bm e4"), "terminating"),
        (format!("{pos} bm e4;;"), "empty operation"),
        (format!("{pos} ;"), "empty operation"),
        (format!("{pos} bm e4; bm d4;"), "more than once"),
        (format!("{pos} 0 1"), "FEN clock"),
        (format!("{pos} 0 1 bm e4;"), "FEN clock"),
        (format!("{pos} id \"open;"), "unterminated"),
        (format!("{pos} id \"a\"b;"), "runs into"),
        (format!("{pos} id a\"b;"), "unterminated"),
        (format!("{pos} id a\"b\"c;"), "stray quote"),
        (format!("{pos} 9bad x;"), "FEN clock"),
        (format!("{pos} _bad x;"), "opcode"),
        (format!("{pos} averyveryverylongopcode x;"), "opcode"),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNX w KQkq - bm e4;".into(),
            "unknown piece",
        ),
        ("4k3/8/8/8/8/8/8/8 w - - bm Ke2;".into(), ""),
        (format!("{pos} bm e4;\n{pos} bm d4;"), "single line"),
    ];
    for (line, needle) in &bad {
        let err = parse_epd(line).expect_err(&format!("must reject {line:?}"));
        let msg = format!("{err:#}");
        assert!(
            msg.contains(needle),
            "{line:?}: expected {needle:?} in {msg:?}"
        );
    }
}

#[test]
fn illegal_best_move_is_refused_on_demand() {
    let e =
        parse_epd("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - bm Qh5; am e5;").unwrap();
    assert!(e.best_moves().is_err());
    assert!(e.avoid_moves().is_err());
}

#[test]
fn suite_errors_name_the_line() {
    let text = format!("{WAC}garbage here\n");
    let err = parse_epd_suite(&text).unwrap_err();
    assert!(format!("{err:#}").contains("line 4"), "{err:#}");
}

#[test]
fn writer_refuses_what_the_parser_would() {
    let mut e: Epd = parse_epd("8/8/8/8/8/8/8/k6K w - -").unwrap();
    e.operations.push(Operation {
        opcode: "bad op".into(),
        operands: vec![],
    });
    assert!(write_epd(&e, CastlingStyle::XFen).is_err());
    assert!(Operation::new("id", ["has space"]).is_err());
    assert!(Operation::new("id", ["semi;colon"]).is_err());
    assert!(Operation::new("id", ["\"un"]).is_err());
}
