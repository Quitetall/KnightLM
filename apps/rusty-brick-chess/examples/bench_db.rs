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
        if len == 0 {
            0
        } else {
            (self.next() as usize) % len
        }
    }
}

fn game_pgn(rng: &mut Lcg, index: usize, max_plies: usize) -> String {
    let mut position = Chess::default();
    let mut sans = Vec::new();
    let plies = 8 + rng.pick(max_plies.max(8));
    for _ in 0..plies {
        let moves = position.legal_moves();
        if moves.is_empty() {
            break;
        }
        let mv = moves[rng.pick(moves.len())];
        sans.push(San::from_move(&position, mv).to_string());
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
    let mut turns = 0usize;
    // How long a generated game runs. Shorter games mean fewer position rows,
    // which is how a million of them fit on a disk with less room than the whole
    // corpus would need.
    let mut max_plies = 60usize;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--games" => games = args.next().unwrap_or_default().parse()?,
            "--db" => path = args.next().unwrap_or_default(),
            "--json" => json = true,
            "--turns" => turns = args.next().unwrap_or_default().parse()?,
            "--max-plies" => max_plies = args.next().unwrap_or_default().parse()?,
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
        batch.push_str(&game_pgn(&mut rng, index, max_plies));
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
        Ok(db
            .query(&GameQuery {
                result: Some("1-0".into()),
                ..Default::default()
            })?
            .games
            .len())
    })?;
    time("by player", &|| {
        Ok(db
            .query(&GameQuery {
                player: Some("Player7".into()),
                ..Default::default()
            })?
            .games
            .len())
    })?;
    time("by elo band", &|| {
        Ok(db
            .query(&GameQuery {
                elo_min: Some(2000),
                elo_max: Some(2200),
                ..Default::default()
            })?
            .games
            .len())
    })?;
    time("by date range", &|| {
        Ok(db
            .query(&GameQuery {
                date_from: Some("2000.01.01".into()),
                date_to: Some("2005.12.31".into()),
                ..Default::default()
            })?
            .games
            .len())
    })?;

    let after_e4 = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
    let started = Instant::now();
    let hits = db.search_position(after_e4)?.len();
    let search_ms = millis(started);
    let started = Instant::now();
    let explorer = db.explore(after_e4)?;
    let explore_ms = millis(started);

    // --- transcripts -----------------------------------------------------
    // A long conversation is the other thing that grows without bound: every
    // turn stores its context blob and its compiled hash. This measures writing
    // them, reading them back, and what the rows cost on disk.
    let transcript = if turns > 0 {
        use rbc_db::{AiTurnInput, SessionProfile};
        let session = db.create_ai_session(
            "bench",
            "chat",
            None,
            None,
            SessionProfile {
                model_json: r#"{"model":"bench"}"#,
                context_spec_json: r#"{"id":"chat-v1"}"#,
                permissions_json: "{}",
            },
        )?;
        let body = "x".repeat(600);
        let started = Instant::now();
        for index in 0..turns {
            db.append_ai_turn(
                &session.id,
                &AiTurnInput {
                    role: if index % 2 == 0 { "user".into() } else { "assistant".into() },
                    content: format!("{body} {index}"),
                    context_json: r#"{"position":{"fen":"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"}}"#.into(),
                    context_sha256: "0".repeat(64),
                    ..AiTurnInput::default()
                },
            )?;
        }
        let write_ms = millis(started);
        let started = Instant::now();
        let loaded = db.ai_turns(&session.id)?.len();
        let read_ms = millis(started);
        // The newest turn is what the composer needs on send.
        let started = Instant::now();
        let newest = db
            .ai_turns(&session.id)?
            .into_iter()
            .next_back()
            .map(|turn| turn.seq);
        let newest_ms = millis(started);
        Some(serde_json::json!({
            "turns": turns,
            "loaded": loaded,
            "newest_seq": newest,
            "write_ms": write_ms.round(),
            "write_per_second": (turns as f64 / (write_ms / 1000.0)).round(),
            "full_read_ms": read_ms.round(),
            "newest_read_ms": newest_ms.round(),
        }))
    } else {
        None
    };

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
        "transcript": transcript,
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "imported {} games in {:.0} ms ({:.0}/s)",
            imported,
            import_ms,
            report["games_per_second"].as_f64().unwrap_or(0.0)
        );
        for (label, ms, found) in &queries {
            println!("  query {label}: {ms:.1} ms, {found} row(s)");
        }
        println!("  position search: {search_ms:.1} ms, {hits} hit(s)");
        println!(
            "  explore: {explore_ms:.1} ms, {} continuation(s)",
            explorer.continuations.len()
        );
        println!("  database: {} bytes", report["database_bytes"]);
        if let Some(transcript) = report.get("transcript").filter(|t| !t.is_null()) {
            println!(
                "  transcript: {} turns written at {}/s, full read {} ms, newest {} ms",
                transcript["turns"],
                transcript["write_per_second"],
                transcript["full_read_ms"],
                transcript["newest_read_ms"]
            );
        }
    }
    Ok(())
}
