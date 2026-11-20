//! Rules-only position facts for the reasoning corpus.
//!
//! stdin is JSONL `{"fen": "...", "move": "<uci>"?}`; stdout is one JSON line per
//! input. Every field is a rules fact computed by shakmaty on the notation
//! crate's validated position: no engine, no search, no evaluation. A failure on
//! one line is reported on that line (`ok: false`), never guessed around.

use anyhow::{Context, Result};
use knightlm_notation::fen::{CastlingStyle, parse_fen, write_fen};
use serde_json::{Value, json};
use shakmaty::uci::UciMove;
use shakmaty::{CastlingMode, Chess, Color, Move, Position, Role, Square};
use std::io::{BufRead, Read, Write};

const ROLES: [Role; 6] = [
    Role::Pawn,
    Role::Knight,
    Role::Bishop,
    Role::Rook,
    Role::Queen,
    Role::King,
];

fn name(role: Role) -> &'static str {
    match role {
        Role::Pawn => "pawn",
        Role::Knight => "knight",
        Role::Bishop => "bishop",
        Role::Rook => "rook",
        Role::Queen => "queen",
        Role::King => "king",
    }
}

fn points(role: Role) -> u32 {
    match role {
        Role::Pawn => 1,
        Role::Knight | Role::Bishop => 3,
        Role::Rook => 5,
        Role::Queen => 9,
        Role::King => 0,
    }
}

fn side(color: Color) -> &'static str {
    if color.is_white() { "white" } else { "black" }
}

fn counts(pos: &Chess, color: Color) -> (Value, u32) {
    let mut map = serde_json::Map::new();
    let mut total = 0;
    for role in ROLES {
        let n = (pos.board().by_color(color) & pos.board().by_role(role)).count() as u32;
        map.insert(name(role).into(), json!(n));
        total += n * points(role);
    }
    (Value::Object(map), total)
}

fn move_facts(pos: &Chess, mv: &Move) -> Value {
    let mut after = pos.clone();
    after.play_unchecked(*mv);
    let castle = match mv {
        Move::Castle { king, rook } => Some(if rook.file() > king.file() {
            "short"
        } else {
            "long"
        }),
        _ => None,
    };
    json!({
        "from": mv.from().map(|s| s.to_string()),
        "to": mv.to().to_string(),
        "role": name(mv.role()),
        "capture": mv.capture().map(name),
        "en_passant": mv.is_en_passant(),
        "castle": castle,
        "promotion": mv.promotion().map(name),
        "gives_check": after.is_check(),
        "fen_after": write_fen(&after, CastlingStyle::XFen),
    })
}

fn position_facts(pos: &Chess, mv: Option<&Move>) -> Value {
    let us = pos.turn();
    let them = !us;
    let board = pos.board();
    let occupied = board.occupied();
    let (white_counts, white_points) = counts(pos, Color::White);
    let (black_counts, black_points) = counts(pos, Color::Black);
    let legal = pos.legal_moves();
    // Legal, non-en-passant captures grouped by the square they take on.
    let mut captures: Vec<(u32, Square, Role)> = Vec::new();
    for m in legal.iter() {
        if let (Some(victim), false) = (m.capture(), m.is_en_passant())
            && !captures.iter().any(|(_, sq, _)| *sq == m.to())
        {
            captures.push((points(victim), m.to(), victim));
        }
    }
    let describe = |&(_, sq, role): &(u32, Square, Role)| {
        let defended = board.attacks_to(sq, them, occupied).any();
        json!({"square": sq.to_string(), "piece": name(role), "defended": defended})
    };
    let capture_list: Vec<Value> = captures
        .iter()
        .filter(|c| c.2 != Role::King)
        .map(describe)
        .collect();
    // Our own pieces the opponent attacks. `defended` counts our pieces that guard the square.
    let mut own_attacked = Vec::new();
    for sq in board.by_color(us) {
        let role = board.role_at(sq).expect("occupied");
        if role == Role::King {
            continue;
        }
        if board.attacks_to(sq, them, occupied).any() {
            own_attacked.push(json!({
                "square": sq.to_string(), "piece": name(role),
                "defended": board.attacks_to(sq, us, occupied).any(),
            }));
        }
    }
    let king = pos.our(Role::King).first().map(|s| s.to_string());
    json!({
        "ok": true,
        "turn": side(us),
        "in_check": pos.is_check(),
        "king_square": king,
        "legal_count": legal.len(),
        "legal_moves": legal.iter().map(|m| UciMove::from_move(*m, CastlingMode::Standard).to_string()).collect::<Vec<_>>(),
        "legal_capture_count": legal.iter().filter(|m| m.is_capture()).count(),
        "legal_check_count": legal.iter().filter(|m| {
            let mut next = pos.clone();
            next.play_unchecked(**m);
            next.is_check()
        }).count(),
        "points": {"white": white_points, "black": black_points},
        "counts": {"white": white_counts, "black": black_counts},
        "captures": capture_list,
        "own_attacked": own_attacked,
        "move": mv.map(|m| move_facts(pos, m)),
    })
}

fn one(request: &Value) -> Result<Value> {
    let fen = request["fen"].as_str().context("fen must be a string")?;
    let pos = parse_fen(fen)?.position;
    let mv = match request["move"].as_str() {
        Some(text) => {
            let uci = UciMove::from_ascii(text.as_bytes()).context("malformed UCI move")?;
            Some(uci.to_move(&pos).context("illegal move")?)
        }
        None => None,
    };
    Ok(position_facts(&pos, mv.as_ref()))
}

pub fn write_facts(input: impl Read, mut output: impl Write) -> Result<()> {
    for line in std::io::BufReader::new(input).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = serde_json::from_str(&line).context("bad request line")?;
        let reply = match one(&request) {
            Ok(facts) => facts,
            Err(error) => json!({"ok": false, "error": error.to_string()}),
        };
        writeln!(output, "{reply}")?;
    }
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(fen: &str, mv: Option<&str>) -> Value {
        let line = json!({"fen": fen, "move": mv}).to_string();
        let mut out = Vec::new();
        write_facts(line.as_bytes(), &mut out).unwrap();
        serde_json::from_slice(&out).unwrap()
    }

    const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    #[test]
    fn start_position_counts() {
        let f = facts(START, Some("g1f3"));
        assert_eq!(f["legal_count"], 20);
        assert_eq!(f["points"]["white"], 39);
        assert_eq!(f["in_check"], false);
        assert_eq!(f["move"]["role"], "knight");
        assert_eq!(f["move"]["gives_check"], false);
    }

    #[test]
    fn capture_check_and_hanging_piece() {
        // White queen takes an undefended black knight on e5 with check.
        let fen = "4k3/8/8/4n3/8/8/8/3QK3 w - - 0 1";
        let f = facts(fen, Some("d1d8"));
        assert_eq!(f["move"]["gives_check"], true);
        let f = facts("4k3/8/8/4n3/8/8/4Q3/4K3 w - - 0 1", Some("e2e5"));
        assert_eq!(f["move"]["capture"], "knight");
        assert_eq!(f["captures"][0]["square"], "e5");
        assert_eq!(f["captures"][0]["defended"], false);
        assert_eq!(f["move"]["gives_check"], true);
    }

    #[test]
    fn castle_and_en_passant_and_bad_move() {
        let f = facts("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1", Some("e1g1"));
        assert_eq!(f["move"]["castle"], "short");
        let f = facts("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1", Some("e5d6"));
        assert_eq!(f["move"]["en_passant"], true);
        assert_eq!(f["captures"].as_array().unwrap().len(), 0);
        let f = facts(START, Some("e2e5"));
        assert_eq!(f["ok"], false);
    }
}
