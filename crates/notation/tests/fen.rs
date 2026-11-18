use knightlm_notation::fen::{CastlingStyle, parse_fen, write_fen, write_fen4};
use shakmaty::{CastlingMode, Position};

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// (fen, perft depth 1..) from the well-known Chess Programming Wiki suite
/// and the Chess960 perft suite.
const PERFT: &[(&str, &[u64])] = &[
    (START, &[20, 400, 8902, 197_281]),
    (
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        &[48, 2039, 97_862],
    ),
    (
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        &[14, 191, 2812, 43_238],
    ),
    (
        "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        &[6, 264, 9467],
    ),
    (
        "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
        &[44, 1486, 62_379],
    ),
    (
        "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
        &[46, 2079, 89_890],
    ),
    // Chess960 (Shredder file letters).
    (
        "bqnb1rkr/pp3ppp/3ppn2/2p5/5P2/P2P4/NPP1P1PP/BQ1BNRKR w HFhf - 2 9",
        &[21, 528, 12_189],
    ),
    // Same positions as above, spelled Shredder-style: Chess960 castling
    // semantics must give identical node counts.
    (
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w HAha - 0 1",
        &[20, 400, 8902, 197_281],
    ),
    (
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w HAha - 0 1",
        &[48, 2039, 97_862],
    ),
    (
        "b1q1rrkb/pppppppp/3nn3/8/P7/1PPP4/4PPPP/BQNNRKRB w GE - 1 9",
        &[],
    ),
];

#[test]
fn well_known_positions_round_trip_and_perft() {
    for (fen, counts) in PERFT {
        let parsed = parse_fen(fen).unwrap_or_else(|e| panic!("{fen}: {e:#}"));
        assert!(parsed.has_clocks);
        // parse -> write -> parse is the identity on the canonical state.
        for style in [CastlingStyle::XFen, CastlingStyle::Shredder] {
            let out = write_fen(&parsed.position, style);
            let again = parse_fen(&out).unwrap();
            assert_eq!(
                again.position, parsed.position,
                "{fen} via {style:?} -> {out}"
            );
        }
        for (i, want) in counts.iter().enumerate() {
            let got = shakmaty::perft(&parsed.position, i as u32 + 1);
            assert_eq!(got, *want, "{fen} depth {}", i + 1);
        }
    }
}

#[test]
fn standard_fens_write_back_byte_identical() {
    for (fen, _) in PERFT.iter().take(6) {
        let p = parse_fen(fen).unwrap();
        assert_eq!(&write_fen(&p.position, CastlingStyle::XFen), fen);
    }
}

#[test]
fn four_field_fen_parses_and_is_flagged() {
    let p = parse_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -").unwrap();
    assert!(!p.has_clocks);
    assert_eq!(p.mode, CastlingMode::Standard);
    assert_eq!(
        write_fen4(&p.position, CastlingStyle::XFen),
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -"
    );
}

#[test]
fn shredder_and_xfen_spellings_are_the_same_state() {
    // Standard start spelled with file letters is the same position.
    let x = parse_fen(START).unwrap();
    let s = parse_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w HAha - 0 1").unwrap();
    assert_eq!(s.position, x.position);
    assert_eq!(s.mode, CastlingMode::Chess960);
    assert_eq!(
        write_fen(&x.position, CastlingStyle::Shredder),
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w HAha - 0 1"
    );
}

#[test]
fn xfen_king_off_e_file_with_kq_letters() {
    // X-FEN: K/Q mean the outermost rook even when the king is not on e1.
    let xfen = "bqnb1rkr/pp3ppp/3ppn2/2p5/5P2/P2P4/NPP1P1PP/BQ1BNRKR w KQkq - 2 9";
    let shredder = "bqnb1rkr/pp3ppp/3ppn2/2p5/5P2/P2P4/NPP1P1PP/BQ1BNRKR w HFhf - 2 9";
    let a = parse_fen(xfen).unwrap();
    let b = parse_fen(shredder).unwrap();
    assert_eq!(a.position, b.position);
    assert_eq!(a.mode, CastlingMode::Chess960);
    assert_eq!(write_fen(&a.position, CastlingStyle::XFen), xfen);
    assert_eq!(write_fen(&a.position, CastlingStyle::Shredder), shredder);
}

#[test]
fn xfen_inner_rook_uses_file_letter() {
    // Rooks on b1 and d1, king g1: Q would mean the outer b1 rook, so a right
    // for the inner d1 rook must be spelled with its file letter.
    let inner = "4k3/8/8/8/8/8/8/1R1R2K1 w D - 0 1";
    let outer = "4k3/8/8/8/8/8/8/1R1R2K1 w Q - 0 1";
    let p = parse_fen(inner).unwrap();
    assert_eq!(write_fen(&p.position, CastlingStyle::XFen), inner);
    assert_eq!(write_fen(&p.position, CastlingStyle::Shredder), inner);
    let q = parse_fen(outer).unwrap();
    assert_ne!(p.position, q.position, "inner and outer rights differ");
    assert_eq!(write_fen(&q.position, CastlingStyle::XFen), outer);
    assert_eq!(
        write_fen(&q.position, CastlingStyle::Shredder),
        "4k3/8/8/8/8/8/8/1R1R2K1 w B - 0 1"
    );
}

#[test]
fn en_passant_written_only_when_legal() {
    // After 1.e4 the e3 square is not capturable, so it is dropped on write.
    let p = parse_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1").unwrap();
    assert!(write_fen(&p.position, CastlingStyle::XFen).contains(" b KQkq - 0 1"));
    // When a capture exists it is kept.
    let q = parse_fen("rnbqkbnr/ppp1pppp/8/8/3pP3/5N2/PPPP1PPP/RNBQKB1R b KQkq e3 0 3").unwrap();
    assert!(write_fen(&q.position, CastlingStyle::XFen).contains(" e3 "));
}

#[test]
fn rejects_malformed_and_illegal_fens() {
    let bad: &[(&str, &str)] = &[
        ("", "4 or 6 fields"),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq",
            "4 or 6 fields",
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0",
            "4 or 6 fields",
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1 extra",
            "4 or 6 fields",
        ),
        (
            "rnbqkbnr_pppppppp_8_8_8_8_PPPPPPPP_RNBQKBNR_w_KQkq_-_0_1",
            "underscores",
        ),
        ("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP w KQkq - 0 1", "8 ranks"),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBN w KQkq - 0 1",
            "7 files",
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNX w KQkq - 0 1",
            "unknown piece",
        ),
        (
            "rnbqkbnr/pppppppp/9/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "empty-square run",
        ),
        (
            "rnbqkbnr/pppppppp/44/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "empty-square run",
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR x KQkq - 0 1",
            "side to move",
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkz - 0 1",
            "castling field",
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq e4 0 1",
            "en passant",
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - x 1",
            "halfmove",
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 0",
            "fullmove",
        ),
        // Legal-looking syntax, illegal positions: refused, never repaired.
        ("8/8/8/8/8/8/8/8 w - - 0 1", ""),
        ("4k3/8/8/8/8/8/8/8 w - - 0 1", ""),
        ("4k3/8/8/8/8/8/8/4K2K w - - 0 1", ""),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBN1 w KQkq - 0 1",
            "",
        ),
        ("4k3/8/8/8/8/8/8/4K3 w K - 0 1", ""),
        ("4k3/8/8/8/8/8/8/R3K3 w k - 0 1", ""),
        ("4k3/8/8/8/8/8/8/3RKR1R w FH - 0 1", ""),
        ("4k3/4Q3/8/8/8/8/8/4K3 w - - 0 1", ""),
        ("P3k3/8/8/8/8/8/8/4K3 w - - 0 1", ""),
        (
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e6 0 1",
            "",
        ),
    ];
    for (fen, needle) in bad {
        let err = parse_fen(fen).expect_err(&format!("must reject {fen:?}"));
        let msg = format!("{err:#}");
        assert!(
            msg.contains(needle),
            "{fen:?}: expected {needle:?} in {msg:?}"
        );
    }
}

#[test]
fn castling_rights_without_a_rook_are_refused_not_dropped() {
    let err = parse_fen("4k3/8/8/8/8/8/8/4K3 w KQ - 0 1").unwrap_err();
    assert!(!format!("{err:#}").is_empty());
}

#[test]
fn startpos_has_20_legal_moves_sanity() {
    assert_eq!(parse_fen(START).unwrap().position.legal_moves().len(), 20);
}
