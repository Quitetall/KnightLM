//! UCI move lists and the `position` command.
//!
//! Moves are validated by playing them, and normalised into the requested
//! castling flavor: `CastlingMode::Standard` writes `e1g1`, and
//! `CastlingMode::Chess960` writes king-takes-rook (`e1h1`). Either flavor is
//! accepted on input, so the two are interconvertible.

use anyhow::{Context, Result, bail, ensure};
use knightlm_core::SCHEMA_VERSION;
use knightlm_core::state::{GameRecord, START_FEN};
use shakmaty::{CastlingMode, Chess, File, Move, Position, Square, uci::UciMove};

use crate::fen::{CastlingStyle, parse_fen, write_fen};

/// Where a `position` command starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Start {
    Startpos,
    Fen(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionCommand {
    pub start: Start,
    /// Normalised in the flavor the command was parsed with.
    pub moves: Vec<String>,
}

/// Play `moves` from `pos`, returning each normalised in `flavor` and the final
/// position. Rejects malformed, illegal, and null (`0000`) moves, naming the ply.
pub fn play_moves(
    pos: &Chess,
    moves: &[&str],
    flavor: CastlingMode,
) -> Result<(Vec<String>, Chess)> {
    let mut cur = pos.clone();
    let mut out = Vec::with_capacity(moves.len());
    for (i, m) in moves.iter().enumerate() {
        let ply = i + 1;
        let uci = UciMove::from_ascii(m.as_bytes())
            .with_context(|| format!("ply {ply}: malformed UCI move {m:?}"))?;
        if matches!(uci, UciMove::Null) {
            bail!("ply {ply}: null move 0000 is not a game move");
        }
        let mv = uci
            .to_move(&cur)
            .map_err(|_| anyhow::anyhow!("ply {ply}: illegal move {m:?}"))?;
        if let (CastlingMode::Standard, Move::Castle { king, rook }) = (flavor, &mv) {
            let home = cur.turn().fold_wb(Square::E1, Square::E8);
            let rook_files = [File::A, File::H];
            if *king != home || !rook_files.contains(&rook.file()) {
                bail!(
                    "ply {ply}: castling {m:?} cannot be spelled in the standard flavor \
                     (king not on e1/e8 or rook not on a/h); use the Chess960 flavor"
                );
            }
        }
        out.push(UciMove::from_move(mv, flavor).to_string());
        cur = cur
            .play(mv)
            .map_err(|_| anyhow::anyhow!("ply {ply}: illegal move {m:?}"))?;
    }
    Ok((out, cur))
}

/// Parse a space-separated UCI move list played from `pos`.
pub fn parse_move_list(pos: &Chess, text: &str, flavor: CastlingMode) -> Result<Vec<String>> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    Ok(play_moves(pos, &tokens, flavor)?.0)
}

pub fn write_move_list(moves: &[String]) -> String {
    moves.join(" ")
}

/// Convert a move list between castling flavors, validating along the way.
pub fn convert_flavor(pos: &Chess, moves: &[&str], to: CastlingMode) -> Result<Vec<String>> {
    Ok(play_moves(pos, moves, to)?.0)
}

fn start_position(start: &Start) -> Result<(Chess, CastlingMode)> {
    let fen = match start {
        Start::Startpos => START_FEN,
        Start::Fen(f) => f.as_str(),
    };
    let parsed = parse_fen(fen)?;
    Ok((parsed.position, parsed.mode))
}

/// Parse `position startpos [moves ...]` or `position fen <fen> [moves ...]`.
/// The FEN may have four or six fields; `moves` introduces the move list.
pub fn parse_position_command(line: &str, flavor: CastlingMode) -> Result<PositionCommand> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    ensure!(
        tokens.first() == Some(&"position"),
        "UCI position command must start with 'position'"
    );
    let moves_at = tokens.iter().position(|t| *t == "moves");
    let (head, moves) = match moves_at {
        Some(i) => (&tokens[1..i], &tokens[i + 1..]),
        None => (&tokens[1..], &tokens[0..0]),
    };
    let start = match head.first() {
        Some(&"startpos") => {
            ensure!(head.len() == 1, "unexpected tokens after 'startpos'");
            Start::Startpos
        }
        Some(&"fen") => {
            ensure!(head.len() > 1, "'position fen' needs a FEN");
            Start::Fen(head[1..].join(" "))
        }
        Some(other) => bail!("expected 'startpos' or 'fen', found {other:?}"),
        None => bail!("position command has no start ('startpos' or 'fen')"),
    };
    let (pos, _) = start_position(&start)?;
    let moves = play_moves(&pos, moves, flavor)?.0;
    Ok(PositionCommand { start, moves })
}

/// Write a validated `position` command. A FEN equal to the standard start is
/// written as `startpos`.
pub fn write_position_command(cmd: &PositionCommand) -> Result<String> {
    let (pos, _) = start_position(&cmd.start)?;
    let mut out = match &cmd.start {
        Start::Startpos => "position startpos".to_owned(),
        Start::Fen(f) => {
            if f.split_whitespace()
                .take(4)
                .eq(START_FEN.split_whitespace().take(4))
            {
                "position startpos".to_owned()
            } else {
                format!("position fen {f}")
            }
        }
    };
    let refs: Vec<&str> = cmd.moves.iter().map(String::as_str).collect();
    play_moves(&pos, &refs, CastlingMode::Chess960).context("position command moves")?;
    if !cmd.moves.is_empty() {
        out.push_str(" moves ");
        out.push_str(&write_move_list(&cmd.moves));
    }
    Ok(out)
}

/// Export a game record as a `position` command in the given flavor.
/// A Chess960 record should be exported with `CastlingMode::Chess960`.
pub fn export_position_command(record: &GameRecord, flavor: CastlingMode) -> Result<String> {
    let parsed = parse_fen(&record.initial_fen)?;
    let moves = parse_move_list(&parsed.position, &record.moves.join(" "), flavor)?;
    let fen = write_fen(
        &parsed.position,
        if parsed.mode.is_standard() {
            CastlingStyle::XFen
        } else {
            CastlingStyle::Shredder
        },
    );
    write_position_command(&PositionCommand {
        start: Start::Fen(fen),
        moves,
    })
}

/// Import a `position` command as a game record. History completeness is
/// asserted only for `startpos`: a FEN start never proves earlier history.
pub fn import_position_command(
    line: &str,
    game_id: &str,
    flavor: CastlingMode,
) -> Result<GameRecord> {
    let cmd = parse_position_command(line, flavor)?;
    let (initial_fen, complete, variant) = match &cmd.start {
        Start::Startpos => (START_FEN.to_owned(), true, "standard"),
        Start::Fen(f) => {
            let parsed = parse_fen(f)?;
            let variant = if parsed.mode.is_standard() {
                "standard"
            } else {
                "chess960"
            };
            (f.clone(), false, variant)
        }
    };
    Ok(GameRecord {
        schema_version: SCHEMA_VERSION,
        game_id: game_id.to_owned(),
        source: "uci".to_owned(),
        initial_fen,
        history_complete: complete,
        moves: cmd.moves,
        variant: variant.to_owned(),
    })
}
