use anyhow::{Context, Result, ensure};
use knightlm_core::state::{GameState, PacketOptions};
use knightlm_notation::{pgn, reader};
use serde_json::json;
use std::io::{Read, Write};

// One midgame decision per game keeps the pilot small and avoids shared openings
// dominating the scored positions. `ply` counts moves already played.
const PLY: usize = 24;

pub fn write_candidates(
    input: impl Read,
    mut output: impl Write,
    source: &str,
    limit: usize,
) -> Result<()> {
    ensure!(limit > 0, "candidate limit must be positive");
    let mut count = 0;
    for game in pgn::read_games(input) {
        let game = game?;
        let tag = |name| game.tags.get(name).map(String::as_str).unwrap_or("");
        if !matches!(tag("Event"), "Rated Blitz game" | "Rated Rapid game")
            || !matches!(tag("Variant"), "" | "Standard")
            || !reader::history_complete_for(&game)
            || !matches!(game.result.as_deref(), Some("1-0" | "0-1" | "1/2-1/2"))
            || game.movetext.len() <= PLY
            || ["WhiteElo", "BlackElo"]
                .iter()
                .any(|name| tag(name).parse::<u16>().unwrap_or(0) < 1600)
        {
            continue;
        }
        let eco = tag("ECO").as_bytes();
        if !matches!(eco, [b'A'..=b'E', b'0'..=b'9', b'0'..=b'9']) {
            continue;
        }
        let id = tag("Site")
            .strip_prefix("https://lichess.org/")
            .filter(|id| id.len() == 8 && id.bytes().all(|byte| byte.is_ascii_alphanumeric()))
            .context("eligible game is missing its Lichess ID")?;
        let mut record = reader::to_game_record(&game, source, id)?;
        let target = record.moves[PLY].clone();
        record.moves.truncate(PLY);
        let state = GameState::replay(&record)?;
        let packet = state.compile(PacketOptions {
            legal_choices: true,
            derived_features: false,
        });
        ensure!(
            packet
                .legal_moves
                .as_ref()
                .is_some_and(|moves| moves.contains(&target)),
            "{id} ply {PLY}: played move {target} is absent from compiler legal choices"
        );
        serde_json::to_writer(
            &mut output,
            &json!({
                "game_id": id,
                "source": source,
                "utc_date": tag("UTCDate"),
                "utc_time": tag("UTCTime"),
                "family": &tag("ECO")[..2],
                "ply": PLY,
                "position_key": state.position_key(),
                "packet": packet,
                "target": target,
            }),
        )?;
        writeln!(output)?;
        count += 1;
        if count == limit {
            output.flush()?;
            return Ok(());
        }
    }
    anyhow::bail!("PGN ended after {count} eligible decisions, expected {limit}")
}
