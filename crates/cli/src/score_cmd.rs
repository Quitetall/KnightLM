use anyhow::{Context, Result};
use knightlm_engine::{Engine, EngineOptions};
use knightlm_play::score::{rank_positions, score_moves};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;

/// Shared driver for `score-moves` (`lines` is `None`) and `rank-positions`.
/// How the oracle is run. The Stockfish defaults (one thread, 64 MB hash, a
/// depth limit) are unchanged; Leela and Maia use a node limit, no `Hash`
/// option and their weights file through `options`.
pub struct OracleSettings {
    pub nodes: bool,
    pub threads: u32,
    pub hash_mb: u32,
    pub options: Vec<(String, String)>,
}

pub fn run(
    engine: Option<String>,
    depth: u32,
    lines: Option<u32>,
    input: Option<PathBuf>,
    settings: OracleSettings,
) -> Result<()> {
    let path = engine
        .or_else(|| std::env::var("STOCKFISH_PATH").ok())
        .context("--engine or STOCKFISH_PATH")?;
    // One thread and a fixed hash keep a fixed-depth search repeatable.
    let mut engine = Engine::spawn_with(
        &path,
        600,
        EngineOptions {
            threads: Some(settings.threads),
            hash_mb: Some(settings.hash_mb),
            extra: settings.options,
            ..Default::default()
        },
    )?;
    engine.set_node_limit(settings.nodes);
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
