use knightlm_core::state::{GameState, START_FEN};
use knightlm_notation::fen::parse_fen;
use knightlm_notation::uci::{
    PositionCommand, Start, convert_flavor, export_position_command, import_position_command,
    parse_move_list, parse_position_command, write_move_list, write_position_command,
};
use shakmaty::{CastlingMode, Chess};

const FRC: &str = "bqnb1rkr/pp3ppp/3ppn2/2p5/5P2/P2P4/NPP1P1PP/BQ1BNRKR w HFhf - 2 9";

#[test]
fn move_list_round_trips_and_replays() {
    let text = "e2e4 e7e5 g1f3 b8c6 f1c4 g8f6 e1g1";
    let moves = parse_move_list(&Chess::default(), text, CastlingMode::Standard).unwrap();
    assert_eq!(write_move_list(&moves), text);
    // Agrees with core's replay of the same moves.
    let rec = import_position_command(
        &format!("position startpos moves {text}"),
        "g1",
        CastlingMode::Standard,
    )
    .unwrap();
    let state = GameState::replay(&rec).unwrap();
    assert_eq!(state.record().moves.len(), 7);
}

#[test]
fn castling_flavors_convert_both_ways() {
    let std_moves = "e2e4 e7e5 g1f3 b8c6 f1c4 g8f6 e1g1";
    let k_takes_r = "e2e4 e7e5 g1f3 b8c6 f1c4 g8f6 e1h1";
    let pos = Chess::default();
    let a = parse_move_list(&pos, k_takes_r, CastlingMode::Standard).unwrap();
    assert_eq!(write_move_list(&a), std_moves);
    let b = parse_move_list(&pos, std_moves, CastlingMode::Chess960).unwrap();
    assert_eq!(write_move_list(&b), k_takes_r);
    let refs: Vec<&str> = std_moves.split(' ').collect();
    assert_eq!(
        write_move_list(&convert_flavor(&pos, &refs, CastlingMode::Chess960).unwrap()),
        k_takes_r
    );
}

#[test]
fn castling_flavor_translation_table() {
    // King e1, rooks a1/h1, spelled Shredder-style. Both spellings of both
    // castles are accepted and translate losslessly.
    let p = parse_fen("4k3/8/8/8/8/8/8/R3K2R w HA - 0 1").unwrap();
    let table = [("e1h1", "e1g1"), ("e1a1", "e1c1")];
    for (frc, std) in table {
        for input in [frc, std] {
            assert_eq!(
                parse_move_list(&p.position, input, CastlingMode::Chess960).unwrap(),
                vec![frc]
            );
            assert_eq!(
                parse_move_list(&p.position, input, CastlingMode::Standard).unwrap(),
                vec![std]
            );
        }
    }
}

#[test]
fn true_frc_castling_is_only_spelled_king_takes_rook() {
    // King b1, rooks a1 and h1: queenside castling lands the king on c1.
    let p = parse_fen("4k3/8/8/8/8/8/8/RK5R w HA - 0 1").unwrap();
    let ok = parse_move_list(&p.position, "b1a1", CastlingMode::Chess960).unwrap();
    assert_eq!(ok, vec!["b1a1"]);
    // The standard flavor cannot express it, and is refused rather than
    // emitting b1c1 (which would read back as a plain king step).
    let err = parse_move_list(&p.position, "b1a1", CastlingMode::Standard).unwrap_err();
    assert!(format!("{err:#}").contains("cannot be spelled"), "{err:#}");
    // And b1c1 is a king step, not castling.
    let step = parse_move_list(&p.position, "b1c1", CastlingMode::Chess960).unwrap();
    assert_eq!(step, vec!["b1c1"]);
}

#[test]
fn position_command_startpos_and_fen() {
    let line = "position startpos moves e2e4 e7e5";
    let cmd = parse_position_command(line, CastlingMode::Standard).unwrap();
    assert_eq!(cmd.start, Start::Startpos);
    assert_eq!(write_position_command(&cmd).unwrap(), line);

    // A FEN equal to the start position normalises to startpos.
    let cmd2 = parse_position_command(
        &format!("position fen {START_FEN} moves e2e4 e7e5"),
        CastlingMode::Standard,
    )
    .unwrap();
    assert_eq!(write_position_command(&cmd2).unwrap(), line);

    // Bare commands and 4-field FENs.
    assert_eq!(
        write_position_command(
            &parse_position_command("position startpos", CastlingMode::Standard).unwrap()
        )
        .unwrap(),
        "position startpos"
    );
    let fen4 = "position fen 4k3/8/8/8/8/8/8/4K2R w K - moves h1h8";
    let c = parse_position_command(fen4, CastlingMode::Standard).unwrap();
    assert_eq!(write_position_command(&c).unwrap(), fen4);
}

#[test]
fn record_export_and_import_agree() {
    let rec = import_position_command(
        "position startpos moves d2d4 d7d5 c2c4",
        "gid",
        CastlingMode::Standard,
    )
    .unwrap();
    assert!(rec.history_complete);
    assert_eq!(rec.initial_fen, START_FEN);
    assert_eq!(
        export_position_command(&rec, CastlingMode::Standard).unwrap(),
        "position startpos moves d2d4 d7d5 c2c4"
    );

    // A FEN start never asserts history; FRC start is tagged chess960 and
    // exports in either flavor to an identical replayed state.
    let frc = import_position_command(
        &format!("position fen {FRC} moves f4f5"),
        "f",
        CastlingMode::Chess960,
    )
    .unwrap();
    assert!(!frc.history_complete);
    assert_eq!(frc.variant, "chess960");
    let s1 = GameState::replay(&frc).unwrap();
    let out = export_position_command(&frc, CastlingMode::Chess960).unwrap();
    let again = import_position_command(&out, "f", CastlingMode::Chess960).unwrap();
    assert_eq!(GameState::replay(&again).unwrap().fen(), s1.fen());
}

#[test]
fn dfrc_game_with_castling_replays_in_both_flavors() {
    // Double FRC: different back ranks. White castles queenside with the
    // outer rook; the command converts between flavors and replays equal.
    let fen = "r3k2r/8/8/8/8/8/8/R3K2R w HAha - 0 1";
    let dfrc = import_position_command(
        &format!("position fen {fen} moves e1a1 e8h8"),
        "x",
        CastlingMode::Chess960,
    )
    .unwrap();
    assert_eq!(dfrc.moves, vec!["e1a1", "e8h8"]);
    let std_flavor = export_position_command(&dfrc, CastlingMode::Standard).unwrap();
    assert!(std_flavor.ends_with("moves e1c1 e8g8"), "{std_flavor}");
    let back = import_position_command(&std_flavor, "x", CastlingMode::Chess960).unwrap();
    assert_eq!(back.moves, dfrc.moves);
    assert_eq!(
        GameState::replay(&back).unwrap().fen(),
        GameState::replay(&dfrc).unwrap().fen()
    );
}

#[test]
fn rejects_malformed_and_illegal_uci() {
    let pos = Chess::default();
    let bad_moves: &[(&str, &str)] = &[
        ("e2e5", "illegal"),
        ("e2e4 e2e4", "ply 2"),
        ("e7e5", "illegal"),
        ("e2", "malformed"),
        ("e2e4x", "malformed"),
        ("E2E4", "malformed"),
        ("e2e4 0000", "null move"),
        ("e1g1", "illegal"),
        ("e7e8q", "illegal"),
        ("e2e4,e7e5", "malformed"),
    ];
    for (text, needle) in bad_moves {
        let err = parse_move_list(&pos, text, CastlingMode::Standard)
            .expect_err(&format!("must reject {text:?}"));
        let msg = format!("{err:#}");
        assert!(
            msg.contains(needle),
            "{text:?}: expected {needle:?} in {msg:?}"
        );
    }

    let bad_cmds: &[(&str, &str)] = &[
        ("", "must start with"),
        ("go depth 3", "must start with"),
        ("position", "no start"),
        ("position moves e2e4", "no start"),
        ("position fen", "needs a FEN"),
        ("position startpos e2e4", "unexpected tokens"),
        ("position frc moves e2e4", "expected"),
        ("position startpos moves e2e5", "illegal"),
        ("position fen 8/8/8/8/8/8/8/8 w - - moves a1a2", ""),
        (
            "position fen rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq moves e2e4",
            "4 or 6",
        ),
    ];
    for (line, needle) in bad_cmds {
        let err = parse_position_command(line, CastlingMode::Standard)
            .expect_err(&format!("must reject {line:?}"));
        let msg = format!("{err:#}");
        assert!(
            msg.contains(needle),
            "{line:?}: expected {needle:?} in {msg:?}"
        );
    }
}

#[test]
fn writer_refuses_an_illegal_command() {
    let cmd = PositionCommand {
        start: Start::Startpos,
        moves: vec!["e2e5".into()],
    };
    assert!(write_position_command(&cmd).is_err());
}
