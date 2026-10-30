use knightlm_notation::reader::read_stream;
use std::io::Cursor;

#[test]
fn reads_plain_pgn() {
    let pgn = "1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 1-0\n";
    let recs = read_stream(Cursor::new(pgn.as_bytes()), "test-src").unwrap();
    assert_eq!(recs.len(), 1);
    assert_eq!(recs[0].source, "test-src");
    assert!(recs[0].history_complete);
    assert_eq!(recs[0].moves, vec!["e2e4","e7e5","g1f3","b8c6","f1b5","a7a6"]);
}

#[test]
fn refuses_illegal_move() {
    let pgn = "1. e4 e5 2. Nf6 1-0\n";
    let r = read_stream(Cursor::new(pgn.as_bytes()), "s");
    assert!(r.is_err(), "expected refusal, got {:?}", r);
}

#[test]
fn reads_empty() {
    let recs = read_stream(Cursor::new(b"" as &[u8]), "s").unwrap();
    assert!(recs.is_empty());
}

#[test]
fn history_complete_false_with_setup() {
    let pgn = "[FEN \"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1\"]\n[SetUp \"1\"]\n\n1. e4 e5 *\n";
    let recs = read_stream(Cursor::new(pgn.as_bytes()), "s").unwrap();
    assert_eq!(recs.len(), 1);
    assert!(!recs[0].history_complete);
}
