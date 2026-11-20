//! cgt/1: round trip, grounding against an independent geometric recomputation,
//! and planted corruption.

use knightlm_core::state::{GameRecord, GameState};
use knightlm_notation::cgt::{
    CgtFacts, CgtVariant, parse_cgt, render_cgt, render_cgt_fen, render_cgt_state,
};
use knightlm_notation::fen::{CastlingStyle, parse_fen, write_fen};
use shakmaty::uci::UciMove;
use shakmaty::{CastlingMode, Chess, Color, Piece, Position, Role, Square};
use std::collections::BTreeSet;

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

const FENS: &[&str] = &[
    START,
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
    "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
    // en passant available
    "rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 3",
    // checkmate (fool's mate) and stalemate
    "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3",
    "7k/5Q2/6K1/8/8/8/8/8 b - - 0 1",
    // Chess960 (Shredder letters)
    "bqnb1rkr/pp3ppp/3ppn2/2p5/5P2/P2P4/NPP1P1PP/BQ1BNRKR w HFhf - 2 9",
    // promotion with check, double check
    "4k3/P7/8/8/8/8/8/4K3 w - - 0 1",
    "4k3/8/8/8/1b6/8/3r4/4K3 w - - 0 1",
];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn positions() -> Vec<Chess> {
    let mut out: Vec<Chess> = FENS
        .iter()
        .map(|f| parse_fen(f).unwrap().position)
        .collect();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..90 {
        let mut pos = Chess::default();
        for _ in 0..160 {
            let moves = pos.legal_moves();
            if moves.is_empty() {
                break;
            }
            let mv = moves[(rng.next() % moves.len() as u64) as usize];
            pos.play_unchecked(mv);
            out.push(pos.clone());
        }
    }
    out
}

fn same_state(a: &Chess, b: &Chess) -> bool {
    write_fen(a, CastlingStyle::XFen) == write_fen(b, CastlingStyle::XFen)
}

#[test]
fn round_trip_is_identical_for_thousands_of_positions() {
    let all = positions();
    assert!(all.len() >= 3000, "only {} positions", all.len());
    for pos in &all {
        for variant in [CgtVariant::Lite, CgtVariant::Full] {
            let facts = CgtFacts {
                repetition_count: Some(1),
                terminal_reason: None,
            };
            let text = render_cgt(pos, &facts, variant);
            let parsed = parse_cgt(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
            assert!(same_state(pos, &parsed.position), "state differs\n{text}");
            assert_eq!(parsed.position.halfmoves(), pos.halfmoves());
            assert_eq!(parsed.position.fullmoves(), pos.fullmoves());
            assert_eq!(parsed.variant, variant);
            assert_eq!(parsed.facts.repetition_count, Some(1));
            let again = render_cgt(&parsed.position, &parsed.facts, parsed.variant);
            assert_eq!(text, again);
        }
    }
}

#[test]
fn rendering_is_deterministic_and_includes_state() {
    let t = render_cgt_fen(START, CgtVariant::Lite).unwrap();
    assert_eq!(t, render_cgt_fen(START, CgtVariant::Lite).unwrap());
    assert!(t.starts_with("cgt/1 lite\nturn w\ncastle KQkq\nep -\nclock 0 1\nrep ?\n"));
    assert!(t.contains("\nNb1: a3 c3\n"));
    assert!(!t.contains("atk:"));
}

#[test]
fn fixture_states_carry_repetition_count() {
    let rec: GameRecord =
        serde_json::from_str(&std::fs::read_to_string("../../fixtures/repetition.json").unwrap())
            .unwrap();
    let state = GameState::replay(&rec).unwrap();
    let text = render_cgt_state(&state, CgtVariant::Full).unwrap();
    assert!(text.contains("\nrep 3\n"), "{text}");
    let parsed = parse_cgt(&text).unwrap();
    assert_eq!(parsed.facts.repetition_count, Some(3));
}

// ---- independent recomputation (plain coordinates, no shakmaty attack tables)

type Grid = [Option<Piece>; 64];

fn grid(pos: &Chess) -> Grid {
    let mut g = [None; 64];
    for s in Square::ALL {
        g[s as usize] = pos.board().piece_at(s);
    }
    g
}

fn at(f: i32, r: i32) -> Option<usize> {
    ((0..8).contains(&f) && (0..8).contains(&r)).then(|| (r * 8 + f) as usize)
}

const ORTHO: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
const DIAG: [(i32, i32); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

fn attacks(g: &Grid, i: usize) -> BTreeSet<usize> {
    let p = g[i].unwrap();
    let (f, r) = ((i % 8) as i32, (i / 8) as i32);
    let mut out = BTreeSet::new();
    let step =
        |df: i32, dr: i32, out: &mut BTreeSet<usize>| at(f + df, r + dr).map(|j| out.insert(j));
    match p.role {
        Role::Pawn => {
            let dr = if p.color == Color::White { 1 } else { -1 };
            step(1, dr, &mut out);
            step(-1, dr, &mut out);
        }
        Role::Knight => {
            for (a, b) in [
                (1, 2),
                (2, 1),
                (-1, 2),
                (-2, 1),
                (1, -2),
                (2, -1),
                (-1, -2),
                (-2, -1),
            ] {
                step(a, b, &mut out);
            }
        }
        Role::King => {
            for (a, b) in ORTHO.iter().chain(DIAG.iter()) {
                step(*a, *b, &mut out);
            }
        }
        _ => {
            let dirs: Vec<(i32, i32)> = match p.role {
                Role::Rook => ORTHO.to_vec(),
                Role::Bishop => DIAG.to_vec(),
                _ => ORTHO.iter().chain(DIAG.iter()).copied().collect(),
            };
            for (df, dr) in dirs {
                let (mut x, mut y) = (f + df, r + dr);
                while let Some(j) = at(x, y) {
                    out.insert(j);
                    if g[j].is_some() {
                        break;
                    }
                    x += df;
                    y += dr;
                }
            }
        }
    }
    out
}

fn name(i: usize) -> String {
    Square::new(i as u32).to_string()
}

fn node(g: &Grid, i: usize) -> String {
    format!("{}{}", g[i].unwrap().char(), name(i))
}

fn section<'a>(text: &'a str, key: &str) -> Vec<&'a str> {
    let mut on = false;
    let mut out = vec![];
    for l in text.lines() {
        if l == format!("{key}:") {
            on = true;
        } else if on {
            if l.ends_with(':')
                || l.starts_with("hang:")
                || l.starts_with("check:")
                || l.starts_with("ksafe:")
                || l.starts_with("pin:")
                || l.starts_with("atk:")
                || l.starts_with("def:")
            {
                break;
            }
            out.push(l);
        }
    }
    out
}

fn edge_set(lines: &[&str]) -> BTreeSet<(String, String)> {
    let mut s = BTreeSet::new();
    for l in lines {
        let (src, rest) = l.split_once(": ").unwrap();
        for t in rest.split(' ') {
            assert!(s.insert((src.to_owned(), t.to_owned())), "duplicate {l}");
        }
    }
    s
}

fn inline_set(text: &str, key: &str) -> BTreeSet<String> {
    let l = text
        .lines()
        .find(|l| l.starts_with(&format!("{key}: ")))
        .unwrap();
    let rest = &l[key.len() + 2..];
    if rest == "-" {
        BTreeSet::new()
    } else {
        rest.split(' ').map(str::to_owned).collect()
    }
}

#[test]
fn edges_agree_with_independent_recomputation() {
    for pos in positions().iter().step_by(3) {
        let g = grid(pos);
        let text = render_cgt(pos, &CgtFacts::default(), CgtVariant::Full);
        let (mut atk, mut def, mut hang) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
        let mut attackers_of = vec![vec![]; 64];
        for i in 0..64 {
            if g[i].is_none() {
                continue;
            }
            for j in attacks(&g, i) {
                attackers_of[j].push(i);
                if let Some(q) = g[j] {
                    let edge = (node(&g, i), node(&g, j));
                    if q.color == g[i].unwrap().color {
                        def.insert(edge)
                    } else {
                        atk.insert(edge)
                    };
                }
            }
        }
        for j in 0..64 {
            let Some(q) = g[j] else { continue };
            if q.role == Role::King {
                continue;
            }
            let by = |c: Color| attackers_of[j].iter().any(|&a| g[a].unwrap().color == c);
            if by(q.color.other()) && !by(q.color) {
                hang.insert(node(&g, j));
            }
        }
        assert_eq!(atk, edge_set(&section(&text, "atk")), "atk\n{text}");
        assert_eq!(def, edge_set(&section(&text, "def")), "def\n{text}");
        assert_eq!(hang, inline_set(&text, "hang"), "hang\n{text}");

        // pins by ray walk from each king
        let mut pins = BTreeSet::new();
        for k in 0..64 {
            let Some(kp) = g[k] else { continue };
            if kp.role != Role::King {
                continue;
            }
            for (df, dr, ortho) in ORTHO
                .iter()
                .map(|d| (d.0, d.1, true))
                .chain(DIAG.iter().map(|d| (d.0, d.1, false)))
            {
                let (mut x, mut y) = ((k % 8) as i32 + df, (k / 8) as i32 + dr);
                let mut first = None;
                while let Some(j) = at(x, y) {
                    if let Some(q) = g[j] {
                        match first {
                            None if q.color == kp.color => first = Some(j),
                            Some(f) if q.color != kp.color => {
                                let slider = q.role == Role::Queen
                                    || q.role == if ortho { Role::Rook } else { Role::Bishop };
                                if slider {
                                    pins.insert(format!(
                                        "{}: {} {}",
                                        node(&g, f),
                                        node(&g, j),
                                        node(&g, k)
                                    ));
                                }
                                break;
                            }
                            _ => break,
                        }
                    }
                    x += df;
                    y += dr;
                }
            }
        }
        let got: BTreeSet<String> = section(&text, "pin")
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(pins, got, "pin\n{text}");

        // checkers of the side to move
        let turn = pos.turn();
        let king = (0..64)
            .find(|&i| {
                g[i] == Some(Piece {
                    color: turn,
                    role: Role::King,
                })
            })
            .unwrap();
        let checkers: BTreeSet<String> = attackers_of[king]
            .iter()
            .filter(|&&a| g[a].unwrap().color != turn)
            .map(|&a| node(&g, a))
            .collect();
        assert_eq!(checkers, inline_set(&text, "check"), "check\n{text}");
    }
}

#[test]
fn every_legal_move_appears_exactly_once() {
    for pos in positions().iter().step_by(2) {
        let text = render_cgt(pos, &CgtFacts::default(), CgtVariant::Lite);
        let g = grid(pos);
        let mut seen: Vec<(usize, String)> = vec![]; // (from index, destination name)
        for l in section(&text, "moves") {
            let (src, rest) = l.split_once(": ").unwrap();
            let from = (0..64)
                .find(|&i| g[i].is_some() && node(&g, i) == src)
                .unwrap();
            for t in rest.split(' ') {
                let base = t.trim_end_matches(['+', '#']);
                let dest = if base.starts_with("O-O") {
                    let rank = if pos.turn() == Color::White { "1" } else { "8" };
                    format!("{}{rank}", if base == "O-O" { "g" } else { "c" })
                } else {
                    let b = base.trim_start_matches('x');
                    b[..2].to_owned() + &b[2..].replace("ep", "")
                };
                seen.push((from, dest));
            }
        }
        let mut expected: Vec<(usize, String)> = pos
            .legal_moves()
            .into_iter()
            .map(|m| {
                let u = UciMove::from_move(m, CastlingMode::Standard).to_string();
                let from = Square::from_ascii(&u.as_bytes()[0..2]).unwrap() as usize;
                let dest = u[2..4].to_owned()
                    + &u[4..]
                        .to_uppercase()
                        .chars()
                        .next()
                        .map(|c| format!("={c}"))
                        .unwrap_or_default();
                (from, dest)
            })
            .collect();
        seen.sort();
        expected.sort();
        assert_eq!(seen, expected, "{text}");
    }
}

#[test]
fn terminal_positions_have_no_moves_and_say_why() {
    let mate = render_cgt_fen(
        "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3",
        CgtVariant::Full,
    )
    .unwrap();
    assert!(mate.contains("\nterm checkmate\n"));
    assert!(mate.contains("moves:\natk:"));
    parse_cgt(&mate).unwrap();
    let stale = render_cgt_fen("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1", CgtVariant::Lite).unwrap();
    assert!(stale.contains("\nterm stalemate\n"));
    parse_cgt(&stale).unwrap();
}

// ---- planted corruption

fn corrupt(find: &str, replace: &str, variant: CgtVariant) -> String {
    let text = render_cgt_fen(START, variant).unwrap();
    assert!(text.contains(find), "{find:?} not in\n{text}");
    let bad = text.replacen(find, replace, 1);
    parse_cgt(&bad)
        .expect_err("corrupt text must be refused")
        .to_string()
}

#[test]
fn parser_refuses_planted_corruption() {
    let l = CgtVariant::Lite;
    let f = CgtVariant::Full;
    assert!(corrupt("Nb1: a3 c3", "Nb1: a3 c3 e5", l).contains("does not hold"));
    assert!(corrupt("Nb1: a3 c3", "Nb1: a3", l).contains("is missing"));
    assert!(corrupt("Nb1: a3 c3", "Nb1: c3 a3", l).contains("canonical"));
    assert!(corrupt("Nb1: a3 c3", "Nb1: a3 a3 c3", l).contains("canonical"));
    assert!(corrupt("Nb1: a3 c3", "Nb1: a3 xc3", l).contains("does not hold"));
    assert!(corrupt("clock 0 1", "clock 0 0", l).contains("fullmove"));
    assert!(corrupt("turn w", "turn x", l).contains("side to move"));
    assert!(!corrupt("castle KQkq", "castle KQkqK", l).is_empty());
    assert!(!corrupt("Ke1 ", "", l).is_empty(), "missing king");
    assert!(!corrupt("W: Ke1", "W: Ke1 Ke2", l).is_empty());
    assert!(corrupt("W: Ke1", "W: ke1", l).contains("wrong colour"));
    assert!(corrupt("rep ?", "rep x", l).contains("rep"));
    assert!(corrupt("cgt/1 lite", "cgt/2 lite", l).contains("not a cgt/1"));
    assert!(!corrupt("moves:", "moves:\nPa2: a3 a4 a5", l).is_empty());
    // edges that disagree with the board (full)
    assert!(!corrupt("def:", "atk:\nPa2: pa7\ndef:", f).is_empty());
    assert!(corrupt("hang: -", "hang: Pa2", f).contains("does not hold"));
    assert!(corrupt("check: -", "check: pe7", f).contains("does not hold"));
    assert!(corrupt("pin:", "pin:\nPd2: bb4 Ke1", f).contains("does not hold"));
    assert!(!corrupt("Ke1: own", "Ke1: hit d1 own", f).is_empty());
    // a section from the wrong variant
    assert!(!corrupt("moves:", "atk:\nmoves:", l).is_empty());
}

#[test]
fn parser_refuses_a_lying_terminal_claim_and_illegal_positions() {
    let t = render_cgt_fen(START, CgtVariant::Lite).unwrap();
    let bad = t.replacen("rep ?\n", "rep ?\nterm checkmate\n", 1);
    assert!(
        parse_cgt(&bad)
            .unwrap_err()
            .to_string()
            .contains("does not hold")
    );
    // side not to move is in check: no legal position
    let ok = render_cgt_fen("4k3/8/8/8/8/8/4R3/4K3 b - - 0 1", CgtVariant::Lite).unwrap();
    parse_cgt(&ok).unwrap();
    let flipped = ok.replacen("turn b", "turn w", 1);
    assert!(format!("{:#}", parse_cgt(&flipped).unwrap_err()).contains("opposite check"));
    // a truncated document
    assert!(parse_cgt("cgt/1 lite").is_err());
    assert!(parse_cgt("").is_err());
}

#[test]
fn four_field_fen_is_refused_rather_than_given_invented_clocks() {
    assert!(
        render_cgt_fen(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -",
            CgtVariant::Lite
        )
        .is_err()
    );
}
