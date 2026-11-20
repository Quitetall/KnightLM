//! Chess Graph Text (`cgt/1`): a canonical, deterministic text form of one
//! position as a graph, and its strict parser. Spec: `docs/model/graph-text.md`.
//!
//! The renderer adds no chess logic: legality, attacks, check and mate come from
//! shakmaty. The parser rebuilds the position from the board and state lines
//! (through the strict FEN reader), recomputes every section, and refuses any
//! text that disagrees with the board or is not byte-identical to the canonical
//! rendering. Carries no engine evaluation and no history beyond the packet's
//! repetition count.

use crate::fen::{CastlingStyle, parse_fen, write_fen};
use anyhow::{Result, bail, ensure};
use knightlm_core::state::GameState;
use shakmaty::{
    Bitboard, CastlingMode, Chess, Color, Move, Piece, Position, Role, Square,
    variant::VariantPosition,
};
use std::collections::BTreeSet;

pub const CGT_VERSION: &str = "cgt/1";

/// Terminal reasons the packet can carry (`RuleFacts::terminal_reason`).
const TERMINAL_REASONS: [&str; 8] = [
    "checkmate",
    "stalemate",
    "proven_dead_position",
    "fivefold_repetition",
    "seventy_five_move",
    "white_wins",
    "black_wins",
    "draw",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CgtVariant {
    /// State, board and legal moves per piece.
    Lite,
    /// Lite plus attack, defend, pin, hanging, check and king-safety edges.
    Full,
}

impl CgtVariant {
    pub fn name(self) -> &'static str {
        match self {
            CgtVariant::Lite => "lite",
            CgtVariant::Full => "full",
        }
    }
}

/// Packet facts the board alone cannot give.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CgtFacts {
    /// `None` is rendered `?` (unknown stays unknown).
    pub repetition_count: Option<u32>,
    /// The packet's terminal reason. Checkmate and stalemate are derived from
    /// the board when this is `None`.
    pub terminal_reason: Option<String>,
}

/// What `parse_cgt` returns: the position and the packet facts it carried.
#[derive(Debug, Clone)]
pub struct ParsedCgt {
    pub position: Chess,
    pub mode: CastlingMode,
    pub variant: CgtVariant,
    pub facts: CgtFacts,
}

fn role_rank(r: Role) -> u8 {
    match r {
        Role::King => 0,
        Role::Queen => 1,
        Role::Rook => 2,
        Role::Bishop => 3,
        Role::Knight => 4,
        Role::Pawn => 5,
    }
}

fn sq_key(s: Square) -> (u32, u32) {
    (s.file() as u32, s.rank() as u32)
}

/// Canonical node order: white before black, K Q R B N P, then file, rank.
fn node_key(board: &shakmaty::Board, s: Square) -> (u8, u8, u32, u32) {
    let p = board.piece_at(s).expect("node is occupied");
    let (f, r) = sq_key(s);
    (
        if p.color.is_white() { 0 } else { 1 },
        role_rank(p.role),
        f,
        r,
    )
}

fn node(board: &shakmaty::Board, s: Square) -> String {
    format!(
        "{}{}",
        board.piece_at(s).expect("node is occupied").char(),
        s
    )
}

fn sorted_nodes(board: &shakmaty::Board, set: Bitboard) -> Vec<Square> {
    let mut v: Vec<Square> = set.into_iter().collect();
    v.sort_by_key(|&s| node_key(board, s));
    v
}

fn sorted_squares(set: Bitboard) -> Vec<Square> {
    let mut v: Vec<Square> = set.into_iter().collect();
    v.sort_by_key(|&s| sq_key(s));
    v
}

fn join_nodes(board: &shakmaty::Board, set: Bitboard) -> String {
    sorted_nodes(board, set)
        .iter()
        .map(|&s| node(board, s))
        .collect::<Vec<_>>()
        .join(" ")
}

fn effective_terminal(pos: &Chess, facts: &CgtFacts) -> Option<String> {
    if let Some(t) = &facts.terminal_reason {
        return Some(t.clone());
    }
    if pos.legal_moves().is_empty() {
        return Some(
            if pos.is_check() {
                "checkmate"
            } else {
                "stalemate"
            }
            .to_owned(),
        );
    }
    None
}

type MoveKey = (u32, u32, u8);

/// `(sort key, token)` for one legal move.
fn move_token(pos: &Chess, mv: &Move) -> (MoveKey, String) {
    let mut after = pos.clone();
    after.play_unchecked(*mv);
    let suffix = if after.is_checkmate() {
        "#"
    } else if after.is_check() {
        "+"
    } else {
        ""
    };
    match mv {
        Move::Castle { king, rook } => {
            let short = rook.file() > king.file();
            (
                (8, 0, u8::from(!short)),
                format!("{}{suffix}", if short { "O-O" } else { "O-O-O" }),
            )
        }
        _ => {
            let to = mv.to();
            let promo = mv.promotion();
            let pk = promo.map_or(0, role_rank);
            let (f, r) = sq_key(to);
            let mut t = String::new();
            if mv.capture().is_some() {
                t.push('x');
            }
            t.push_str(&to.to_string());
            if mv.is_en_passant() {
                t.push_str("ep");
            }
            if let Some(r) = promo {
                t.push('=');
                t.push(r.upper_char());
            }
            t.push_str(suffix);
            ((f, r, pk), t)
        }
    }
}

/// Render a position as cgt/1 text (no trailing newline).
pub fn render_cgt(pos: &Chess, facts: &CgtFacts, variant: CgtVariant) -> String {
    let board = pos.board();
    let fen = write_fen(pos, CastlingStyle::XFen);
    let f: Vec<&str> = fen.split(' ').collect();
    let term = effective_terminal(pos, facts);
    let mut out: Vec<String> = Vec::new();
    out.push(format!("{CGT_VERSION} {}", variant.name()));
    out.push(format!("turn {}", f[1]));
    out.push(format!("castle {}", f[2]));
    out.push(format!("ep {}", f[3]));
    out.push(format!("clock {} {}", f[4], f[5]));
    out.push(match facts.repetition_count {
        Some(n) => format!("rep {n}"),
        None => "rep ?".to_owned(),
    });
    if let Some(t) = &term {
        out.push(format!("term {t}"));
    }
    for color in [Color::White, Color::Black] {
        let list = join_nodes(board, board.by_color(color));
        out.push(format!(
            "{}: {list}",
            if color.is_white() { "W" } else { "B" }
        ));
    }

    out.push("moves:".into());
    if term.is_none() {
        let mut groups: Vec<(Square, Vec<(MoveKey, String)>)> = Vec::new();
        for mv in pos.legal_moves() {
            let from = mv.from().expect("no drops in standard chess");
            let tok = move_token(pos, &mv);
            match groups.iter_mut().find(|(s, _)| *s == from) {
                Some((_, v)) => v.push(tok),
                None => groups.push((from, vec![tok])),
            }
        }
        groups.sort_by_key(|(s, _)| node_key(board, *s));
        for (s, mut v) in groups {
            v.sort();
            let toks: Vec<String> = v.into_iter().map(|(_, t)| t).collect();
            out.push(format!("{}: {}", node(board, s), toks.join(" ")));
        }
    }

    if variant == CgtVariant::Full {
        let occ = board.occupied();
        out.push("atk:".into());
        out.push_edges(board, |s, p| {
            board.attacks_from(s) & board.by_color(p.color.other())
        });
        out.push("def:".into());
        out.push_edges(board, |s, p| {
            board.attacks_from(s) & board.by_color(p.color)
        });

        out.push("pin:".into());
        let mut pins: Vec<(Square, Square, Square)> = Vec::new();
        for color in [Color::White, Color::Black] {
            let Some(king) = board.king_of(color) else {
                continue;
            };
            let base = board.attacks_to(king, color.other(), occ);
            for p in board.by_color(color) {
                if p == king {
                    continue;
                }
                let opened = board.attacks_to(king, color.other(), occ.without(p)) & !base;
                for pinner in opened {
                    pins.push((p, pinner, king));
                }
            }
        }
        pins.sort_by_key(|&(p, pinner, _)| (node_key(board, p), node_key(board, pinner)));
        for (p, pinner, king) in pins {
            out.push(format!(
                "{}: {} {}",
                node(board, p),
                node(board, pinner),
                node(board, king)
            ));
        }

        let mut hang = Bitboard::EMPTY;
        for s in occ {
            let p = board.piece_at(s).expect("occupied");
            if p.role == Role::King {
                continue;
            }
            if board.attacks_to(s, p.color.other(), occ).any()
                && board.attacks_to(s, p.color, occ).is_empty()
            {
                hang.add(s);
            }
        }
        out.push(inline("hang", &join_nodes(board, hang)));
        out.push(inline("check", &join_nodes(board, pos.checkers())));

        out.push("ksafe:".into());
        for color in [Color::White, Color::Black] {
            let Some(king) = board.king_of(color) else {
                continue;
            };
            let (mut open, mut hit, mut own) = (Bitboard::EMPTY, Bitboard::EMPTY, Bitboard::EMPTY);
            for s in board.attacks_from(king) {
                if board.by_color(color).contains(s) {
                    own.add(s);
                } else if board.attacks_to(s, color.other(), occ.without(king)).any() {
                    hit.add(s);
                } else {
                    open.add(s);
                }
            }
            let mut line = format!("{}:", node(board, king));
            for (name, set) in [("open", open), ("hit", hit), ("own", own)] {
                if set.any() {
                    line.push(' ');
                    line.push_str(name);
                    for s in sorted_squares(set) {
                        line.push(' ');
                        line.push_str(&s.to_string());
                    }
                }
            }
            out.push(line);
        }
    }
    out.join("\n")
}

fn inline(key: &str, body: &str) -> String {
    if body.is_empty() {
        format!("{key}: -")
    } else {
        format!("{key}: {body}")
    }
}

trait PushEdges {
    fn push_edges(&mut self, board: &shakmaty::Board, targets: impl Fn(Square, Piece) -> Bitboard);
}

impl PushEdges for Vec<String> {
    fn push_edges(&mut self, board: &shakmaty::Board, targets: impl Fn(Square, Piece) -> Bitboard) {
        for s in sorted_nodes(board, board.occupied()) {
            let p = board.piece_at(s).expect("occupied");
            let t = targets(s, p);
            if t.any() {
                self.push(format!("{}: {}", node(board, s), join_nodes(board, t)));
            }
        }
    }
}

/// Render the current position of a game state (standard chess only), with the
/// packet's repetition count and terminal reason.
pub fn render_cgt_state(state: &GameState, variant: CgtVariant) -> Result<String> {
    let VariantPosition::Chess(pos) = state.position() else {
        bail!("cgt/1 renders standard chess and Chess960 only");
    };
    let facts = state.facts();
    Ok(render_cgt(
        pos,
        &CgtFacts {
            repetition_count: facts.repetition_count,
            terminal_reason: facts.terminal_reason,
        },
        variant,
    ))
}

/// Render a FEN (4 or 6 fields). Repetition is unknown (`rep ?`) and no clocks
/// are invented: a four-field FEN is refused, because `clock` is mandatory.
pub fn render_cgt_fen(fen: &str, variant: CgtVariant) -> Result<String> {
    let parsed = parse_fen(fen)?;
    ensure!(
        parsed.has_clocks,
        "cgt/1 needs the halfmove and fullmove clocks; the FEN has four fields"
    );
    Ok(render_cgt(&parsed.position, &CgtFacts::default(), variant))
}

// ---------------------------------------------------------------- parser

const SECTION_KEYS: [&str; 7] = ["moves", "atk", "def", "pin", "hang", "check", "ksafe"];

fn is_piece_line(line: &str) -> bool {
    let b = line.as_bytes();
    b.len() >= 3
        && Piece::from_char(b[0] as char).is_some()
        && (b'a'..=b'h').contains(&b[1])
        && (b'1'..=b'8').contains(&b[2])
}

fn parse_node(tok: &str) -> Result<(Piece, Square)> {
    let mut chars = tok.chars();
    let c = chars.next().unwrap_or(' ');
    let piece = Piece::from_char(c);
    let sq = tok.get(1..).and_then(|s| s.parse::<Square>().ok());
    match (piece, sq) {
        (Some(p), Some(s)) if tok.len() == 3 => Ok((p, s)),
        _ => bail!("{tok:?} is not a piece node such as Nb1 or pd7"),
    }
}

fn expect_prefix<'a>(line: Option<&&'a str>, prefix: &str) -> Result<&'a str> {
    let Some(line) = line else {
        bail!("missing {prefix:?} line");
    };
    match line.strip_prefix(prefix) {
        Some(rest) => Ok(rest),
        None => bail!("expected a line starting {prefix:?}, found {line:?}"),
    }
}

/// Items of each section, as comparable strings: one per move/edge token, one per
/// line for pins and king safety.
fn section_items(lines: &[&str]) -> Result<Vec<(String, BTreeSet<String>)>> {
    let mut out: Vec<(String, BTreeSet<String>)> = Vec::new();
    for line in lines {
        if is_piece_line(line) {
            let Some((src, rest)) = line.split_once(": ") else {
                bail!("malformed line {line:?}");
            };
            let Some((key, set)) = out.last_mut() else {
                bail!("line {line:?} is outside any section");
            };
            match key.as_str() {
                "moves" | "atk" | "def" => {
                    for t in rest.split(' ') {
                        set.insert(format!("{src}>{t}"));
                    }
                }
                "pin" | "ksafe" => {
                    set.insert((*line).to_owned());
                }
                _ => bail!("line {line:?} does not belong in section {key}"),
            }
            continue;
        }
        let Some((key, rest)) = line.split_once(':') else {
            bail!("unrecognized line {line:?}");
        };
        ensure!(SECTION_KEYS.contains(&key), "unknown section {key:?}");
        let mut set = BTreeSet::new();
        if matches!(key, "hang" | "check") {
            let rest = rest.strip_prefix(' ').unwrap_or("");
            if rest != "-" {
                for t in rest.split(' ') {
                    set.insert(t.to_owned());
                }
            }
        } else {
            ensure!(rest.is_empty(), "section header {key:?} has trailing text");
        }
        out.push((key.to_owned(), set));
    }
    Ok(out)
}

/// Parse cgt/1 text. Refuses text that is not the exact canonical rendering of a
/// legal position: a move that is not legal, an edge the board does not support,
/// a missing or extra item, or a non-canonical order all name their line.
pub fn parse_cgt(text: &str) -> Result<ParsedCgt> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut it = lines.iter().peekable();
    let header = it.next().copied().unwrap_or("");
    let variant = match header.strip_prefix("cgt/1 ") {
        Some("lite") => CgtVariant::Lite,
        Some("full") => CgtVariant::Full,
        Some(v) => bail!("unknown cgt/1 variant {v:?}"),
        None => bail!("not a cgt/1 document: first line is {header:?}"),
    };
    let turn = expect_prefix(it.next(), "turn ")?;
    let castle = expect_prefix(it.next(), "castle ")?;
    let ep = expect_prefix(it.next(), "ep ")?;
    let clock = expect_prefix(it.next(), "clock ")?;
    let rep = expect_prefix(it.next(), "rep ")?;
    let term = match it.peek() {
        Some(l) if l.starts_with("term ") => {
            let t = &l[5..];
            it.next();
            ensure!(
                TERMINAL_REASONS.contains(&t),
                "unknown terminal reason {t:?}"
            );
            Some(t.to_owned())
        }
        _ => None,
    };
    let white = expect_prefix(it.next(), "W:")?;
    let black = expect_prefix(it.next(), "B:")?;

    let mut grid: [Option<Piece>; 64] = [None; 64];
    for (list, color) in [(white, Color::White), (black, Color::Black)] {
        for tok in list.split_whitespace() {
            let (p, s) = parse_node(tok)?;
            ensure!(p.color == color, "{tok} is in the wrong colour list");
            ensure!(grid[s as usize].is_none(), "two pieces on {s}");
            grid[s as usize] = Some(p);
        }
    }
    let mut placement = String::new();
    for rank in (0..8).rev() {
        let mut empty = 0;
        for file in 0..8 {
            match grid[rank * 8 + file] {
                Some(p) => {
                    if empty > 0 {
                        placement.push_str(&empty.to_string());
                        empty = 0;
                    }
                    placement.push(p.char());
                }
                None => empty += 1,
            }
        }
        if empty > 0 {
            placement.push_str(&empty.to_string());
        }
        if rank > 0 {
            placement.push('/');
        }
    }
    let clock_parts: Vec<&str> = clock.split(' ').collect();
    ensure!(clock_parts.len() == 2, "clock needs halfmove and fullmove");
    let fen = format!(
        "{placement} {turn} {castle} {ep} {} {}",
        clock_parts[0], clock_parts[1]
    );
    let parsed = parse_fen(&fen)?;
    let repetition_count = match rep {
        "?" => None,
        n => Some(
            n.parse::<u32>()
                .map_err(|_| anyhow::anyhow!("rep {n:?} is not a number or ?"))?,
        ),
    };
    let facts = CgtFacts {
        repetition_count,
        terminal_reason: term.clone(),
    };
    let pos = parsed.position;

    if let Some(t) = &term {
        let ok = match t.as_str() {
            "checkmate" => pos.is_checkmate(),
            "stalemate" => pos.is_stalemate(),
            "seventy_five_move" => pos.halfmoves() >= 150,
            "fivefold_repetition" => repetition_count.is_some_and(|n| n >= 5),
            _ => true,
        };
        ensure!(ok, "term {t} does not hold on this board");
    }

    let expected_text = render_cgt(&pos, &facts, variant);
    let rest: Vec<&str> = it.map(|l| &**l).collect();
    let expected_lines: Vec<&str> = expected_text.split('\n').collect();
    let exp_start = expected_lines
        .iter()
        .position(|l| *l == "moves:")
        .expect("renderer always writes moves:");
    let actual = section_items(&rest)?;
    let expected = section_items(&expected_lines[exp_start..])?;
    for (key, exp_set) in &expected {
        let empty = BTreeSet::new();
        let act = actual
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, s)| s)
            .unwrap_or(&empty);
        if let Some(extra) = act.difference(exp_set).next() {
            bail!("{key}: {extra:?} does not hold on this board");
        }
        if let Some(miss) = exp_set.difference(act).next() {
            bail!("{key}: {miss:?} is missing");
        }
    }
    if let Some((k, _)) = actual
        .iter()
        .find(|(k, _)| !expected.iter().any(|(e, _)| e == k))
    {
        bail!("section {k} is not part of the {} variant", variant.name());
    }
    ensure!(
        text == expected_text,
        "text is not in canonical form (order, spacing or duplicates differ from the renderer)"
    );
    Ok(ParsedCgt {
        position: pos,
        mode: parsed.mode,
        variant,
        facts,
    })
}
