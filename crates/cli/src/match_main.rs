use anyhow::{Context, Result};
use clap::Parser;
use knightlm_cli::match_driver::{MatchOptions, MatchResult, play_game_with, to_pgn};
use knightlm_cli::players::{EnginePlayer, ModelPlayer, RandomLegal};
use knightlm_cli::uci::Player;
use serde_json::json;
use std::collections::BTreeMap;

#[derive(Parser)]
#[command(
    name = "knightlm-match",
    version,
    about = "KnightLM headless match driver"
)]
struct Args {
    /// White player (random, engine, model). With --swap-colors, the "subject"
    /// is the first player here that is not an engine, else white.
    #[arg(long, default_value = "random")]
    white: String,
    /// Black player.
    #[arg(long, default_value = "random")]
    black: String,
    /// Engine binary path (used by engine players).
    #[arg(long)]
    engine: Option<String>,
    /// Model server URL (used by model players).
    #[arg(long)]
    model: Option<String>,
    /// Per-request model timeout in seconds; a timeout forfeits the game.
    #[arg(long, default_value_t = 120)]
    model_timeout: u64,
    /// Search depth for engine players.
    #[arg(long, default_value_t = 10)]
    depth: u32,
    #[arg(long, default_value_t = 42)]
    seed: u64,
    /// Number of games.
    #[arg(long, default_value_t = 1)]
    games: u32,
    /// Max plies per game (opening included) before a draw.
    #[arg(long, default_value_t = 300)]
    max_plies: usize,
    /// Fixed opening set: one line per opening, UCI moves separated by spaces
    /// (blank lines and `#` comments ignored). Game g uses opening
    /// (g / colors) % len, where colors is 2 with --swap-colors, else 1.
    #[arg(long)]
    openings: Option<std::path::PathBuf>,
    /// Odd-numbered games exchange colors so each opening is played both ways.
    #[arg(long)]
    swap_colors: bool,
    /// Draw at once when a threefold-repetition or fifty-move claim is available.
    #[arg(long)]
    claim_draws: bool,
    /// A player error (HTTP failure, timeout, illegal move) forfeits the game
    /// instead of aborting the match.
    #[arg(long)]
    forfeit_on_error: bool,
    /// Write a PGN file here (one game per line block).
    #[arg(long)]
    pgn: Option<std::path::PathBuf>,
    /// Write per-game records and per-termination aggregates as JSON.
    #[arg(long)]
    summary_json: Option<std::path::PathBuf>,
    #[arg(long, default_value = "KnightLM match")]
    event: String,
}

fn make_player(kind: &str, side: &str, args: &Args) -> Result<Box<dyn Player>> {
    match kind {
        "random" => Ok(Box::new(RandomLegal { seed: args.seed })),
        "engine" => {
            let path = args
                .engine
                .clone()
                .or_else(|| std::env::var("STOCKFISH_PATH").ok())
                .with_context(|| format!("{side} engine player: --engine or STOCKFISH_PATH"))?;
            Ok(Box::new(EnginePlayer::spawn(&path, args.depth, 120)?))
        }
        "model" => {
            let url = args
                .model
                .clone()
                .with_context(|| format!("{side} model player: --model URL"))?;
            Ok(Box::new(ModelPlayer::with_timeout(
                &url,
                args.model_timeout,
            )))
        }
        other => anyhow::bail!("unknown {side} player kind: {other}"),
    }
}

fn read_openings(path: &std::path::Path) -> Result<Vec<Vec<String>>> {
    let text = std::fs::read_to_string(path).with_context(|| path.display().to_string())?;
    Ok(text
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .map(|l| l.split_whitespace().map(str::to_owned).collect())
        .collect())
}

/// Outcome from the subject's side: `win`, `draw` or `loss`.
fn outcome(result: &MatchResult, subject_white: bool) -> &'static str {
    match (result.result.as_str(), subject_white) {
        ("1-0", true) | ("0-1", false) => "win",
        ("1-0", false) | ("0-1", true) => "loss",
        _ => "draw",
    }
}

/// The termination with "by white/black" rewritten to "by subject/opponent" so
/// aggregates do not depend on which color the subject held.
fn termination_kind(result: &MatchResult, subject_white: bool) -> String {
    let t = &result.termination;
    let (subject, other) = if subject_white {
        ("white", "black")
    } else {
        ("black", "white")
    };
    t.replace(&format!("by {subject}"), "by subject")
        .replace(&format!("by {other}"), "by opponent")
}

fn main() -> Result<()> {
    let a = Args::parse();
    let openings = match &a.openings {
        Some(p) => read_openings(p)?,
        None => vec![vec![]],
    };
    anyhow::ensure!(!openings.is_empty(), "opening file has no openings");
    let colors = if a.swap_colors { 2 } else { 1 };
    let subject_is_white = a.white != "engine";
    let mut pgn_out = String::new();
    let mut summary = Vec::new();
    let mut games = Vec::new();
    let mut table: BTreeMap<String, BTreeMap<&'static str, u32>> = BTreeMap::new();
    for g in 0..a.games {
        let opening_idx = (g / colors) as usize % openings.len();
        let swapped = a.swap_colors && g % 2 == 1;
        let (wk, bk) = if swapped {
            (&a.black, &a.white)
        } else {
            (&a.white, &a.black)
        };
        let white = make_player(wk, "white", &a)?;
        let black = make_player(bk, "black", &a)?;
        let res = play_game_with(
            &*white,
            &*black,
            &MatchOptions {
                max_plies: a.max_plies,
                opening: openings[opening_idx].clone(),
                claim_draws: a.claim_draws,
                forfeit_on_error: a.forfeit_on_error,
            },
        )?;
        let subject_white = subject_is_white != swapped;
        let oc = outcome(&res, subject_white);
        let kind = termination_kind(&res, subject_white);
        *table
            .entry(kind.clone())
            .or_default()
            .entry(oc)
            .or_default() += 1;
        games.push(json!({
            "game": g + 1, "opening": opening_idx, "subject_color": if subject_white {"white"} else {"black"},
            "result": res.result, "outcome": oc, "termination": kind, "plies": res.moves.len(),
            "failure": res.failure, "moves": res.moves,
        }));
        pgn_out.push_str(&to_pgn(&res, &a.event));
        pgn_out.push('\n');
        summary.push(format!(
            "game {}: {} {} ({}) [{}]",
            g + 1,
            res.result,
            res.white,
            res.black,
            res.termination
        ));
    }
    if let Some(path) = &a.pgn {
        std::fs::write(path, &pgn_out).with_context(|| path.display().to_string())?;
    }
    if let Some(path) = &a.summary_json {
        let count = |o: &str| {
            table
                .values()
                .map(|m| m.get(o).copied().unwrap_or(0))
                .sum::<u32>()
        };
        let (w, d, l) = (count("win"), count("draw"), count("loss"));
        let out = json!({
            "schema": "knightlm-match-summary/v1",
            "subject": {"white": a.white, "black": a.black},
            "games": a.games, "openings": openings.len(), "swap_colors": a.swap_colors,
            "claim_draws": a.claim_draws, "max_plies": a.max_plies, "engine_depth": a.depth,
            "wins": w, "draws": d, "losses": l,
            "score": if a.games > 0 { (w as f64 + 0.5 * d as f64) / a.games as f64 } else { 0.0 },
            "by_termination": table,
            "per_game": games,
        });
        std::fs::write(path, serde_json::to_string_pretty(&out)? + "\n")
            .with_context(|| path.display().to_string())?;
    }
    for line in summary {
        println!("{}", line);
    }
    Ok(())
}
