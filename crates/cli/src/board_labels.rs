//! Rules-QA labels for the board-encoder pretraining (docs/model/board-encoder.md).
//!
//! stdin is JSONL `{"fen": "...", "id"?: ...}`; stdout is one JSON line per input.
//! Every field is a rules fact computed by shakmaty on the notation crate's
//! validated position: no engine, no search, no evaluation. A failure on one line
//! is reported on that line (`ok: false`), never guessed around.
//!
//! Square index is `rank * 8 + file` (a1 = 0, h1 = 7, a8 = 56). Bit strings are 64
//! characters in that order. `attacked_*` are geometric attacks (a pinned piece
//! still attacks; an own-occupied square counts as defended), the same geometry as
//! the `atk` and `def` sections of cgt/1.

use anyhow::{Context, Result};
use knightlm_notation::fen::{CastlingStyle, parse_fen, write_fen};
use serde_json::{Value, json};
use shakmaty::uci::UciMove;
use shakmaty::{CastlingMode, Chess, Color, Position, Role, Square};
use std::io::{BufRead, Write};

const ROLES: [Role; 5] = [
    Role::Pawn,
    Role::Knight,
    Role::Bishop,
    Role::Rook,
    Role::Queen,
];

fn points(role: Role) -> u32 {
    match role {
        Role::Pawn => 1,
        Role::Knight | Role::Bishop => 3,
        Role::Rook => 5,
        Role::Queen => 9,
        Role::King => 0,
    }
}

fn attacked_by(pos: &Chess, color: Color) -> String {
    let board = pos.board();
    let occupied = board.occupied();
    Square::ALL
        .iter()
        .map(|&sq| {
            if board.attacks_to(sq, color, occupied).any() {
                '1'
            } else {
                '0'
            }
        })
        .collect()
}

fn side_counts(pos: &Chess, color: Color) -> (Vec<u32>, u32) {
    let board = pos.board();
    let counts: Vec<u32> = ROLES
        .iter()
        .map(|&r| (board.by_color(color) & board.by_role(r)).count() as u32)
        .collect();
    let total = ROLES.iter().zip(&counts).map(|(&r, n)| points(r) * n).sum();
    (counts, total)
}

/// Play `walk` pseudo-random legal moves (xorshift64 seeded with `seed`) from `pos`, stopping early at a
/// position with no legal move. Only the compiler's move generator chooses moves; the walk exists to give
/// the encoder positions other than the corpus's single ply.
pub fn random_walk(mut pos: Chess, walk: u64, seed: u64) -> Chess {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    for _ in 0..walk {
        let legal = pos.legal_moves();
        if legal.is_empty() {
            break;
        }
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        pos.play_unchecked(legal[(state % legal.len() as u64) as usize]);
    }
    pos
}

#[cfg(test)]
pub fn labels_for_fen(fen: &str) -> Result<Value> {
    labels_for_position(parse_fen(fen)?.position)
}

pub fn labels_for_position(pos: Chess) -> Result<Value> {
    let board = pos.board();
    let squares: String = Square::ALL
        .iter()
        .map(|&sq| match board.piece_at(sq) {
            Some(p) => p.char(),
            None => '.',
        })
        .collect();
    let (white_counts, white_points) = side_counts(&pos, Color::White);
    let (black_counts, black_points) = side_counts(&pos, Color::Black);
    let legal: Vec<String> = pos
        .legal_moves()
        .iter()
        .map(|m| UciMove::from_move(*m, CastlingMode::Standard).to_string())
        .collect();
    Ok(json!({
        "ok": true,
        "turn": if pos.turn().is_white() { "w" } else { "b" },
        "squares": squares,
        "attacked_white": attacked_by(&pos, Color::White),
        "attacked_black": attacked_by(&pos, Color::Black),
        "in_check": pos.is_check(),
        "legal_moves": legal,
        // Pawn, knight, bishop, rook, queen counts.
        "counts": {"white": white_counts, "black": black_counts},
        "points": {"white": white_points, "black": black_points},
    }))
}

pub fn write_labels(input: impl BufRead, mut output: impl Write) -> Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let row: Value = serde_json::from_str(&line).context("input line is not JSON")?;
        let fen = row.get("fen").and_then(Value::as_str).unwrap_or_default();
        let walk = row.get("walk").and_then(Value::as_u64).unwrap_or(0);
        let seed = row.get("seed").and_then(Value::as_u64).unwrap_or(0);
        let walked = parse_fen(fen).map(|p| random_walk(p.position, walk, seed));
        let mut out = match walked
            .as_ref()
            .map_err(|e| anyhow::anyhow!("{e}"))
            .and_then(|p| labels_for_position(p.clone()))
        {
            Ok(v) => v,
            Err(e) => json!({"ok": false, "error": e.to_string()}),
        };
        match &walked {
            // With a walk the row describes the position reached, and says where it started.
            Ok(p) if walk > 0 => {
                out["fen"] = json!(write_fen(p, CastlingStyle::XFen));
                out["source_fen"] = json!(fen);
                out["walk"] = json!(walk);
                out["seed"] = json!(seed);
            }
            _ => out["fen"] = json!(fen),
        }
        if let Some(id) = row.get("id") {
            out["id"] = id.clone();
        }
        writeln!(output, "{out}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    #[test]
    fn start_position_labels() {
        let v = labels_for_fen(START).unwrap();
        assert_eq!(&v["squares"].as_str().unwrap()[..8], "RNBQKBNR");
        assert_eq!(v["legal_moves"].as_array().unwrap().len(), 20);
        assert_eq!(v["in_check"], false);
        assert_eq!(v["points"]["white"], 39);
        // White attacks every square of ranks 1-2 except a1/h1... a2..h2 are guarded; rank 3 is hit by pawns.
        let aw = v["attacked_white"].as_str().unwrap();
        assert_eq!(&aw[16..24], "11111111");
        assert_eq!(&aw[40..48], "00000000");
    }

    #[test]
    fn walk_is_deterministic_and_legal() {
        let line = format!("{{\"fen\":\"{START}\",\"walk\":12,\"seed\":7}}\n");
        let (mut a, mut b) = (Vec::new(), Vec::new());
        write_labels(line.as_bytes(), &mut a).unwrap();
        write_labels(line.as_bytes(), &mut b).unwrap();
        assert_eq!(a, b);
        let v: Value = serde_json::from_slice(&a).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["source_fen"], START);
        // The reported FEN is the position reached, and labelling it again agrees.
        let again = labels_for_fen(v["fen"].as_str().unwrap()).unwrap();
        assert_eq!(again["legal_moves"], v["legal_moves"]);
        assert_eq!(again["squares"], v["squares"]);
        assert_ne!(
            v["squares"].as_str().unwrap(),
            labels_for_fen(START).unwrap()["squares"].as_str().unwrap()
        );
    }

    #[test]
    fn bad_fen_is_reported_not_guessed() {
        let mut out = Vec::new();
        write_labels("{\"fen\":\"nope\"}\n".as_bytes(), &mut out).unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["ok"], false);
    }
}
