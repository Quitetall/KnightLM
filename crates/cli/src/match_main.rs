use anyhow::{Context, Result};
use clap::Parser;
use knightlm_cli::match_driver::{play_game, to_pgn};
use knightlm_cli::players::{EnginePlayer, ModelPlayer, RandomLegal};
use knightlm_cli::uci::Player;

#[derive(Parser)]
#[command(name = "knightlm-match", version, about = "KnightLM headless match driver")]
struct Args {
    /// White player.
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
    /// Search depth for engine players.
    #[arg(long, default_value_t = 10)]
    depth: u32,
    #[arg(long, default_value_t = 42)]
    seed: u64,
    /// Number of games.
    #[arg(long, default_value_t = 1)]
    games: u32,
    /// Max plies per game before a draw.
    #[arg(long, default_value_t = 300)]
    max_plies: usize,
    /// Write a PGN file here (one game per line block).
    #[arg(long)]
    pgn: Option<std::path::PathBuf>,
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
            let url = args.model.clone().with_context(|| format!("{side} model player: --model URL"))?;
            Ok(Box::new(ModelPlayer::new(&url)))
        }
        other => anyhow::bail!("unknown {side} player kind: {other}"),
    }
}

fn main() -> Result<()> {
    let a = Args::parse();
    let mut pgn_out = String::new();
    let mut summary = Vec::new();
    for g in 0..a.games {
        let white = make_player(&a.white, "white", &a)?;
        let black = make_player(&a.black, "black", &a)?;
        let res = play_game(&*white, &*black, a.max_plies)?;
        let pgn = to_pgn(&res, &a.event);
        pgn_out.push_str(&pgn);
        pgn_out.push('\n');
        summary.push(format!(
            "game {}: {} {} ({}) [{}]",
            g + 1, res.result, res.white, res.black, res.termination
        ));
    }
    if let Some(path) = &a.pgn {
        std::fs::write(path, &pgn_out).with_context(|| path.display().to_string())?;
    }
    for line in summary {
        println!("{}", line);
    }
    Ok(())
}
