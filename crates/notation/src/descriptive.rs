//! English descriptive notation (`P-K4`, `Kt-KB3`, `BxKt`) resolved against a
//! position. Older books use it. A move is accepted only if exactly one legal
//! move in the position can be written that way, so ambiguity and illegal
//! moves are refused rather than guessed.

use anyhow::{Result, bail};
use shakmaty::{CastlingMode, Chess, Color, File, Move, Position, Role, uci::UciMove};

const FILES: [&str; 8] = ["QR", "QKt", "QB", "Q", "K", "KB", "KKt", "KR"];

fn file_aliases(file: File) -> Vec<&'static str> {
    let full = FILES[file as usize];
    match full {
        "Q" | "K" => vec![full],
        _ => vec![full, &full[1..]],
    }
}

fn relative_rank(color: Color, rank: u32) -> u32 {
    if color == Color::White {
        rank + 1
    } else {
        8 - rank
    }
}

fn piece_letters(role: Role) -> &'static str {
    match role {
        Role::Pawn => "P",
        Role::Knight => "Kt",
        Role::Bishop => "B",
        Role::Rook => "R",
        Role::Queen => "Q",
        Role::King => "K",
    }
}

/// Every spelling of one legal move.
fn spellings(pos: &Chess, mv: &Move) -> Vec<String> {
    let color = pos.turn();
    if mv.is_castle() {
        let long = mv.to().file() < mv.from().unwrap().file();
        return if long {
            vec!["O-O-O".into(), "0-0-0".into()]
        } else {
            vec!["O-O".into(), "0-0".into()]
        };
    }
    let from = mv.from().unwrap();
    let to = mv.to();
    let role = mv.role();
    let mut attackers = vec![piece_letters(role).to_string()];
    if role == Role::Pawn {
        attackers.extend(file_aliases(from.file()).iter().map(|f| format!("{f}P")));
    } else if role != Role::King && role != Role::Queen {
        let side = if (from.file() as usize) < 4 { "Q" } else { "K" };
        attackers.push(format!("{side}{}", piece_letters(role)));
    }
    let rank = relative_rank(color, to.rank() as u32);
    let mut dests = Vec::new();
    for file in file_aliases(to.file()) {
        dests.push(format!("{file}{rank}"));
        if rank == 1 {
            dests.push(format!("{file}sq"));
        }
    }
    let mut out = Vec::new();
    match mv.capture() {
        None => {
            for a in &attackers {
                for d in &dests {
                    out.push(format!("{a}-{d}"));
                }
            }
        }
        Some(captured) => {
            let mut targets = vec![piece_letters(captured).to_string()];
            if captured == Role::Pawn {
                targets.extend(file_aliases(to.file()).iter().map(|f| format!("{f}P")));
            } else if captured != Role::King && captured != Role::Queen {
                targets.push(format!(
                    "{}{}",
                    if (to.file() as usize) < 4 { "Q" } else { "K" },
                    piece_letters(captured)
                ));
            }
            for a in &attackers {
                for t in targets.iter().chain(dests.iter()) {
                    out.push(format!("{a}x{t}"));
                }
            }
        }
    }
    if let Some(promo) = mv.promotion() {
        let letter = piece_letters(promo);
        let base = out.clone();
        for s in base {
            out.push(format!("{s}={letter}"));
        }
        if promo != Role::Queen {
            out.retain(|s| s.contains('='));
        }
    }
    out
}

fn normalise(text: &str) -> String {
    let mut s: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '\u{2013}' | '\u{2014}' | '\u{2212}' => '-',
            '\u{00d7}' => 'x',
            c => c,
        })
        .collect();
    for suffix in ["e.p.", "dblch", "mate", "ch", "+", "#", "!", "?"] {
        while s.ends_with(suffix) {
            s.truncate(s.len() - suffix.len());
        }
    }
    s.replace("(Q)", "=Q")
        .replace("(R)", "=R")
        .replace("(B)", "=B")
        .replace("(Kt)", "=Kt")
        .replace("(P)", "")
}

/// Resolve descriptive notation to a UCI move in `pos`.
pub fn parse_descriptive(pos: &Chess, text: &str) -> Result<String> {
    let want = normalise(text);
    if want.is_empty() {
        bail!("empty move");
    }
    let mut found: Vec<Move> = Vec::new();
    for mv in pos.legal_moves() {
        if spellings(pos, &mv)
            .iter()
            .any(|s| *s == want || s.trim_end_matches("=Q") == want)
        {
            found.push(mv);
        }
    }
    match found.as_slice() {
        [one] => Ok(UciMove::from_move(*one, CastlingMode::Standard).to_string()),
        [] => bail!("no legal move matches {want:?}"),
        _ => bail!("{want:?} is ambiguous: {} legal moves match", found.len()),
    }
}

/// Play one descriptive move: the resulting position and the UCI move.
pub fn play_descriptive(pos: &Chess, text: &str) -> Result<(Chess, String)> {
    let uci = parse_descriptive(pos, text)?;
    let mv = UciMove::from_ascii(uci.as_bytes())?.to_move(pos)?;
    Ok((pos.clone().play(mv)?, uci))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start() -> Chess {
        Chess::default()
    }

    #[test]
    fn plays_a_short_game_in_descriptive_notation() {
        let mut pos = start();
        for (text, uci) in [
            ("P - K 4", "e2e4"),
            ("P - K 4", "e7e5"),
            ("Kt - K B 3", "g1f3"),
            ("Kt - Q B 3", "b8c6"),
            ("B - Kt 5", "f1b5"),
            ("P - Q R 3", "a7a6"),
            ("B x Kt", "b5c6"),
            ("Q P x B", "d7c6"),
            ("Kt x P", "f3e5"),
            ("O - O - O", "?"),
        ] {
            match play_descriptive(&pos, text) {
                Ok((next, got)) => {
                    assert_eq!(got, uci, "{text}");
                    pos = next;
                }
                Err(_) => assert_eq!(uci, "?", "{text} should have played"),
            }
        }
    }

    #[test]
    fn refuses_ambiguous_and_illegal_moves() {
        let pos = start();
        assert!(parse_descriptive(&pos, "Kt - B 3").is_err());
        assert!(parse_descriptive(&pos, "P - K 5").is_err());
        assert!(parse_descriptive(&pos, "Q - R 5").is_err());
    }

    #[test]
    fn castling_check_and_mate_suffixes() {
        let mut pos = start();
        for text in [
            "P - K 4",
            "P - K 4",
            "Kt - K B 3",
            "Kt - Q B 3",
            "B - B 4",
            "B - B 4",
            "O - O",
        ] {
            pos = play_descriptive(&pos, text).unwrap().0;
        }
        assert!(parse_descriptive(&pos, "B x P ch").is_ok());
    }
}
