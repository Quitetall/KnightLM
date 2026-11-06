//! Measure a database at scale: import throughput, the query surface, position
//! search and the explorer.
//!
//! ```text
//! cargo run --release --example bench_db -- --games 100000 --db /tmp/bench.db
//! ```
//!
//! The games are synthetic and deterministic: a seeded generator plays legal
//! moves and writes real PGN, imported through the same path a person's file
//! takes. That matters for the timings - they run against a database built by
//! the importer, not one written by hand.

use std::time::Instant;

use rbc_db::{Db, GameQuery};
use shakmaty::san::San;
use shakmaty::{CastlingMode, Chess, Position};

/// A small deterministic generator: a benchmark that is not repeatable is an
/// anecdote.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn pick(&mut self, len: usize) -> usize {
        if len == 0 { 0 } else { (self.next() as usize) % len }
    }
}

fn game_pgn(rng: &mut Lcg, index: usize) -> String {
    let mut position = Chess::default();
    let mut sans = Vec::new();
    let plies = 8 + rng.pick(60);
    for _ in 0..plies {
        let moves = position.legal_moves();
        if moves.is_empty() {
            break;
        }
        let mv = moves[rng.pick(moves.len())].clone();
        sans.push(San::from_move(&position, mv.clone()).to_string());
        position = position.play(mv).expect("legal");
    }
    let result = match index % 3 {
        0 => "1-0",
        1 => "0-1",
        _ => "1/2-1/2",
    };
    let elo_white = 1200 + (rng.next() % 1600) as i32;
    let elo_black = 1200 + (rng.next() % 1600) as i32;
    let year = 1990 + (index % 35);
    let mut movetext = String::new();
    for (ply, san) in sans.iter().enumerate() {
        if ply % 2 == 0 {
            movetext.push_str(&format!("{}. ", ply / 2 + 1));
        }
        movetext.push_str(san);
        movetext.push(' ');
    }
    movetext.push_str(result);
    let _ = CastlingMode::Standard;
    format!(
        "[Event \"Synthetic {}\"]\n[White \"Player{}\"]\n[Black \"Player{}\"]\n[Result \"{}\"]\n\
         [WhiteElo \"{}\"]\n[BlackElo \"{}\"]\n[Date \"{}.{:02}.{:02}\"]\n\n{}\n",
        index % 50,
        index % 997,
        (index * 7) % 991,
        result,
        elo_white,
        elo_black,
        year,
        1 + index % 12,
        1 + index % 28,
        movetext,
    )
}

fn millis(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut games = 10_000usize;
    let mut path = "/tmp/rbc-bench.db".to_string();
    let mut json = false;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--games" => games = args.next().unwrap_or_default().parse()?,
            "--db" => path = args.next().unwrap_or_default(),
            "--json" => json = true,
            _ => {}
        }
    }
    let _ = std::fs::remove_file(&path);
    let db = Db::open(&path)?;
    let mut rng = Lcg(0x9E37_79B9_7F4A_7C15);

    let started = Instant::now();
    let mut batch = String::new();
    let mut batch_games = 0;
    let mut imported = 0;
    for index in 0..games {
        batch.push_str(&game_pgn(&mut rng, index));
        batch.push('\n');
        batch_games += 1;
        if batch_games == 500 {
            imported += db.import_pgn_lenient(&batch, None)?.0.len();
            batch.clear();
            batch_games = 0;
        }
    }
    if !batch.is_empty() {
        imported += db.import_pgn_lenient(&batch, None)?.0.len();
    }
    let import_ms = millis(started);

    let mut queries: Vec<(String, f64, usize)> = Vec::new();
    let mut time = |label: &str, run: &dyn Fn() -> anyhow::Result<usize>| -> anyhow::Result<()> {
        let started = Instant::now();
        let found = run()?;
        queries.push((label.to_string(), millis(started), found));
        Ok(())
    };
    time("no filter (first page)", &|| {
        Ok(db.query(&GameQuery::default())?.games.len())
    })?;
    time("by result", &|| {
        Ok(db.query(&GameQuery { result: Some("1-0".into()), ..Default::default() })?.games.len())
    })?;
    time("by player", &|| {
        Ok(db.query(&GameQuery { player: Some("Player7".into()), ..Default::default() })?.games.len())
    })?;
    time("by elo band", &|| {
        Ok(db.query(&GameQuery { elo_min: Some(2000), elo_max: Some(2200), ..Default::default() })?.games.len())
    })?;
    time("by date range", &|| {
        Ok(db.query(&GameQuery {
            date_from: Some("2000.01.01".into()),
            date_to: Some("2005.12.31".into()),
            ..Default::default()
        })?.games.len())
    })?;

    let after_e4 = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
    let started = Instant::now();
    let hits = db.search_position(after_e4)?.len();
    let search_ms = millis(started);
    let started = Instant::now();
    let explorer = db.explore(after_e4)?;
    let explore_ms = millis(started);

    let report = serde_json::json!({
        "games_requested": games,
        "games_imported": imported,
        "import_ms": import_ms.round(),
        "games_per_second": (imported as f64 / (import_ms / 1000.0)).round(),
        "queries": queries.iter().map(|(label, ms, found)| serde_json::json!({
            "label": label, "ms": ms.round(), "rows": found
        })).collect::<Vec<_>>(),
        "position_search": { "ms": search_ms.round(), "hits": hits },
        "explore": { "ms": explore_ms.round(), "continuations": explorer.continuations.len() },
        "database_bytes": std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "imported {} games in {:.0} ms ({:.0}/s)",
            imported, import_ms, report["games_per_second"].as_f64().unwrap_or(0.0)
        );
        for (label, ms, found) in &queries {
            println!("  query {label}: {ms:.1} ms, {found} row(s)");
        }
        println!("  position search: {search_ms:.1} ms, {hits} hit(s)");
        println!("  explore: {explore_ms:.1} ms, {} continuation(s)", explorer.continuations.len());
        println!("  database: {} bytes", report["database_bytes"]);
    }
    Ok(())
}
