use clap::Parser;
use knightlm_cli::players::{EnginePlayer, ModelPlayer, RandomLegal};
use knightlm_cli::uci::UciEngine;

#[derive(Parser)]
#[command(name = "knightlm-uci", version, about = "KnightLM UCI engine")]
struct Args {
    /// Which player backs this engine.
    #[arg(long, default_value = "random")]
    player: String,
    #[arg(long, default_value = "knightlm")]
    name: String,
    /// Engine binary path (for --player engine).
    #[arg(long)]
    engine: Option<String>,
    /// Model server URL (for --player model), e.g. http://127.0.0.1:8000/v1/move.
    #[arg(long)]
    model: Option<String>,
    /// Search depth for the engine player.
    #[arg(long, default_value_t = 10)]
    depth: u32,
    #[arg(long, default_value_t = 42)]
    seed: u64,
}

fn main() {
    let a = Args::parse();
    let author = "KnightLM";
    let engine = match a.player.as_str() {
        "engine" => {
            let path = a.engine.clone().or_else(|| std::env::var("STOCKFISH_PATH").ok())
                .expect("--engine or STOCKFISH_PATH required for --player engine");
            UciEngine::new(Box::new(EnginePlayer::spawn(&path, a.depth, 120).expect("spawn engine")), &a.name, author)
        }
        "model" => {
            let url = a.model.clone().expect("--model URL required for --player model");
            UciEngine::new(Box::new(ModelPlayer::new(&url)), &a.name, author)
        }
        _ => UciEngine::new(Box::new(RandomLegal { seed: a.seed }), &a.name, author),
    };
    let mut e = engine;
    e.run();
}
