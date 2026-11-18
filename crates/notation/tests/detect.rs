use knightlm_notation::{Format, detect};

#[test]
fn detects_each_format() {
    let cases: &[(&str, Format)] = &[
        ("[Event \"x\"]\n\n1. e4 *", Format::Pgn),
        ("1. e4 e5 2. Nf3", Format::San),
        ("Nf3", Format::San),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            Format::Fen,
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -",
            Format::Fen,
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - bm e4;",
            Format::Epd,
        ),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - id \"x\";",
            Format::Epd,
        ),
        ("position startpos moves e2e4", Format::UciPosition),
        ("e2e4 e7e5 g1f3 e7e8q", Format::UciMoves),
    ];
    for (text, want) in cases {
        assert_eq!(detect(text.as_bytes()), *want, "{text}");
    }
}
