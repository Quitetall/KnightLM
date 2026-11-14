use anyhow::{Context, Result};
use knightlm_cli::score::{rank_positions, score_moves};
use knightlm_engine::{Engine, EngineOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;

/// Shared driver for `score-moves` (`lines` is `None`) and `rank-positions`.
pub fn run(
    engine: Option<String>,
    depth: u32,
    lines: Option<u32>,
    input: Option<PathBuf>,
) -> Result<()> {
    let path = engine
        .or_else(|| std::env::var("STOCKFISH_PATH").ok())
        .context("--engine or STOCKFISH_PATH")?;
    // One thread and a fixed hash keep a fixed-depth search repeatable.
    let mut engine = Engine::spawn_with(
        &path,
        600,
        EngineOptions {
            threads: Some(1),
            hash_mb: Some(64),
            ..Default::default()
        },
    )?;
    let reader: Box<dyn BufRead> = match input {
        Some(p) => Box::new(BufReader::new(
            std::fs::File::open(&p).with_context(|| p.display().to_string())?,
        )),
        None => Box::new(BufReader::new(std::io::stdin())),
    };
    let mut out = BufWriter::new(std::io::stdout().lock());
    match lines {
        None => score_moves(&mut engine, reader, &mut out, depth)?,
        Some(n) => rank_positions(&mut engine, reader, &mut out, depth, n)?,
    };
    out.flush()?;
    Ok(())
}
