//! Streaming importer for the monthly Lichess PGN dumps.
//!
//! The dumps are tens of gigabytes compressed and contain tens of millions of
//! games, so the importer never loads the file: it decompresses a chunk at a
//! time, splits on game boundaries, filters on the tag pairs and imports only
//! the games that match.

use anyhow::{Context, Result, bail};
use rbc_db::Db;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, Default, Clone)]
pub struct DumpFilters {
    /// Match White or Black (case-insensitive) exactly.
    pub player: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub min_elo: Option<i32>,
    pub max_elo: Option<i32>,
    pub result: Option<String>,
    pub time_control: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DumpOptions {
    pub file: PathBuf,
    pub db: PathBuf,
    /// Stop after importing this many games (0 = no limit).
    pub max_games: usize,
    /// Keep the per-ply position index (needed for position search).
    pub index_positions: bool,
    /// Commit every N games.
    pub batch: usize,
}

impl DumpOptions {
    pub fn new(file: impl Into<PathBuf>, db: impl Into<PathBuf>) -> Self {
        Self {
            file: file.into(),
            db: db.into(),
            max_games: 0,
            index_positions: false,
            batch: 2000,
        }
    }
}

pub fn import_dump(options: &DumpOptions, filters: &DumpFilters) -> Result<()> {
    if !options.file.exists() {
        bail!("no such file: {}", options.file.display());
    }
    if let Some(parent) = options.db.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let db_path = options.db.display().to_string();
    let db = Db::open(&db_path).context("open import database")?;

    let file = std::fs::File::open(&options.file)
        .with_context(|| format!("open {}", options.file.display()))?;
    let decoder = zstd::stream::read::Decoder::new(std::io::BufReader::with_capacity(
        1 << 20,
        file,
    ))
    .context("zstd decoder")?;
    let mut reader = std::io::BufReader::with_capacity(1 << 20, decoder);

    let mut buffer: Vec<u8> = Vec::with_capacity(1 << 20);
    let mut chunk = vec![0u8; 1 << 20];
    let started = Instant::now();
    let mut last_report = Instant::now();
    let mut seen: u64 = 0;
    let mut matched: u64 = 0;
    let mut imported: u64 = 0;
    let mut skipped: u64 = 0;
    let mut pending = 0usize;
    let mut done = false;

    db.begin_bulk()?;
    while !done {
        let read = reader.read(&mut chunk).context("read dump")?;
        if read == 0 {
            done = true;
        } else {
            buffer.extend_from_slice(&chunk[..read]);
        }

        // Games are separated by a blank line before the next `[Event `.
        loop {
            let Some(index) = find(&buffer, b"\n\n[Event ") else {
                break;
            };
            let game = String::from_utf8_lossy(&buffer[..index]).into_owned();
            buffer.drain(..index + 2);
            seen += 1;
            if !matches(&game, filters) {
                continue;
            }
            matched += 1;
            match db.import_pgn_lenient_opts(&game, None, options.index_positions) {
                Ok((_ids, misses)) => {
                    imported += 1;
                    skipped += misses as u64;
                    pending += 1;
                }
                Err(_) => skipped += 1,
            }
            if options.max_games > 0 && imported as usize >= options.max_games {
                done = true;
                break;
            }
            if pending >= options.batch {
                db.commit_bulk()?;
                db.begin_bulk()?;
                pending = 0;
            }
        }
        if done && !buffer.is_empty() {
            let game = String::from_utf8_lossy(&buffer).into_owned();
            buffer.clear();
            seen += 1;
            let under_cap = options.max_games == 0 || (imported as usize) < options.max_games;
            if under_cap && matches(&game, filters) {
                matched += 1;
                if let Ok((_ids, misses)) =
                    db.import_pgn_lenient_opts(&game, None, options.index_positions)
                {
                    imported += 1;
                    skipped += misses as u64;
                } else {
                    skipped += 1;
                }
            }
        }
        if last_report.elapsed() >= Duration::from_secs(5) {
            let elapsed = started.elapsed().as_secs_f64().max(0.001);
            eprintln!(
                "  seen {seen:>10}  imported {imported:>9}  ({:.0} games/s, {:.0} MB/s)",
                imported as f64 / elapsed,
                (seen as f64 * 2600.0) / elapsed / 1_048_576.0,
            );
            last_report = Instant::now();
        }
    }
    db.commit_bulk()?;

    let elapsed = started.elapsed();
    let size = std::fs::metadata(&options.db).map(|m| m.len()).unwrap_or(0);
    println!(
        "imported {imported} of {matched} matching games ({seen} scanned, {skipped} skipped) in {:.1} min",
        elapsed.as_secs_f64() / 60.0
    );
    println!(
        "database: {} ({:.1} GB)",
        options.db.display(),
        size as f64 / 1_073_741_824.0
    );
    Ok(())
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn matches(game: &str, filters: &DumpFilters) -> bool {
    let header_end = game.find("\n\n1.").or_else(|| game.find("\n\n1...")).unwrap_or(game.len());
    let header = &game[..header_end];
    let tag = |name: &str| -> Option<&str> {
        let needle = format!("[{name} \"");
        let start = header.find(&needle)? + needle.len();
        let rest = &header[start..];
        let end = rest.find('"')?;
        Some(&rest[..end])
    };
    if let Some(player) = filters.player.as_deref().filter(|p| !p.is_empty()) {
        let white = tag("White").unwrap_or_default();
        let black = tag("Black").unwrap_or_default();
        if !white.eq_ignore_ascii_case(player) && !black.eq_ignore_ascii_case(player) {
            return false;
        }
    }
    if let Some(from) = filters.date_from.as_deref().filter(|d| !d.is_empty()) {
        let date = tag("Date").unwrap_or_default().replace('-', ".");
        let from = from.replace('-', ".");
        if date < from {
            return false;
        }
    }
    if let Some(to) = filters.date_to.as_deref().filter(|d| !d.is_empty()) {
        let date = tag("Date").unwrap_or_default().replace('-', ".");
        let to = to.replace('-', ".");
        if date > to {
            return false;
        }
    }
    let elo = |name: &str| tag(name).and_then(|v| v.trim().parse::<i32>().ok());
    if let Some(min) = filters.min_elo {
        let in_range = [elo("WhiteElo"), elo("BlackElo")]
            .iter()
            .flatten()
            .any(|value| *value >= min);
        if !in_range {
            return false;
        }
    }
    if let Some(max) = filters.max_elo {
        let in_range = [elo("WhiteElo"), elo("BlackElo")]
            .iter()
            .flatten()
            .any(|value| *value <= max);
        if !in_range {
            return false;
        }
    }
    if let Some(result) = filters.result.as_deref().filter(|r| !r.is_empty()) {
        if tag("Result").unwrap_or_default() != result {
            return false;
        }
    }
    if let Some(tc) = filters.time_control.as_deref().filter(|t| !t.is_empty()) {
        if tag("TimeControl").unwrap_or_default() != tc {
            return false;
        }
    }
    true
}

/// Path of the example slice created from the bundled dump location.
pub fn default_slice_path(data_dir: &Path) -> PathBuf {
    data_dir.join("september-2026-slice.db")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbc_db::GameQuery;

    fn sample_pgn() -> String {
        let game = |white: &str, black: &str, date: &str, elo: u32, moves: &str| {
            format!(
                "[Event \"Rated Blitz game\"]\n[Site \"https://lichess.org/x\"]\n[Date \"{date}\"]\n[White \"{white}\"]\n[Black \"{black}\"]\n[Result \"1-0\"]\n[WhiteElo \"{elo}\"]\n[BlackElo \"{elo}\"]\n[TimeControl \"300+0\"]\n\n{moves}\n\n"
            )
        };
        format!(
            "{}{}{}",
            game("Alpha", "Beta", "2026.09.01", 1800, "1. e4 e5 2. Nf3 Nc6 1-0"),
            game("Gamma", "Alpha", "2026.09.02", 2100, "1. d4 d5 2. c4 e6 1-0"),
            game("Delta", "Epsilon", "2026.08.15", 1200, "1. c4 e5 1-0"),
        )
    }

    fn write_dump(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("rbc-import-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dump = dir.join("slice.pgn.zst");
        let compressed = zstd::stream::encode_all(sample_pgn().as_bytes(), 3).unwrap();
        std::fs::write(&dump, compressed).unwrap();
        let db = dir.join("out.db");
        (dump, db)
    }

    #[test]
    fn imports_a_capped_slice() {
        let (dump, db_path) = write_dump("cap");
        let mut options = DumpOptions::new(&dump, &db_path);
        options.max_games = 2;
        import_dump(&options, &DumpFilters::default()).unwrap();

        let db = Db::open(&db_path.display().to_string()).unwrap();
        let all = db.query(&GameQuery::default()).unwrap();
        assert_eq!(all.total, 2);
    }

    #[test]
    fn filters_by_player_and_date() {
        let (dump, db_path) = write_dump("player");
        let mut options = DumpOptions::new(&dump, &db_path);
        options.max_games = 0;
        let filters = DumpFilters {
            player: Some("alpha".into()),
            date_from: Some("2026-09-01".into()),
            ..DumpFilters::default()
        };
        import_dump(&options, &filters).unwrap();

        let db = Db::open(&db_path.display().to_string()).unwrap();
        let all = db.query(&GameQuery::default()).unwrap();
        assert_eq!(all.total, 2);
        assert!(all.games.iter().all(|g| g.white == "Alpha" || g.black == "Alpha"));
    }

    #[test]
    fn indexing_makes_positions_searchable() {
        let (dump, db_path) = write_dump("index");
        let mut options = DumpOptions::new(&dump, &db_path);
        options.max_games = 0;
        options.index_positions = true;
        import_dump(&options, &DumpFilters::default()).unwrap();

        let db = Db::open(&db_path.display().to_string()).unwrap();
        let fen = "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2";
        let hits = db.search_position_variant(fen, "standard").unwrap();
        assert!(!hits.is_empty(), "expected the e4 e5 position to be indexed");
    }
}
