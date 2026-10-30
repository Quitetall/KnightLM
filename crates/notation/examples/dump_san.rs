use knightlm_notation::san::{parse_san, to_san};
use shakmaty::{fen::Fen, uci::UciMove, CastlingMode, Chess, Position};

fn main() {
    let fens = [
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3",
    ];
    for fen in fens {
        let pos: Chess = Fen::from_ascii(fen.as_bytes())
            .unwrap()
            .into_position(CastlingMode::Standard)
            .unwrap();
        for mv in pos.legal_moves() {
            let uci = UciMove::from_move(mv, CastlingMode::Standard).to_string();
            let san = to_san(&pos, &uci).unwrap();
            let back = parse_san(&pos, &san).unwrap();
            assert_eq!(back, uci);
            println!(
                "{}",
                serde_json::json!({"fen": fen, "uci": uci, "san": san})
            );
        }
    }
}
