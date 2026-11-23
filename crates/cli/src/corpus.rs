use anyhow::{Context, Result, ensure};
use knightlm_core::state::{GameState, PacketOptions};
use knightlm_notation::descriptive::play_descriptive;
use knightlm_notation::fen::{CastlingStyle, parse_fen, write_fen};
use knightlm_notation::san::play_san;
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
            ..PacketOptions::default()
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

const COMMENT_TAGS: [&str; 8] = [
    "Event",
    "Site",
    "White",
    "Black",
    "Result",
    "StudyName",
    "ChapterName",
    "Annotator",
];

fn count_comments(nodes: &[pgn::PgnNode]) -> usize {
    nodes
        .iter()
        .map(|n| {
            usize::from(n.pre_comment.is_some())
                + usize::from(n.post_comment.is_some())
                + n.variations
                    .iter()
                    .map(|v| count_comments(v))
                    .sum::<usize>()
        })
        .sum()
}

/// Replay each PGN game's main line on `notation` and emit one JSON line per
/// comment-bearing move (`kind: "comment"`), one summary per game
/// (`kind: "game"`), and a `kind: "quarantine"` line where a move fails to
/// replay (the rest of that game is dropped: later positions are unreliable).
/// Variation comments are counted but not emitted.
pub fn write_commentary(input: impl Read, mut output: impl Write, source: &str) -> Result<()> {
    for (index, game) in pgn::read_games(input).enumerate() {
        let game = match game {
            Ok(game) => game,
            Err(error) => {
                writeln!(
                    output,
                    "{}",
                    json!({"kind": "quarantine", "game_id": format!("{source}#{index}"), "ply": null, "stage": "pgn", "error": error.to_string()})
                )?;
                break;
            }
        };
        let tags: serde_json::Map<String, serde_json::Value> = COMMENT_TAGS
            .iter()
            .filter_map(|name| game.tags.get(*name).map(|v| (name.to_string(), json!(v))))
            .collect();
        let site = game.tags.get("Site").cloned().unwrap_or_default();
        let id = if site.is_empty() {
            format!("{source}#{index}")
        } else {
            site
        };
        let start = reader::initial_fen_for(&game);
        let mut pos = match parse_fen(&start) {
            Ok(parsed) => parsed.position,
            Err(error) => {
                writeln!(
                    output,
                    "{}",
                    json!({"kind": "quarantine", "game_id": id, "ply": null, "stage": "fen", "error": error.to_string()})
                )?;
                continue;
            }
        };
        let mut played = 0usize;
        let mut mainline_comments = 0usize;
        let mut failed = false;
        if let Some(first) = game.movetext.first().and_then(|n| n.pre_comment.as_ref()) {
            mainline_comments += 1;
            writeln!(
                output,
                "{}",
                json!({"kind": "comment", "game_id": id, "ply": 0, "fen_before": write_fen(&pos, CastlingStyle::XFen), "san": null, "comment": first, "nags": [], "tags": tags, "start_fen": start})
            )?;
        }
        for (ply, node) in game.movetext.iter().enumerate() {
            let before = write_fen(&pos, CastlingStyle::XFen);
            match play_san(&pos, &node.san) {
                Ok((next, uci)) => {
                    pos = next;
                    played += 1;
                    if let Some(comment) = &node.post_comment {
                        mainline_comments += 1;
                        writeln!(
                            output,
                            "{}",
                            json!({"kind": "comment", "game_id": id, "ply": ply + 1, "fen_before": before, "fen_after": write_fen(&pos, CastlingStyle::XFen), "san": node.san, "uci": uci, "comment": comment, "nags": node.nags, "tags": tags, "start_fen": start})
                        )?;
                    }
                }
                Err(error) => {
                    failed = true;
                    writeln!(
                        output,
                        "{}",
                        json!({"kind": "quarantine", "game_id": id, "ply": ply + 1, "stage": "replay", "san": node.san, "fen_before": before, "error": error.to_string()})
                    )?;
                    break;
                }
            }
        }
        writeln!(
            output,
            "{}",
            json!({"kind": "game", "game_id": id, "plies": game.movetext.len(), "replayed": played, "failed": failed, "mainline_comments": mainline_comments, "all_comments": count_comments(&game.movetext)})
        )?;
    }
    output.flush()?;
    Ok(())
}

/// Replay SAN lines: stdin is JSONL `{"id", "fen"?, "sans": [...]}`; stdout is
/// one JSON line per input with the validated FEN after each SAN played and,
/// on failure, the zero-based index and reason of the first move that fails.
pub fn replay_lines(input: impl Read, mut output: impl Write) -> Result<()> {
    use std::io::BufRead;
    for line in std::io::BufReader::new(input).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request: serde_json::Value = serde_json::from_str(&line).context("bad request line")?;
        let id = request["id"].clone();
        let start = request["fen"]
            .as_str()
            .unwrap_or(knightlm_core::state::START_FEN);
        let sans: Vec<&str> = request["sans"]
            .as_array()
            .context("sans must be an array")?
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        let descriptive = request["notation"].as_str() == Some("descriptive");
        let mut reply =
            json!({"id": id, "fen": start, "ok": true, "played": 0, "fens": [], "ucis": []});
        match parse_fen(start) {
            Err(error) => {
                reply["ok"] = json!(false);
                reply["error"] = json!(error.to_string());
                reply["failed_at"] = json!(null);
            }
            Ok(parsed) => {
                let mut pos = parsed.position;
                let (mut fens, mut ucis) = (Vec::new(), Vec::new());
                for (index, san) in sans.iter().enumerate() {
                    let played = if descriptive {
                        play_descriptive(&pos, san)
                    } else {
                        play_san(&pos, san)
                    };
                    match played {
                        Ok((next, uci)) => {
                            pos = next;
                            fens.push(write_fen(&pos, CastlingStyle::XFen));
                            ucis.push(uci);
                        }
                        Err(error) => {
                            reply["ok"] = json!(false);
                            reply["failed_at"] = json!(index);
                            reply["error"] = json!(error.to_string());
                            break;
                        }
                    }
                }
                reply["played"] = json!(ucis.len());
                reply["fens"] = json!(fens);
                reply["ucis"] = json!(ucis);
            }
        }
        writeln!(output, "{reply}")?;
    }
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commentary(pgn: &str) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        write_commentary(pgn.as_bytes(), &mut out, "t.pgn").unwrap();
        String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    #[test]
    fn comment_row_carries_validated_fens() {
        let rows = commentary("[Event \"x\"]\n\n1. e4 {Best by test.} e5 2. Nf3 *\n");
        assert_eq!(rows[0]["kind"], "comment");
        assert_eq!(rows[0]["san"], "e4");
        assert_eq!(rows[0]["uci"], "e2e4");
        assert!(
            rows[0]["fen_after"]
                .as_str()
                .unwrap()
                .starts_with("rnbqkbnr/pppppppp/8/8/4P3")
        );
        assert_eq!(rows.last().unwrap()["replayed"], 3);
    }

    #[test]
    fn corrupted_move_is_quarantined_and_game_stops() {
        let rows = commentary("[Event \"x\"]\n\n1. e4 {ok} e5 2. Ke7 {bad} Nc6 3. Nf3 {never} *\n");
        let kinds: Vec<_> = rows.iter().map(|r| r["kind"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["comment", "quarantine", "game"]);
        assert_eq!(rows[1]["ply"], 3);
        assert_eq!(rows[1]["san"], "Ke7");
        assert_eq!(rows[2]["failed"], true);
    }

    #[test]
    fn replay_lines_reports_first_failure() {
        let mut out = Vec::new();
        let input =
            "{\"id\":1,\"sans\":[\"e4\",\"e5\",\"Qh8\"]}\n{\"id\":2,\"fen\":\"bad\",\"sans\":[]}\n";
        replay_lines(input.as_bytes(), &mut out).unwrap();
        let rows: Vec<serde_json::Value> = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(
            (
                rows[0]["ok"].as_bool(),
                rows[0]["failed_at"].as_u64(),
                rows[0]["played"].as_u64()
            ),
            (Some(false), Some(2), Some(2))
        );
        assert_eq!(rows[1]["ok"], false);
    }
}
