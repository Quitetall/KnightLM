//! Game database for RustyBrickChess.
//!
//! This is a Tauri-free, SQLite-backed port of En Croissant's database module:
//! the same domain shape (`NormalizedGame`, collections, annotations, position
//! search, player/event catalogs) without `AppState`, `specta`, DuckDB or the
//! AIX extension. The public API is what the app and the frontend speak.

use anyhow::{Context, Result};
use knightlm_core::state::{parse_chess, parse_variant_fen, variant_for_name, START_FEN};
use rusqlite::types::Value;
use rusqlite::{Connection, params, params_from_iter};
use serde::{Deserialize, Serialize};
use shakmaty::{CastlingMode, Chess, EnPassantMode, Position, fen::Fen, san::SanPlus, uci::UciMove};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS games (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    event        TEXT,
    site         TEXT,
    date         TEXT,
    round        TEXT,
    white        TEXT NOT NULL,
    white_elo    INTEGER,
    black        TEXT NOT NULL,
    black_elo    INTEGER,
    result       TEXT NOT NULL,
    fen          TEXT NOT NULL,
    moves        TEXT NOT NULL,
    movetext     TEXT,
    ply_count    INTEGER NOT NULL,
    time_control TEXT,
    other        TEXT,
    variant      TEXT,
    added_at     TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_games_white  ON games(white);
CREATE INDEX IF NOT EXISTS idx_games_black  ON games(black);
CREATE INDEX IF NOT EXISTS idx_games_event  ON games(event);
CREATE INDEX IF NOT EXISTS idx_games_result ON games(result);

CREATE TABLE IF NOT EXISTS positions (
    game_id INTEGER NOT NULL,
    ply     INTEGER NOT NULL,
    key     TEXT NOT NULL,
    uci     TEXT,
    PRIMARY KEY (game_id, ply)
);
CREATE INDEX IF NOT EXISTS idx_positions_key ON positions(key);

CREATE TABLE IF NOT EXISTS puzzles (
    id                TEXT PRIMARY KEY,
    fen               TEXT NOT NULL,
    moves             TEXT NOT NULL,
    rating            INTEGER NOT NULL,
    rating_deviation  INTEGER NOT NULL,
    popularity        INTEGER NOT NULL,
    nb_plays          INTEGER NOT NULL,
    themes            TEXT,
    game_url          TEXT,
    opening_tags      TEXT
);

CREATE TABLE IF NOT EXISTS collections (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL UNIQUE,
    description TEXT
);
CREATE TABLE IF NOT EXISTS collection_games (
    collection_id INTEGER NOT NULL,
    game_id       INTEGER NOT NULL,
    PRIMARY KEY (collection_id, game_id)
);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS annotations (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id INTEGER NOT NULL,
    ply     INTEGER NOT NULL,
    path    TEXT,
    comment TEXT,
    nag     INTEGER,
    shapes  TEXT
);
CREATE INDEX IF NOT EXISTS idx_annotations_game ON annotations(game_id, ply);

CREATE TABLE IF NOT EXISTS evals (
    game_id INTEGER NOT NULL,
    ply     INTEGER NOT NULL,
    depth   INTEGER NOT NULL,
    cp      INTEGER,
    mate    INTEGER,
    PRIMARY KEY (game_id, ply, depth)
);

CREATE TABLE IF NOT EXISTS puzzle_attempts (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    puzzle_id     TEXT NOT NULL,
    solved        INTEGER NOT NULL,
    rating_before INTEGER NOT NULL,
    rating_after  INTEGER NOT NULL,
    at            TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS repertoires (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT NOT NULL,
    color      TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS repertoire_cards (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    repertoire_id INTEGER NOT NULL,
    fen           TEXT NOT NULL,
    uci           TEXT NOT NULL,
    san           TEXT NOT NULL,
    due_at        TEXT NOT NULL DEFAULT (datetime('now')),
    interval_days REAL NOT NULL DEFAULT 0,
    ease          REAL NOT NULL DEFAULT 2.5,
    reviews       INTEGER NOT NULL DEFAULT 0,
    lapses        INTEGER NOT NULL DEFAULT 0,
    UNIQUE(repertoire_id, fen, uci)
);

CREATE TABLE IF NOT EXISTS explorer_cache (
    source     TEXT NOT NULL,
    fen        TEXT NOT NULL,
    body       TEXT NOT NULL,
    fetched_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (source, fen)
);
"#;

const STANDARD_TAGS: &[&str] = &[
    "Event", "Site", "Date", "Round", "White", "Black", "Result", "WhiteElo", "BlackElo",
    "TimeControl", "FEN", "SetUp",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    pub id: i64,
    pub event: Option<String>,
    pub site: Option<String>,
    pub date: Option<String>,
    pub round: Option<String>,
    pub white: String,
    pub white_elo: Option<i32>,
    pub black: String,
    pub black_elo: Option<i32>,
    pub result: String,
    pub fen: String,
    /// Main-line SAN, space separated.
    pub moves: String,
    /// Full PGN movetext tree (mainline + variations + comments + NAGs) as JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub movetext: Option<String>,
    pub ply_count: i64,
    pub time_control: Option<String>,
    pub other: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    pub added_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GameQuery {
    pub white: Option<String>,
    pub black: Option<String>,
    pub result: Option<String>,
    pub event: Option<String>,
    pub player: Option<String>,
    pub collection_id: Option<i64>,
    /// Inclusive lower bound on the PGN date (`YYYY.MM.DD`).
    pub date_from: Option<String>,
    /// Inclusive upper bound on the PGN date (`YYYY.MM.DD`).
    pub date_to: Option<String>,
    /// Keep games where either side's Elo falls inside this range.
    pub elo_min: Option<i32>,
    pub elo_max: Option<i32>,
    /// Variant name; `standard` also matches games with no variant tag.
    pub variant: Option<String>,
    /// `newest` (default), `oldest`, `date`, `white`, `black`.
    pub sort: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResponse {
    pub games: Vec<Game>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub games: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerRow {
    pub name: String,
    pub elo: Option<i32>,
    pub games: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRow {
    pub name: String,
    pub games: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub id: i64,
    pub game_id: i64,
    pub ply: i64,
    #[serde(default)]
    pub path: Option<String>,
    pub comment: Option<String>,
    pub nag: Option<i64>,
    pub shapes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotationInput {
    pub game_id: i64,
    pub ply: i64,
    /// Tree path ("0.1.2"); mainline only games may omit it.
    #[serde(default)]
    pub path: Option<String>,
    pub comment: Option<String>,
    pub nag: Option<i64>,
    pub shapes: Option<String>,
}

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
    path: Arc<Mutex<std::path::PathBuf>>,
}

impl Db {
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path).with_context(|| format!("open {path}"))?;
        conn.execute_batch(SCHEMA).context("create schema")?;
        migrate(&conn)?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
            path: Arc::new(Mutex::new(std::path::PathBuf::from(path))),
        };
        db.seed_puzzles()?;
        Ok(db)
    }

    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
            path: Arc::new(Mutex::new(std::path::PathBuf::from(":memory:"))),
        };
        db.seed_puzzles()?;
        Ok(db)
    }

    /// The database file this handle currently points at.
    pub fn path(&self) -> String {
        self.path
            .lock()
            .map(|p| p.display().to_string())
            .unwrap_or_default()
    }

    /// Switch this handle (and every clone of it) to another SQLite file,
    /// creating and migrating it when needed.
    pub fn switch_to(&self, path: &std::path::Path) -> Result<()> {
        let conn = Connection::open(path).with_context(|| format!("open {}", path.display()))?;
        conn.execute_batch(SCHEMA).context("create schema")?;
        migrate(&conn)?;
        {
            let mut guard = self.conn.lock().expect("db lock");
            *guard = conn;
        }
        if let Ok(mut guard) = self.path.lock() {
            *guard = path.to_path_buf();
        }
        self.seed_puzzles()?;
        Ok(())
    }

    /// Import one or more PGN games. Every game is validated by the canonical
    /// reader (illegal moves are refused) before it is stored.
    pub fn import_pgn(&self, pgn: &str, collection: Option<i64>) -> Result<Vec<i64>> {
        let games = knightlm_notation::pgn::parse_all(pgn.as_bytes())?;
        if games.is_empty() {
            anyhow::bail!("no games in PGN");
        }
        let mut ids = Vec::new();
        for game in games {
            ids.push(self.import_one(game, collection, true)?);
        }
        Ok(ids)
    }

    /// Import as many games as possible, skipping unsupported ones (a mixed
    /// Chess.com/Lichess export may contain variants such as Chess960).
    pub fn import_pgn_lenient(
        &self,
        pgn: &str,
        collection: Option<i64>,
    ) -> Result<(Vec<i64>, usize)> {
        self.import_pgn_lenient_opts(pgn, collection, true)
    }

    /// Like [`Self::import_pgn_lenient`], optionally skipping the per-ply
    /// position index (bulk imports of large archives can turn it off).
    pub fn import_pgn_lenient_opts(
        &self,
        pgn: &str,
        collection: Option<i64>,
        index_positions: bool,
    ) -> Result<(Vec<i64>, usize)> {
        let games = knightlm_notation::pgn::parse_all(pgn.as_bytes())?;
        let mut ids = Vec::new();
        let mut skipped = 0usize;
        for game in games {
            match self.import_one(game, collection, index_positions) {
                Ok(id) => ids.push(id),
                Err(_) => skipped += 1,
            }
        }
        Ok((ids, skipped))
    }

    /// Open a transaction around many imports (bulk loading).
    pub fn begin_bulk(&self) -> Result<()> {
        self.conn.lock().expect("db lock").execute_batch("BEGIN")?;
        Ok(())
    }

    pub fn commit_bulk(&self) -> Result<()> {
        self.conn.lock().expect("db lock").execute_batch("COMMIT")?;
        Ok(())
    }

    pub fn rollback_bulk(&self) -> Result<()> {
        self.conn.lock().expect("db lock").execute_batch("ROLLBACK")?;
        Ok(())
    }

    fn import_one(
        &self,
        game: knightlm_notation::pgn::PgnGame,
        collection: Option<i64>,
        index_positions_wanted: bool,
    ) -> Result<i64> {
        let tags = &game.tags;
        let variant = normalize_variant(tags.get("Variant").map(String::as_str));
        let replayable = matches!(
            variant.as_str(),
            "standard"
                | "chess960"
                | "atomic"
                | "antichess"
                | "kingofthehill"
                | "3check"
                | "crazyhouse"
                | "racingkings"
                | "horde"
        );
        let record = if replayable {
            Some(
                knightlm_notation::reader::to_game_record(&game, "import", "pending")
                    .context("invalid game")?,
            )
        } else {
            None
        };
        let tag = |k: &str| tags.get(k).cloned();
        let elo = |k: &str| tags.get(k).and_then(|v| v.trim().parse::<i32>().ok());
        let san: Vec<String> = game.movetext.iter().map(|n| n.san.clone()).collect();
        let movetext_json = serde_json::to_string(&game.movetext)?;
        let initial_fen = record
            .as_ref()
            .map(|r| r.initial_fen.clone())
            .unwrap_or_else(|| {
                tags.get("FEN")
                    .cloned()
                    .map(|fen| {
                        if fen.split_whitespace().count() == 4 {
                            format!("{fen} 0 1")
                        } else {
                            fen
                        }
                    })
                    .unwrap_or_else(|| START_FEN.to_owned())
            });
        let variant_value = (variant != "standard").then(|| variant.clone());
        let result = game.result.clone().unwrap_or_else(|| "*".into());
        let other: BTreeMap<String, String> = tags
            .iter()
            .filter(|(k, _)| !STANDARD_TAGS.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let other_json = if other.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&other)?)
        };
        let conn = self.conn.lock().expect("db lock");
        conn.execute(
            "INSERT INTO games
             (event, site, date, round, white, white_elo, black, black_elo,
              result, fen, moves, movetext, ply_count, time_control, other, variant)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            params![
                tag("Event"),
                tag("Site"),
                tag("Date"),
                tag("Round"),
                tag("White").unwrap_or_else(|| "?".into()),
                elo("WhiteElo"),
                tag("Black").unwrap_or_else(|| "?".into()),
                elo("BlackElo"),
                result,
                initial_fen,
                san.join(" "),
                movetext_json,
                san.len() as i64,
                tag("TimeControl"),
                other_json,
                variant_value,
            ],
        )
        .context("insert game")?;
        let id = conn.last_insert_rowid();
        if index_positions_wanted {
            if let Some(record) = &record {
                index_positions(&conn, id, &record.initial_fen, &record.moves, &variant)?;
            }
        }
        if let Some(cid) = collection {
            conn.execute(
                "INSERT OR IGNORE INTO collection_games (collection_id, game_id) VALUES (?1, ?2)",
                params![cid, id],
            )?;
        }
        Ok(id)
    }

    pub fn query(&self, q: &GameQuery) -> Result<QueryResponse> {
        let mut filters: Vec<String> = Vec::new();
        let mut args: Vec<Value> = Vec::new();
        if let Some(v) = &q.white {
            filters.push("white = ?".into());
            args.push(Value::Text(v.clone()));
        }
        if let Some(v) = &q.black {
            filters.push("black = ?".into());
            args.push(Value::Text(v.clone()));
        }
        if let Some(v) = &q.result {
            filters.push("result = ?".into());
            args.push(Value::Text(v.clone()));
        }
        if let Some(v) = &q.event {
            filters.push("event = ?".into());
            args.push(Value::Text(v.clone()));
        }
        if let Some(v) = &q.player {
            filters.push("(white = ? OR black = ?)".into());
            args.push(Value::Text(v.clone()));
            args.push(Value::Text(v.clone()));
        }
        if let Some(v) = q.collection_id {
            filters.push(
                "id IN (SELECT game_id FROM collection_games WHERE collection_id = ?)".into(),
            );
            args.push(Value::Integer(v));
        }
        if let Some(from) = q.date_from.as_ref().filter(|d| !d.trim().is_empty()) {
            filters.push("date >= ?".into());
            args.push(Value::Text(from.trim().replace('-', ".")));
        }
        if let Some(to) = q.date_to.as_ref().filter(|d| !d.trim().is_empty()) {
            filters.push("date <= ?".into());
            args.push(Value::Text(to.trim().replace('-', ".")));
        }
        if let Some(min) = q.elo_min {
            filters.push(
                "((white_elo IS NOT NULL AND white_elo >= ?) OR (black_elo IS NOT NULL AND black_elo >= ?))"
                    .into(),
            );
            args.push(Value::Integer(min as i64));
            args.push(Value::Integer(min as i64));
        }
        if let Some(max) = q.elo_max {
            filters.push(
                "((white_elo IS NOT NULL AND white_elo <= ?) OR (black_elo IS NOT NULL AND black_elo <= ?))"
                    .into(),
            );
            args.push(Value::Integer(max as i64));
            args.push(Value::Integer(max as i64));
        }
        if let Some(variant) = q.variant.as_deref().filter(|v| !v.trim().is_empty()) {
            if variant == "standard" {
                filters.push(
                    "(variant IS NULL OR variant = '' OR variant = 'standard')".into(),
                );
            } else {
                filters.push("variant = ?".into());
                args.push(Value::Text(variant.to_owned()));
            }
        }
        let where_sql = if filters.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", filters.join(" AND "))
        };
        let conn = self.conn.lock().expect("db lock");
        let total: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM games {where_sql}"),
            params_from_iter(args.clone()),
            |row| row.get(0),
        )?;
        let order = match q.sort.as_deref() {
            Some("oldest") => "id ASC",
            Some("date") => "date DESC, id DESC",
            Some("white") => "white ASC, id DESC",
            Some("black") => "black ASC, id DESC",
            Some("length") => "ply_count DESC, id DESC",
            _ => "id DESC",
        };
        let limit = q.limit.unwrap_or(50).min(500) as i64;
        let offset = q.offset.unwrap_or(0) as i64;
        let sql = format!(
            "SELECT id, event, site, date, round, white, white_elo, black, black_elo,
                    result, fen, moves, ply_count, time_control, other, variant, added_at,
                    NULL AS movetext
             FROM games {where_sql} ORDER BY {order} LIMIT ? OFFSET ?"
        );
        let mut stmt = conn.prepare(&sql)?;
        let mut bound = args;
        bound.push(Value::Integer(limit));
        bound.push(Value::Integer(offset));
        let games = stmt
            .query_map(params_from_iter(bound.clone()), row_to_game)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(QueryResponse { games, total })
    }

    pub fn get(&self, id: i64) -> Result<Option<Game>> {
        let conn = self.conn.lock().expect("db lock");
        get_game(&conn, id)
    }

    pub fn count(&self) -> Result<i64> {
        let conn = self.conn.lock().expect("db lock");
        Ok(conn.query_row("SELECT COUNT(*) FROM games", [], |row| row.get(0))?)
    }

    pub fn collections(&self) -> Result<Vec<Collection>> {
        let conn = self.conn.lock().expect("db lock");
        let mut stmt = conn.prepare(
            "SELECT c.id, c.name, c.description, COUNT(cg.game_id)
             FROM collections c
             LEFT JOIN collection_games cg ON cg.collection_id = c.id
             GROUP BY c.id, c.name, c.description
             ORDER BY c.name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Collection {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                games: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn rename_collection(&self, id: i64, name: &str) -> Result<bool> {
        let conn = self.conn.lock().expect("db lock");
        Ok(conn.execute(
            "UPDATE collections SET name = ?2 WHERE id = ?1",
            params![id, name.trim()],
        )? > 0)
    }

    pub fn delete_collection(&self, id: i64) -> Result<bool> {
        let mut conn = self.conn.lock().expect("db lock");
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM collection_games WHERE collection_id = ?1",
            params![id],
        )?;
        let removed = tx.execute("DELETE FROM collections WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(removed > 0)
    }

    pub fn create_collection(&self, name: &str, description: Option<&str>) -> Result<Collection> {
        let conn = self.conn.lock().expect("db lock");
        conn.execute(
            "INSERT INTO collections (name, description) VALUES (?1, ?2)",
            params![name, description],
        )?;
        let id = conn.last_insert_rowid();
        Ok(Collection {
            id,
            name: name.to_owned(),
            description: description.map(str::to_owned),
            games: 0,
        })
    }

    pub fn add_to_collection(&self, collection_id: i64, game_id: i64) -> Result<()> {
        let conn = self.conn.lock().expect("db lock");
        conn.execute(
            "INSERT OR IGNORE INTO collection_games (collection_id, game_id) VALUES (?1, ?2)",
            params![collection_id, game_id],
        )?;
        Ok(())
    }

    pub fn players(&self) -> Result<Vec<PlayerRow>> {
        let conn = self.conn.lock().expect("db lock");
        let mut stmt = conn.prepare(
            "SELECT name, MAX(elo), COUNT(*) FROM (
                 SELECT white AS name, white_elo AS elo FROM games
                 UNION ALL
                 SELECT black AS name, black_elo AS elo FROM games
             ) WHERE name IS NOT NULL
             GROUP BY name ORDER BY COUNT(*) DESC LIMIT 500",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(PlayerRow {
                name: row.get(0)?,
                elo: row.get(1)?,
                games: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn events(&self) -> Result<Vec<EventRow>> {
        let conn = self.conn.lock().expect("db lock");
        let mut stmt = conn.prepare(
            "SELECT event, COUNT(*) FROM games WHERE event IS NOT NULL
             GROUP BY event ORDER BY COUNT(*) DESC LIMIT 300",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(EventRow {
                name: row.get(0)?,
                games: row.get(1)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Games whose move sequence reaches the given position (4- or 6-field FEN).
    /// Exact position search within a variant.
    pub fn search_position_variant(&self, fen: &str, variant: &str) -> Result<Vec<Game>> {
        let (pos, _) = variant_start(fen, Some(variant))?;
        let key = position_key_variant(&pos, variant);
        let conn = self.conn.lock().expect("db lock");
        let mut stmt =
            conn.prepare("SELECT DISTINCT game_id FROM positions WHERE key = ?1 ORDER BY game_id DESC")?;
        let ids = stmt
            .query_map(params![key], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut games = Vec::new();
        for id in ids {
            if let Some(g) = get_game(&conn, id)? {
                games.push(g);
            }
        }
        Ok(games)
    }

    /// Partial position search: every position that contains all the given
    /// pieces. The query's side to move, castling and en-passant fields are
    /// ignored; empty squares are wildcards.
    pub fn search_position_partial(&self, fen: &str) -> Result<Vec<Game>> {
        let board = fen.split_whitespace().next().unwrap_or_default();
        let mut required: Vec<(usize, char)> = Vec::new();
        let mut rank: i32 = 7;
        let mut file = 0usize;
        for ch in board.chars() {
            match ch {
                '/' => {
                    rank -= 1;
                    file = 0;
                }
                '1'..='8' => file += ch.to_digit(10).unwrap_or(0) as usize,
                _ => {
                    if (0..8).contains(&rank) && file < 8 {
                        required.push((rank as usize * 8 + file, ch));
                        file += 1;
                    }
                }
            }
        }
        if required.is_empty() {
            anyhow::bail!("partial search needs at least one piece");
        }
        let conn = self.conn.lock().expect("db lock");
        let mut stmt = conn.prepare("SELECT DISTINCT game_id, key FROM positions")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut ids = std::collections::BTreeSet::new();
        for row in rows {
            let (id, key) = row?;
            if board_contains(&key, &required) {
                ids.insert(id);
            }
        }
        let mut games = Vec::new();
        for id in ids.into_iter().rev() {
            if let Some(g) = get_game(&conn, id)? {
                games.push(g);
            }
        }
        Ok(games)
    }

    pub fn search_position(&self, fen: &str) -> Result<Vec<Game>> {
        let key = normalize_key(fen)?;
        let conn = self.conn.lock().expect("db lock");
        let mut stmt =
            conn.prepare("SELECT DISTINCT game_id FROM positions WHERE key = ?1 ORDER BY game_id DESC")?;
        let ids = stmt
            .query_map(params![key], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut games = Vec::new();
        for id in ids {
            if let Some(g) = get_game(&conn, id)? {
                games.push(g);
            }
        }
        Ok(games)
    }

    pub fn settings(&self) -> Result<BTreeMap<String, String>> {
        let conn = self.conn.lock().expect("db lock");
        let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<BTreeMap<_, _>>>()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().expect("db lock");
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    /// One annotation per node: saving again replaces the previous one.
    /// A node is identified by its tree path when given, otherwise by ply.
    pub fn add_annotation(&self, input: &AnnotationInput) -> Result<i64> {
        let conn = self.conn.lock().expect("db lock");
        match input.path.as_deref().filter(|p| !p.is_empty()) {
            Some(path) => conn.execute(
                "DELETE FROM annotations WHERE game_id = ?1 AND path = ?2",
                params![input.game_id, path],
            )?,
            None => conn.execute(
                "DELETE FROM annotations WHERE game_id = ?1 AND ply = ?2
                 AND (path IS NULL OR path = '')",
                params![input.game_id, input.ply],
            )?,
        };
        conn.execute(
            "INSERT INTO annotations (game_id, ply, path, comment, nag, shapes)
             VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                input.game_id,
                input.ply,
                input.path,
                input.comment,
                input.nag,
                input.shapes
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn annotations(&self, game_id: i64) -> Result<Vec<Annotation>> {
        let conn = self.conn.lock().expect("db lock");
        let mut stmt = conn.prepare(
            "SELECT id, game_id, ply, path, comment, nag, shapes FROM annotations
             WHERE game_id = ?1 ORDER BY ply, path, id",
        )?;
        let rows = stmt.query_map(params![game_id], |row| {
            Ok(Annotation {
                id: row.get(0)?,
                game_id: row.get(1)?,
                ply: row.get(2)?,
                path: row.get(3)?,
                comment: row.get(4)?,
                nag: row.get(5)?,
                shapes: row.get(6)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}


// ---------------------------------------------------------------------------
// Openings (CC0 Lichess chess-openings)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Opening {
    pub eco: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Continuation {
    pub uci: String,
    pub san: String,
    pub games: i64,
    pub white: i64,
    pub draw: i64,
    pub black: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Explorer {
    pub eco: Option<String>,
    pub name: Option<String>,
    pub continuations: Vec<Continuation>,
}

static OPENINGS: OnceLock<BTreeMap<String, Opening>> = OnceLock::new();

fn opening_book() -> &'static BTreeMap<String, Opening> {
    OPENINGS.get_or_init(|| {
        let mut map = BTreeMap::new();
        let sources = [
            include_str!("../assets/openings/a.tsv"),
            include_str!("../assets/openings/b.tsv"),
            include_str!("../assets/openings/c.tsv"),
            include_str!("../assets/openings/d.tsv"),
            include_str!("../assets/openings/e.tsv"),
        ];
        for src in sources {
            for line in src.lines().skip(1) {
                let mut parts = line.splitn(3, '\t');
                let (Some(eco), Some(name), Some(pgn)) =
                    (parts.next(), parts.next(), parts.next())
                else {
                    continue;
                };
                if let Ok(key) = key_for_pgn(pgn) {
                    map.entry(key).or_insert_with(|| Opening {
                        eco: eco.to_owned(),
                        name: name.to_owned(),
                    });
                }
            }
        }
        map
    })
}

fn key_for_pgn(pgn: &str) -> Result<String> {
    let game = knightlm_notation::pgn::parse(pgn)?;
    let (mut pos, _) = parse_chess("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")?;
    for node in &game.movetext {
        let mv = SanPlus::from_ascii(node.san.as_bytes())?
            .san
            .to_move(&pos)?;
        pos = pos.play(mv)?;
    }
    Ok(position_key(&pos))
}

/// The named opening for a position, if the CC0 table knows it.
pub fn classify_opening(fen: &str) -> Option<Opening> {
    let key = normalize_key(fen).ok()?;
    opening_book().get(&key).cloned()
}

impl Db {
    /// Opening name plus the most common continuations in this database.
    pub fn explore(&self, fen: &str) -> Result<Explorer> {
        let key = normalize_key(fen)?;
        let opening = opening_book().get(&key).cloned();
        let pos: Chess = {
            let fields: Vec<&str> = fen.split_whitespace().collect();
            let full = if fields.len() == 4 {
                format!("{} 0 1", fen.trim())
            } else {
                fen.trim().to_owned()
            };
            parse_chess(&full)?.0
        };
        let conn = self.conn.lock().expect("db lock");
        let mut stmt = conn.prepare(
            "SELECT p2.uci,
                    COUNT(*),
                    SUM(CASE WHEN g.result = '1-0' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN g.result = '1/2-1/2' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN g.result = '0-1' THEN 1 ELSE 0 END)
             FROM positions p1
             JOIN positions p2 ON p2.game_id = p1.game_id AND p2.ply = p1.ply + 1
             JOIN games g ON g.id = p1.game_id
             WHERE p1.key = ?1 AND p2.uci IS NOT NULL
             GROUP BY p2.uci
             ORDER BY COUNT(*) DESC
             LIMIT 12",
        )?;
        let rows = stmt.query_map(params![key], |row| {
            let uci: String = row.get(0)?;
            let san = UciMove::from_ascii(uci.as_bytes())
                .ok()
                .and_then(|u| u.to_move(&pos).ok())
                .map(|m| SanPlus::from_move(pos.clone(), m).to_string())
                .unwrap_or_else(|| uci.clone());
            Ok(Continuation {
                uci,
                san,
                games: row.get(1)?,
                white: row.get(2)?,
                draw: row.get(3)?,
                black: row.get(4)?,
            })
        })?;
        Ok(Explorer {
            eco: opening.as_ref().map(|o| o.eco.clone()),
            name: opening.map(|o| o.name),
            continuations: rows.collect::<rusqlite::Result<Vec<_>>>()?,
        })
    }
}

// ---------------------------------------------------------------------------
// Puzzles (Lichess puzzle CSV, CC0)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Puzzle {
    pub id: String,
    pub fen: String,
    pub moves: String,
    pub moves_san: String,
    pub rating: i32,
    pub rating_deviation: i32,
    pub popularity: i32,
    pub nb_plays: i32,
    pub themes: Option<String>,
    pub game_url: Option<String>,
    pub opening_tags: Option<String>,
}

impl Db {
    fn seed_puzzles(&self) -> Result<()> {
        let count: i64 = {
            let conn = self.conn.lock().expect("db lock");
            conn.query_row("SELECT COUNT(*) FROM puzzles", [], |row| row.get(0))?
        };
        if count == 0 {
            self.import_puzzles_csv(include_str!("../assets/puzzles_sample.csv"))?;
        }
        Ok(())
    }

    /// Import a Lichess puzzle CSV (`PuzzleId,FEN,Moves,Rating,...`).
    pub fn import_puzzles_csv(&self, csv: &str) -> Result<usize> {
        let conn = self.conn.lock().expect("db lock");
        let mut imported = 0usize;
        // Accept both the Lichess CSV with its header and a headerless paste.
        let mut lines = csv.lines().filter(|l| !l.trim().is_empty());
        let first = lines.next().unwrap_or_default();
        let body: Vec<&str> = if first.starts_with("PuzzleId") {
            lines.collect()
        } else {
            std::iter::once(first).chain(lines).collect()
        };
        for line in body {
            let cols: Vec<&str> = line.split(',').collect();
            if cols.len() < 7 {
                continue;
            }
            conn.execute(
                "INSERT OR IGNORE INTO puzzles
                 (id, fen, moves, rating, rating_deviation, popularity, nb_plays,
                  themes, game_url, opening_tags)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    cols[0],
                    cols[1],
                    cols[2],
                    cols[3].parse::<i32>().unwrap_or(0),
                    cols[4].parse::<i32>().unwrap_or(0),
                    cols[5].parse::<i32>().unwrap_or(0),
                    cols[6].parse::<i32>().unwrap_or(0),
                    cols.get(7).map(|s| s.to_string()),
                    cols.get(8).map(|s| s.to_string()),
                    cols.get(9).map(|s| s.to_string()),
                ],
            )?;
            imported += conn.changes() as usize;
        }
        Ok(imported)
    }

    /// A puzzle, nearest to `rating` when given, filtered by theme when given.
    pub fn next_puzzle(&self, rating: Option<i32>, theme: Option<&str>) -> Result<Option<Puzzle>> {
        let conn = self.conn.lock().expect("db lock");
        let mut sql = String::from(
            "SELECT id, fen, moves, rating, rating_deviation, popularity, nb_plays,
                    themes, game_url, opening_tags
             FROM puzzles WHERE 1=1",
        );
        let mut args: Vec<Value> = Vec::new();
        if let Some(theme) = theme.filter(|t| !t.trim().is_empty()) {
            sql.push_str(" AND (' ' || COALESCE(themes, '') || ' ') LIKE ?");
            args.push(Value::Text(format!("% {} %", theme.trim())));
        }
        if let Some(r) = rating {
            sql.push_str(" ORDER BY ABS(rating - ?) LIMIT 1");
            args.push(Value::Integer(r as i64));
        } else {
            sql.push_str(" ORDER BY RANDOM() LIMIT 1");
        }
        let mut stmt = conn.prepare(&sql)?;
        let row = stmt
            .query_row(params_from_iter(args), row_to_puzzle)
            .ok();
        Ok(row)
    }

    pub fn get_puzzle(&self, id: &str) -> Result<Option<Puzzle>> {
        let conn = self.conn.lock().expect("db lock");
        let row = conn
            .query_row(
                "SELECT id, fen, moves, rating, rating_deviation, popularity, nb_plays,
                        themes, game_url, opening_tags
                 FROM puzzles WHERE id = ?1",
                params![id],
                row_to_puzzle,
            )
            .ok();
        Ok(row)
    }

    /// Every theme in the puzzle set, most common first.
    pub fn puzzle_themes(&self) -> Result<Vec<(String, i64)>> {
        let conn = self.conn.lock().expect("db lock");
        let mut stmt = conn.prepare("SELECT themes FROM puzzles WHERE themes IS NOT NULL")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut counts: BTreeMap<String, i64> = BTreeMap::new();
        for row in rows {
            for theme in row?.split_whitespace() {
                *counts.entry(theme.to_owned()).or_default() += 1;
            }
        }
        let mut out: Vec<(String, i64)> = counts.into_iter().collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        Ok(out)
    }

    /// The user's local puzzle rating (default 1500).
    pub fn puzzle_rating(&self) -> Result<i32> {
        let conn = self.conn.lock().expect("db lock");
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'puzzle_rating'",
                [],
                |row| row.get(0),
            )
            .ok();
        Ok(value.and_then(|v| v.parse().ok()).unwrap_or(1500))
    }

    /// Record a puzzle result and update the local rating with a standard Elo
    /// step (K = 32, clamped to 400..3000). Returns (before, after).
    pub fn record_puzzle_attempt(
        &self,
        puzzle_id: &str,
        puzzle_rating: i32,
        solved: bool,
    ) -> Result<(i32, i32)> {
        let before = self.puzzle_rating()?;
        let expected = 1.0 / (1.0 + 10f64.powf((puzzle_rating - before) as f64 / 400.0));
        let score = if solved { 1.0 } else { 0.0 };
        let after = (before as f64 + 32.0 * (score - expected)).round() as i32;
        let after = after.clamp(400, 3000);
        let conn = self.conn.lock().expect("db lock");
        conn.execute(
            "INSERT INTO puzzle_attempts (puzzle_id, solved, rating_before, rating_after)
             VALUES (?1, ?2, ?3, ?4)",
            params![puzzle_id, solved, before, after],
        )?;
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('puzzle_rating', ?1)",
            params![after.to_string()],
        )?;
        Ok((before, after))
    }

    /// Recent puzzle attempts, newest first.
    pub fn puzzle_attempts(&self, limit: u32) -> Result<Vec<PuzzleAttempt>> {
        let conn = self.conn.lock().expect("db lock");
        let mut stmt = conn.prepare(
            "SELECT id, puzzle_id, solved, rating_before, rating_after, at
             FROM puzzle_attempts ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(PuzzleAttempt {
                id: row.get(0)?,
                puzzle_id: row.get(1)?,
                solved: row.get::<_, i64>(2)? != 0,
                rating_before: row.get(3)?,
                rating_after: row.get(4)?,
                at: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn save_evals(&self, game_id: i64, depth: u32, evals: &[(i64, Option<i32>, Option<i32>)]) -> Result<()> {
        let mut conn = self.conn.lock().expect("db lock");
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM evals WHERE game_id = ?1 AND depth = ?2",
            params![game_id, depth],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO evals (game_id, ply, depth, cp, mate)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (ply, cp, mate) in evals {
                stmt.execute(params![game_id, ply, depth, cp, mate])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn evals(&self, game_id: i64, depth: u32) -> Result<Vec<EvalRow>> {
        let conn = self.conn.lock().expect("db lock");
        let mut stmt = conn.prepare(
            "SELECT ply, depth, cp, mate FROM evals
             WHERE game_id = ?1 AND depth = ?2 ORDER BY ply",
        )?;
        let rows = stmt.query_map(params![game_id, depth], |row| {
            Ok(EvalRow {
                ply: row.get(0)?,
                depth: row.get(1)?,
                cp: row.get(2)?,
                mate: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Evals from the most recently written depth for this game.
    pub fn evals_latest(&self, game_id: i64) -> Result<Vec<EvalRow>> {
        let conn = self.conn.lock().expect("db lock");
        let depth: Option<u32> = conn
            .query_row(
                "SELECT depth FROM evals WHERE game_id = ?1 ORDER BY rowid DESC LIMIT 1",
                params![game_id],
                |row| row.get(0),
            )
            .ok();
        let Some(depth) = depth else {
            return Ok(Vec::new());
        };
        let mut stmt = conn.prepare(
            "SELECT ply, depth, cp, mate FROM evals
             WHERE game_id = ?1 AND depth = ?2 ORDER BY ply",
        )?;
        let rows = stmt.query_map(params![game_id, depth], |row| {
            Ok(EvalRow {
                ply: row.get(0)?,
                depth: row.get(1)?,
                cp: row.get(2)?,
                mate: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn update_tags(&self, id: i64, tags: &GameTags) -> Result<bool> {
        let mut sets: Vec<&str> = Vec::new();
        let mut args: Vec<Value> = Vec::new();
        macro_rules! text {
            ($field:ident, $col:literal) => {
                if let Some(v) = &tags.$field {
                    sets.push(concat!($col, " = ?"));
                    args.push(Value::Text(v.clone()));
                }
            };
        }
        macro_rules! number {
            ($field:ident, $col:literal) => {
                if let Some(v) = tags.$field {
                    sets.push(concat!($col, " = ?"));
                    args.push(Value::Integer(v as i64));
                }
            };
        }
        text!(event, "event");
        text!(site, "site");
        text!(date, "date");
        text!(round, "round");
        text!(white, "white");
        text!(black, "black");
        text!(result, "result");
        number!(white_elo, "white_elo");
        number!(black_elo, "black_elo");
        text!(time_control, "time_control");
        if sets.is_empty() {
            return Ok(self.get(id)?.is_some());
        }
        args.push(Value::Integer(id));
        let sql = format!("UPDATE games SET {} WHERE id = ?", sets.join(", "));
        let conn = self.conn.lock().expect("db lock");
        Ok(conn.execute(&sql, params_from_iter(args))? > 0)
    }

    pub fn create_repertoire(&self, name: &str, color: &str) -> Result<Repertoire> {
        let color = if color.eq_ignore_ascii_case("black") { "black" } else { "white" };
        let id = {
            let conn = self.conn.lock().expect("db lock");
            conn.execute(
                "INSERT INTO repertoires (name, color) VALUES (?1, ?2)",
                params![name.trim(), color],
            )?;
            conn.last_insert_rowid()
        };
        self.repertoire(id)?.context("created repertoire")
    }

    pub fn repertoires(&self) -> Result<Vec<Repertoire>> {
        let conn = self.conn.lock().expect("db lock");
        let mut stmt = conn.prepare(
            "SELECT r.id, r.name, r.color, r.created_at,
                    COUNT(c.id),
                    SUM(CASE WHEN c.due_at <= datetime('now') THEN 1 ELSE 0 END)
             FROM repertoires r LEFT JOIN repertoire_cards c ON c.repertoire_id = r.id
             GROUP BY r.id ORDER BY r.name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Repertoire {
                id: row.get(0)?,
                name: row.get(1)?,
                color: row.get(2)?,
                created_at: row.get(3)?,
                cards: row.get(4)?,
                due: row.get::<_, Option<i64>>(5)?.unwrap_or(0),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn repertoire(&self, id: i64) -> Result<Option<Repertoire>> {
        Ok(self.repertoires()?.into_iter().find(|r| r.id == id))
    }

    pub fn delete_repertoire(&self, id: i64) -> Result<bool> {
        let mut conn = self.conn.lock().expect("db lock");
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM repertoire_cards WHERE repertoire_id = ?1", params![id])?;
        let removed = tx.execute("DELETE FROM repertoires WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(removed > 0)
    }

    /// Build cards from a PGN: one card per move played by the repertoire's
    /// colour, across the mainline and every variation.
    pub fn import_repertoire_pgn(&self, id: i64, pgn: &str) -> Result<usize> {
        let color = {
            let conn = self.conn.lock().expect("db lock");
            conn.query_row("SELECT color FROM repertoires WHERE id = ?1", params![id], |row| {
                row.get::<_, String>(0)
            })?
        };
        let want = if color == "black" {
            shakmaty::Color::Black
        } else {
            shakmaty::Color::White
        };
        let games = knightlm_notation::pgn::parse_all(pgn.as_bytes())?;
        let mut cards: Vec<(String, String, String)> = Vec::new();
        for game in games {
            let variant_name = normalize_variant(game.tags.get("Variant").map(String::as_str));
            let (start, mode) = match game.tags.get("FEN") {
                Some(fen) => variant_start(fen, Some(&variant_name))?,
                None => variant_start(START_FEN, Some(&variant_name))?,
            };
            collect_repertoire_cards(&game.movetext, &start, mode, want, &mut cards)?;
        }
        let mut conn = self.conn.lock().expect("db lock");
        let tx = conn.transaction()?;
        let mut added = 0usize;
        {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO repertoire_cards
                 (repertoire_id, fen, uci, san) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (fen, uci, san) in &cards {
                added += stmt.execute(params![id, fen, uci, san])?;
            }
        }
        tx.commit()?;
        Ok(added)
    }

    pub fn next_repertoire_card(&self, id: i64) -> Result<Option<RepertoireCard>> {
        let conn = self.conn.lock().expect("db lock");
        let card = conn
            .query_row(
                "SELECT id, repertoire_id, fen, uci, san, due_at, interval_days, ease, reviews, lapses
                 FROM repertoire_cards
                 WHERE repertoire_id = ?1 AND due_at <= datetime('now')
                 ORDER BY due_at, id LIMIT 1",
                params![id],
                row_to_repertoire_card,
            )
            .ok();
        Ok(card)
    }

    /// SM-2 style scheduling. A correct answer grows the interval, a wrong one
    /// resets it and slightly lowers the ease factor.
    pub fn review_repertoire_card(&self, card_id: i64, correct: bool) -> Result<RepertoireCard> {
        let conn = self.conn.lock().expect("db lock");
        let (interval, ease, reviews, lapses) = conn.query_row(
            "SELECT interval_days, ease, reviews, lapses FROM repertoire_cards WHERE id = ?1",
            params![card_id],
            |row| {
                Ok((
                    row.get::<_, f64>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )?;
        let (interval, ease, lapses) = if correct {
            let ease = (ease + 0.1).min(3.0);
            let interval = match reviews {
                0 => 1.0,
                1 => 3.0,
                _ => (interval * ease).max(1.0),
            };
            (interval, ease, lapses)
        } else {
            let ease = (ease - 0.2).max(1.3);
            (1.0, ease, lapses + 1)
        };
        let reviews = reviews + 1;
        conn.execute(
            "UPDATE repertoire_cards
             SET interval_days = ?2, ease = ?3, reviews = ?4, lapses = ?5,
                 due_at = datetime('now', ?6)
             WHERE id = ?1",
            params![
                card_id,
                interval,
                ease,
                reviews,
                lapses,
                format!("+{} hours", (interval * 24.0).round() as i64)
            ],
        )?;
        Ok(conn.query_row(
            "SELECT id, repertoire_id, fen, uci, san, due_at, interval_days, ease, reviews, lapses
             FROM repertoire_cards WHERE id = ?1",
            params![card_id],
            row_to_repertoire_card,
        )?)
    }

    pub fn stats(&self) -> Result<Stats> {
        let conn = self.conn.lock().expect("db lock");
        let count = |sql: &str| -> Result<i64> { Ok(conn.query_row(sql, [], |row| row.get(0))?) };
        let games = count("SELECT COUNT(*) FROM games")?;
        let white_wins = count("SELECT COUNT(*) FROM games WHERE result = '1-0'")?;
        let draws = count("SELECT COUNT(*) FROM games WHERE result = '1/2-1/2'")?;
        let black_wins = count("SELECT COUNT(*) FROM games WHERE result = '0-1'")?;
        let unfinished = count("SELECT COUNT(*) FROM games WHERE result NOT IN ('1-0','1/2-1/2','0-1')")?;
        let players = count(
            "SELECT COUNT(*) FROM (SELECT white AS n FROM games UNION SELECT black AS n FROM games)",
        )?;
        let events = count("SELECT COUNT(DISTINCT event) FROM games WHERE event IS NOT NULL")?;
        let collections = count("SELECT COUNT(*) FROM collections")?;
        let puzzles = count("SELECT COUNT(*) FROM puzzles")?;
        let mut stmt = conn.prepare(
            "SELECT name, MAX(elo), COUNT(*) FROM (
                 SELECT white AS name, white_elo AS elo FROM games
                 UNION ALL
                 SELECT black AS name, black_elo AS elo FROM games
             ) WHERE name IS NOT NULL
             GROUP BY name ORDER BY COUNT(*) DESC LIMIT 8",
        )?;
        let top_players = stmt
            .query_map([], |row| {
                Ok(PlayerRow {
                    name: row.get(0)?,
                    elo: row.get(1)?,
                    games: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut stmt = conn.prepare(
            "SELECT event, COUNT(*) FROM games WHERE event IS NOT NULL
             GROUP BY event ORDER BY COUNT(*) DESC LIMIT 8",
        )?;
        let top_events = stmt
            .query_map([], |row| {
                Ok(EventRow {
                    name: row.get(0)?,
                    games: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Stats {
            games,
            white_wins,
            draws,
            black_wins,
            unfinished,
            players,
            events,
            collections,
            puzzles,
            top_players,
            top_events,
        })
    }

    pub fn delete_game(&self, id: i64) -> Result<bool> {
        let mut conn = self.conn.lock().expect("db lock");
        let tx = conn.transaction()?;
        for table in ["positions", "annotations", "evals", "collection_games"] {
            tx.execute(&format!("DELETE FROM {table} WHERE game_id = ?1"), params![id])?;
        }
        let removed = tx.execute("DELETE FROM games WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(removed > 0)
    }

    /// Replace a game's mainline with validated SAN moves. Positions, cached
    /// evals and annotations are rebuilt/cleared because their plies changed.
    pub fn update_moves(&self, id: i64, san_moves: &str) -> Result<bool> {
        let Some(game) = self.get(id)? else {
            return Ok(false);
        };
        let variant_name = game.variant.clone().unwrap_or_else(|| "standard".into());
        let (mut pos, mode) = variant_start(&game.fen, Some(&variant_name))?;
        let mut nodes: Vec<knightlm_notation::pgn::PgnNode> = Vec::new();
        let mut ucis: Vec<String> = Vec::new();
        let mut sans: Vec<String> = Vec::new();
        for raw in san_moves.split_whitespace() {
            let token = raw.trim();
            if token.is_empty()
                || token.chars().all(|c| c.is_ascii_digit() || c == '.')
                || matches!(token, "1-0" | "0-1" | "1/2-1/2" | "*")
            {
                continue;
            }
            let parsed = SanPlus::from_ascii(token.as_bytes())
                .with_context(|| format!("invalid SAN: {token}"))?;
            let mv = parsed
                .san
                .to_move(&pos)
                .with_context(|| format!("illegal move: {token}"))?;
            ucis.push(UciMove::from_move(mv, mode).to_string());
            sans.push(SanPlus::from_move(pos.clone(), mv).to_string());
            nodes.push(knightlm_notation::pgn::PgnNode {
                san: SanPlus::from_move(pos.clone(), mv).to_string(),
                nags: Vec::new(),
                pre_comment: None,
                post_comment: None,
                variations: Vec::new(),
            });
            pos = pos.play(mv)?;
        }
        let movetext = serde_json::to_string(&nodes)?;
        let mut conn = self.conn.lock().expect("db lock");
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE games SET moves = ?2, movetext = ?3, ply_count = ?4 WHERE id = ?1",
            params![id, sans.join(" "), movetext, sans.len() as i64],
        )?;
        for table in ["positions", "evals", "annotations"] {
            tx.execute(&format!("DELETE FROM {table} WHERE game_id = ?1"), params![id])?;
        }
        index_positions(
            &tx,
            id,
            &game.fen,
            &ucis,
            &game.variant.clone().unwrap_or_else(|| "standard".into()),
        )?;
        tx.commit()?;
        Ok(true)
    }

    /// Replace the whole movetext tree from PGN text (variations allowed).
    pub fn update_pgn_tree(&self, id: i64, pgn: &str) -> Result<bool> {
        let Some(game) = self.get(id)? else {
            return Ok(false);
        };
        let games = knightlm_notation::pgn::parse_all(pgn.as_bytes())?;
        let Some(parsed) = games.into_iter().next() else {
            anyhow::bail!("no movetext");
        };
        let mut nodes = parsed.movetext;
        let (start, mode) = variant_start(&game.fen, game.variant.as_deref())?;
        let (sans, ucis) = validate_tree(&mut nodes, &start, mode)?;
        self.apply_tree(id, &nodes, &sans, &ucis)?;
        Ok(true)
    }

    /// Remove a node and everything after it in its own line.
    pub fn delete_tree_node(&self, id: i64, path: &str) -> Result<bool> {
        let Some(game) = self.get(id)? else {
            return Ok(false);
        };
        let mut nodes: Vec<knightlm_notation::pgn::PgnNode> = game
            .movetext
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?
            .unwrap_or_default();
        {
            let (line, idx) = resolve_tree_path(&mut nodes, path)?;
            let first_variation = line
                .get_mut(idx)
                .and_then(|node| {
                    (!node.variations.is_empty()).then(|| node.variations.remove(0))
                });
            line.truncate(idx);
            if let Some(variation) = first_variation {
                line.extend(variation);
            }
        }
        prune_tree(&mut nodes);
        let (start, mode) = variant_start(&game.fen, game.variant.as_deref())?;
        let (sans, ucis) = validate_tree(&mut nodes, &start, mode)?;
        self.apply_tree(id, &nodes, &sans, &ucis)?;
        Ok(true)
    }

    /// Promote a variation to the mainline; the old line becomes a variation.
    pub fn promote_variation(&self, id: i64, path: &str) -> Result<bool> {
        let Some(game) = self.get(id)? else {
            return Ok(false);
        };
        let Some((prefix, variation)) = path.rsplit_once('~') else {
            anyhow::bail!("not a variation");
        };
        let Some((j, _)) = variation.split_once('.') else {
            anyhow::bail!("bad variation path");
        };
        let j: usize = j.parse()?;
        let mut nodes: Vec<knightlm_notation::pgn::PgnNode> = game
            .movetext
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?
            .unwrap_or_default();
        {
            let (parent, at) = resolve_tree_path(&mut nodes, prefix)?;
            let old_line = parent.split_off(at);
            if old_line.is_empty() {
                anyhow::bail!("empty line");
            }
            let promoted = old_line[0]
                .variations
                .get(j)
                .cloned()
                .context("variation not found")?;
            let mut demoted = old_line;
            demoted[0].variations.remove(j);
            let mut promoted = promoted;
            if let Some(first) = promoted.first_mut() {
                first.variations.push(demoted);
            }
            parent.extend(promoted);
        }
        prune_tree(&mut nodes);
        let (start, mode) = variant_start(&game.fen, game.variant.as_deref())?;
        let (sans, ucis) = validate_tree(&mut nodes, &start, mode)?;
        self.apply_tree(id, &nodes, &sans, &ucis)?;
        Ok(true)
    }

    /// Play a move after `parent_path` (empty = start). Reuses an existing
    /// continuation when it matches, otherwise creates a variation. The clock
    /// is written into the node's PGN comment. Returns the node's tree path.
    pub fn add_tree_move(
        &self,
        id: i64,
        parent_path: &str,
        san: &str,
        clock: Option<&str>,
    ) -> Result<(Game, String)> {
        let Some(game) = self.get(id)? else {
            anyhow::bail!("game not found");
        };
        let mut nodes: Vec<knightlm_notation::pgn::PgnNode> = game
            .movetext
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?
            .unwrap_or_default();
        let (start, mode) = variant_start(&game.fen, game.variant.as_deref())?;
        let line_sans = sans_along_path(&nodes, parent_path, &start, mode)?;
        let position = play_sans(&start, &line_sans, mode)?;
        let parsed = SanPlus::from_ascii(san.as_bytes())?;
        let mv = parsed
            .san
            .to_move(&position)
            .with_context(|| format!("illegal move: {san}"))?;
        let canonical = SanPlus::from_move(position.clone(), mv).to_string();

        let (new_path, existing) = if parent_path.is_empty() {
            match nodes.first() {
                Some(next) if next.san == canonical => {
                    ("0".to_owned(), true)
                }
                Some(_) => {
                    let mut node = new_node(&canonical, clock);
                    // A new first move is an alternative to the existing one.
                    node.variations.clear();
                    let branch = vec![node];
                    let at = 0usize;
                    let existing_node = &mut nodes[at];
                    existing_node.variations.push(branch);
                    let j = existing_node.variations.len() - 1;
                    (format!("0~{j}.0"), false)
                }
                None => {
                    nodes.push(new_node(&canonical, clock));
                    ("0".to_owned(), false)
                }
            }
        } else {
            let (line, idx) = resolve_tree_path(&mut nodes, parent_path)?;
            match line.get(idx + 1) {
                Some(next) if next.san == canonical => {
                    if let Some(clock) = clock {
                        set_clock_comment(&mut line[idx + 1], clock);
                    }
                    (next_child_path(parent_path, idx + 1), true)
                }
                Some(_) => {
                    let next_path = next_child_path(parent_path, idx + 1);
                    let mut node = new_node(&canonical, clock);
                    node.variations.clear();
                    let branch = vec![node];
                    let next = &mut line[idx + 1];
                    next.variations.push(branch);
                    let j = next.variations.len() - 1;
                    (format!("{next_path}~{j}.0"), false)
                }
                None => {
                    line.push(new_node(&canonical, clock));
                    (next_child_path(parent_path, idx + 1), false)
                }
            }
        };
        if existing {
            if let Some(clock) = clock {
                let (line, idx) = resolve_tree_path(&mut nodes, &new_path)?;
                set_clock_comment(&mut line[idx], clock);
            }
        }
        prune_tree(&mut nodes);
        let (sans, ucis) = validate_tree(&mut nodes, &start, mode)?;
        self.apply_tree(id, &nodes, &sans, &ucis)?;
        let game = self.get(id)?.context("game")?;
        Ok((game, new_path))
    }

    fn apply_tree(
        &self,
        id: i64,
        nodes: &[knightlm_notation::pgn::PgnNode],
        sans: &[String],
        ucis: &[String],
    ) -> Result<()> {
        let movetext = serde_json::to_string(nodes)?;
        let Some(game) = self.get(id)? else {
            anyhow::bail!("game not found");
        };
        let mut conn = self.conn.lock().expect("db lock");
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE games SET moves = ?2, movetext = ?3, ply_count = ?4 WHERE id = ?1",
            params![id, sans.join(" "), movetext, sans.len() as i64],
        )?;
        for table in ["positions", "evals", "annotations"] {
            tx.execute(&format!("DELETE FROM {table} WHERE game_id = ?1"), params![id])?;
        }
        index_positions(
            &tx,
            id,
            &game.fen,
            ucis,
            &game.variant.clone().unwrap_or_else(|| "standard".into()),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Export one game as PGN, preserving variations, comments and NAGs.
    pub fn export_pgn(&self, id: i64) -> Result<Option<String>> {
        let Some(game) = self.get(id)? else {
            return Ok(None);
        };
        let mut tags: BTreeMap<String, String> = BTreeMap::new();
        let pairs = [
            ("Event", &game.event),
            ("Site", &game.site),
            ("Date", &game.date),
            ("Round", &game.round),
            ("TimeControl", &game.time_control),
        ];
        for (k, v) in pairs {
            if let Some(v) = v {
                tags.insert(k.to_owned(), v.clone());
            }
        }
        tags.insert("White".into(), game.white.clone());
        tags.insert("Black".into(), game.black.clone());
        tags.insert("Result".into(), game.result.clone());
        if let Some(elo) = game.white_elo {
            tags.insert("WhiteElo".into(), elo.to_string());
        }
        if let Some(elo) = game.black_elo {
            tags.insert("BlackElo".into(), elo.to_string());
        }
        if game.fen != "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" {
            tags.insert("FEN".into(), game.fen.clone());
            tags.insert("SetUp".into(), "1".into());
        }
        if let Some(other) = &game.other {
            for (k, v) in other {
                tags.insert(k.clone(), v.clone());
            }
        }
        let movetext = game
            .movetext
            .as_deref()
            .and_then(|json| serde_json::from_str::<Vec<knightlm_notation::pgn::PgnNode>>(json).ok())
            .unwrap_or_else(|| {
                game.moves
                    .split_whitespace()
                    .map(|san| knightlm_notation::pgn::PgnNode {
                        san: san.to_owned(),
                        nags: Vec::new(),
                        pre_comment: None,
                        post_comment: None,
                        variations: Vec::new(),
                    })
                    .collect()
            });
        let out = knightlm_notation::pgn::PgnGame {
            tags,
            movetext,
            result: Some(game.result.clone()),
        };
        Ok(Some(knightlm_notation::pgn::write(&out)))
    }

    pub fn cache_explorer(&self, source: &str, fen: &str, body: &str) -> Result<()> {
        let conn = self.conn.lock().expect("db lock");
        conn.execute(
            "INSERT OR REPLACE INTO explorer_cache (source, fen, body, fetched_at)
             VALUES (?1, ?2, ?3, datetime('now'))",
            params![source, fen, body],
        )?;
        Ok(())
    }

    pub fn cached_explorer(&self, source: &str, fen: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().expect("db lock");
        Ok(conn
            .query_row(
                "SELECT body FROM explorer_cache WHERE source = ?1 AND fen = ?2",
                params![source, fen],
                |row| row.get(0),
            )
            .ok())
    }
}

/// Editable PGN tags. Only the fields present are written.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GameTags {
    #[serde(default)]
    pub event: Option<String>,
    #[serde(default)]
    pub site: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub round: Option<String>,
    #[serde(default)]
    pub white: Option<String>,
    #[serde(default)]
    pub black: Option<String>,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub white_elo: Option<i32>,
    #[serde(default)]
    pub black_elo: Option<i32>,
    #[serde(default)]
    pub time_control: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalRow {
    pub ply: i64,
    pub depth: u32,
    pub cp: Option<i32>,
    pub mate: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repertoire {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub cards: i64,
    pub due: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepertoireCard {
    pub id: i64,
    pub repertoire_id: i64,
    pub fen: String,
    pub uci: String,
    pub san: String,
    pub due_at: String,
    pub interval_days: f64,
    pub ease: f64,
    pub reviews: i64,
    pub lapses: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub games: i64,
    pub white_wins: i64,
    pub draws: i64,
    pub black_wins: i64,
    pub unfinished: i64,
    pub players: i64,
    pub events: i64,
    pub collections: i64,
    pub puzzles: i64,
    pub top_players: Vec<PlayerRow>,
    pub top_events: Vec<EventRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PuzzleAttempt {
    pub id: i64,
    pub puzzle_id: String,
    pub solved: bool,
    pub rating_before: i32,
    pub rating_after: i32,
    pub at: String,
}

fn row_to_puzzle(row: &rusqlite::Row<'_>) -> rusqlite::Result<Puzzle> {
    let fen: String = row.get(1)?;
    let moves: String = row.get(2)?;
    let moves_san = moves_to_san(&fen, &moves);
    Ok(Puzzle {
        id: row.get(0)?,
        fen,
        moves,
        moves_san,
        rating: row.get(3)?,
        rating_deviation: row.get(4)?,
        popularity: row.get(5)?,
        nb_plays: row.get(6)?,
        themes: row.get(7)?,
        game_url: row.get(8)?,
        opening_tags: row.get(9)?,
    })
}

fn moves_to_san(fen: &str, moves: &str) -> String {
    let Ok(mut pos) = Fen::from_ascii(fen.as_bytes())
        .and_then(|f| f.into_position::<Chess>(CastlingMode::Standard).map_err(|_| shakmaty::fen::ParseFenError::InvalidBoard))
    else {
        return moves.to_owned();
    };
    let mut out = Vec::new();
    for token in moves.split_whitespace() {
        let Ok(uci) = UciMove::from_ascii(token.as_bytes()) else {
            continue;
        };
        let Ok(mv) = uci.to_move(&pos) else { continue };
        out.push(SanPlus::from_move(pos.clone(), mv).to_string());
        if let Ok(next) = pos.clone().play(mv) {
            pos = next;
        }
    }
    out.join(" ")
}

/// Additive migrations for databases created by older builds.
fn migrate(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(positions)")?;
    let cols: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if !cols.iter().any(|c| c == "uci") {
        conn.execute("ALTER TABLE positions ADD COLUMN uci TEXT", [])?;
    }
    let game_cols: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(games)")?;
        stmt.query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    if !game_cols.iter().any(|c| c == "movetext") {
        conn.execute("ALTER TABLE games ADD COLUMN movetext TEXT", [])?;
    }
    if !game_cols.iter().any(|c| c == "variant") {
        conn.execute("ALTER TABLE games ADD COLUMN variant TEXT", [])?;
    }
    let ann_cols: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(annotations)")?;
        stmt.query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    if !ann_cols.iter().any(|c| c == "path") {
        conn.execute("ALTER TABLE annotations ADD COLUMN path TEXT", [])?;
    }
    // Older builds inserted a new row per annotation save; keep the latest
    // annotation for each node and drop the duplicates.
    conn.execute(
        "DELETE FROM annotations WHERE id NOT IN
         (SELECT MAX(id) FROM annotations GROUP BY game_id, ply, COALESCE(path, ''))",
        [],
    )?;
    Ok(())
}

fn row_to_game(row: &rusqlite::Row<'_>) -> rusqlite::Result<Game> {
    let other_json: Option<String> = row.get(14)?;
    let other = other_json
        .as_deref()
        .and_then(|s| serde_json::from_str::<BTreeMap<String, String>>(s).ok());
    Ok(Game {
        id: row.get(0)?,
        event: row.get(1)?,
        site: row.get(2)?,
        date: row.get(3)?,
        round: row.get(4)?,
        white: row.get(5)?,
        white_elo: row.get(6)?,
        black: row.get(7)?,
        black_elo: row.get(8)?,
        result: row.get(9)?,
        fen: row.get(10)?,
        moves: row.get(11)?,
        ply_count: row.get(12)?,
        time_control: row.get(13)?,
        other,
        variant: row.get(15)?,
        added_at: row.get(16)?,
        movetext: row.get(17)?,
    })
}

fn get_game(conn: &Connection, id: i64) -> Result<Option<Game>> {
    let mut stmt = conn.prepare(
        "SELECT id, event, site, date, round, white, white_elo, black, black_elo,
                result, fen, moves, ply_count, time_control, other, variant, added_at, movetext
         FROM games WHERE id = ?1",
    )?;
    let mut rows = stmt.query_map(params![id], row_to_game)?;
    Ok(rows.next().transpose()?)
}

fn index_positions(
    conn: &Connection,
    game_id: i64,
    initial_fen: &str,
    ucis: &[String],
    variant: &str,
) -> Result<()> {
    let (mut pos, _) = variant_start(initial_fen, Some(variant))?;
    conn.execute(
        "INSERT OR REPLACE INTO positions (game_id, ply, key, uci) VALUES (?1, 0, ?2, NULL)",
        params![game_id, position_key_variant(&pos, variant)],
    )?;
    for (i, uci) in ucis.iter().enumerate() {
        let mv = UciMove::from_ascii(uci.as_bytes())?
            .to_move(&pos)
            .context("illegal move while indexing")?;
        pos = pos.play(mv)?;
        conn.execute(
            "INSERT OR REPLACE INTO positions (game_id, ply, key, uci) VALUES (?1, ?2, ?3, ?4)",
            params![game_id, (i + 1) as i64, position_key_variant(&pos, variant), uci],
        )?;
    }
    Ok(())
}

fn position_key_variant(pos: &shakmaty::variant::VariantPosition, variant: &str) -> String {
    let key = Fen::from_position(pos, EnPassantMode::Legal)
        .to_string()
        .split_whitespace()
        .take(4)
        .collect::<Vec<_>>()
        .join(" ");
    if variant == "standard" || variant == "chess960" {
        key
    } else {
        format!("{variant}|{key}")
    }
}

fn position_key(pos: &Chess) -> String {
    Fen::from_position(pos, EnPassantMode::Legal)
        .to_string()
        .split_whitespace()
        .take(4)
        .collect::<Vec<_>>()
        .join(" ")
}

fn board_contains(key: &str, required: &[(usize, char)]) -> bool {
    let board = key.split_whitespace().next().unwrap_or_default();
    let mut squares: [Option<char>; 64] = [None; 64];
    let mut rank: i32 = 7;
    let mut file = 0usize;
    for ch in board.chars() {
        match ch {
            '/' => {
                rank -= 1;
                file = 0;
            }
            '1'..='8' => file += ch.to_digit(10).unwrap_or(0) as usize,
            _ => {
                if (0..8).contains(&rank) && file < 8 {
                    squares[rank as usize * 8 + file] = Some(ch);
                    file += 1;
                }
            }
        }
    }
    required
        .iter()
        .all(|(idx, piece)| squares.get(*idx).and_then(|p| *p) == Some(*piece))
}

fn normalize_key(fen: &str) -> Result<String> {
    let fields: Vec<&str> = fen.split_whitespace().collect();
    let full = if fields.len() == 4 {
        format!("{} 0 1", fen.trim())
    } else if fields.len() == 6 {
        fen.trim().to_owned()
    } else {
        anyhow::bail!("FEN needs 4 or 6 fields");
    };
    let (pos, _) = parse_chess(&full).context("invalid FEN")?;
    Ok(position_key(&pos))
}

fn normalize_variant(value: Option<&str>) -> String {
    let value = value.unwrap_or("standard").trim().to_ascii_lowercase();
    match value.as_str() {
        "" | "standard" | "from position" | "chess" => "standard".into(),
        "chess960" | "chess 960" | "fischer random" | "fischerrandom" => "chess960".into(),
        "king of the hill" | "kingofthehill" => "kingofthehill".into(),
        "three-check" | "three check" | "threecheck" | "3check" => "3check".into(),
        other => other.to_owned(),
    }
}

fn row_to_repertoire_card(row: &rusqlite::Row<'_>) -> rusqlite::Result<RepertoireCard> {
    Ok(RepertoireCard {
        id: row.get(0)?,
        repertoire_id: row.get(1)?,
        fen: row.get(2)?,
        uci: row.get(3)?,
        san: row.get(4)?,
        due_at: row.get(5)?,
        interval_days: row.get(6)?,
        ease: row.get(7)?,
        reviews: row.get(8)?,
        lapses: row.get(9)?,
    })
}

fn collect_repertoire_cards(
    nodes: &[knightlm_notation::pgn::PgnNode],
    start: &shakmaty::variant::VariantPosition,
    mode: CastlingMode,
    want: shakmaty::Color,
    out: &mut Vec<(String, String, String)>,
) -> Result<()> {
    fn walk(
        nodes: &[knightlm_notation::pgn::PgnNode],
        pos: &shakmaty::variant::VariantPosition,
        mode: CastlingMode,
        want: shakmaty::Color,
        out: &mut Vec<(String, String, String)>,
    ) -> Result<()> {
        let mut current = pos.clone();
        for node in nodes {
            let mv = SanPlus::from_ascii(node.san.as_bytes())?
                .san
                .to_move(&current)?;
            if current.turn() == want {
                let fen_key = Fen::from_position(&current, EnPassantMode::Legal)
                    .to_string()
                    .split_whitespace()
                    .take(4)
                    .collect::<Vec<_>>()
                    .join(" ");
                out.push((
                    fen_key,
                    UciMove::from_move(mv, mode).to_string(),
                    SanPlus::from_move(current.clone(), mv).to_string(),
                ));
            }
            let next = current.clone().play(mv)?;
            for variation in &node.variations {
                walk(variation, &current, mode, want, out)?;
            }
            current = next;
        }
        Ok(())
    }
    walk(nodes, start, mode, want, out)
}

fn new_node(san: &str, clock: Option<&str>) -> knightlm_notation::pgn::PgnNode {
    knightlm_notation::pgn::PgnNode {
        san: san.to_owned(),
        nags: Vec::new(),
        pre_comment: None,
        post_comment: clock.map(|c| format!("[%clk {c}]")),
        variations: Vec::new(),
    }
}

fn set_clock_comment(node: &mut knightlm_notation::pgn::PgnNode, clock: &str) {
    let tag = format!("[%clk {clock}]");
    let comment = node.post_comment.get_or_insert_with(String::new);
    if let Some(start) = comment.find("[%clk ") {
        if let Some(end) = comment[start..].find(']') {
            comment.replace_range(start..start + end + 1, &tag);
            return;
        }
    }
    if !comment.is_empty() {
        comment.push(' ');
    }
    comment.push_str(&tag);
}

fn next_child_path(parent: &str, index: usize) -> String {
    if parent.is_empty() {
        return index.to_string();
    }
    if let Some((base, last)) = parent.rsplit_once('.') {
        if let Ok(last) = last.parse::<usize>() {
            return format!("{base}.{}", last + 1);
        }
    }
    parent
        .parse::<usize>()
        .map(|p| (p + 1).to_string())
        .unwrap_or_else(|_| index.to_string())
}

fn sans_along_path(
    nodes: &[knightlm_notation::pgn::PgnNode],
    path: &str,
    start: &shakmaty::variant::VariantPosition,
    mode: CastlingMode,
) -> Result<Vec<String>> {
    if path.is_empty() {
        return Ok(Vec::new());
    }
    let (first, rest) = split_tree_path(path)?;
    let mut out = Vec::new();
    let mut line = nodes;
    let mut idx = first;
    let mut segments = rest.as_slice();
    loop {
        for node in line.iter().take(idx + 1) {
            out.push(node.san.clone());
        }
        if segments.is_empty() {
            break;
        }
        let (variation, node) = segments[0];
        let owner = line.get(idx).context("move not found")?;
        line = owner.variations.get(variation).context("variation")?;
        idx = node;
        segments = &segments[1..];
    }
    // Validate on the way out so callers get a canonical position.
    let _ = play_sans(start, &out, mode)?;
    Ok(out)
}

fn play_sans(
    pos: &shakmaty::variant::VariantPosition,
    sans: &[String],
    _mode: CastlingMode,
) -> Result<shakmaty::variant::VariantPosition> {
    let mut pos = pos.clone();
    for san in sans {
        let mv = SanPlus::from_ascii(san.as_bytes())?
            .san
            .to_move(&pos)?;
        pos = pos.play(mv)?;
    }
    Ok(pos)
}

fn split_tree_path(path: &str) -> Result<(usize, Vec<(usize, usize)>)> {
    let mut segments = path.split('~');
    let first = segments
        .next()
        .unwrap_or("0")
        .parse::<usize>()
        .context("bad path")?;
    let mut rest = Vec::new();
    for segment in segments {
        let mut parts = segment.split('.');
        let j = parts.next().context("bad variation")?.parse::<usize>()?;
        let i = parts.next().context("bad variation node")?.parse::<usize>()?;
        rest.push((j, i));
    }
    Ok((first, rest))
}

fn resolve_tree_path<'a>(
    nodes: &'a mut Vec<knightlm_notation::pgn::PgnNode>,
    path: &str,
) -> Result<(&'a mut Vec<knightlm_notation::pgn::PgnNode>, usize)> {
    fn walk<'a>(
        line: &'a mut Vec<knightlm_notation::pgn::PgnNode>,
        idx: usize,
        rest: &[(usize, usize)],
    ) -> Result<(&'a mut Vec<knightlm_notation::pgn::PgnNode>, usize)> {
        if rest.is_empty() {
            anyhow::ensure!(idx < line.len(), "move not found");
            return Ok((line, idx));
        }
        let (variation, node) = rest[0];
        let owner = line.get_mut(idx).context("move not found")?;
        let branch = owner
            .variations
            .get_mut(variation)
            .context("variation not found")?;
        walk(branch, node, &rest[1..])
    }
    let (first, rest) = split_tree_path(path)?;
    walk(nodes, first, &rest)
}

fn prune_tree(nodes: &mut Vec<knightlm_notation::pgn::PgnNode>) {
    for node in nodes.iter_mut() {
        node.variations.retain(|line| !line.is_empty());
        for variation in &mut node.variations {
            prune_tree(variation);
        }
    }
}

fn variant_start(
    fen: &str,
    variant: Option<&str>,
) -> Result<(shakmaty::variant::VariantPosition, CastlingMode)> {
    let name = variant.unwrap_or("standard");
    let variant = variant_for_name(name);
    let mode = if variant == shakmaty::variant::Variant::Chess {
        if name.eq_ignore_ascii_case("chess960") {
            shakmaty::CastlingMode::Chess960
        } else {
            knightlm_core::state::castling_mode_for_fen(fen)
        }
    } else {
        CastlingMode::Standard
    };
    parse_variant_fen(fen, variant, mode)
}

fn validate_tree(
    nodes: &mut [knightlm_notation::pgn::PgnNode],
    start: &shakmaty::variant::VariantPosition,
    mode: CastlingMode,
) -> Result<(Vec<String>, Vec<String>)> {
    let mut position = start.clone();
    let mut sans = Vec::new();
    let mut ucis = Vec::new();
    for node in nodes.iter_mut() {
        let mv = SanPlus::from_ascii(node.san.as_bytes())?
            .san
            .to_move(&position)
            .with_context(|| format!("illegal move: {}", node.san))?;
        let san = SanPlus::from_move(position.clone(), mv).to_string();
        node.san = san.clone();
        for variation in &mut node.variations {
            validate_tree(variation, &position, mode)?;
        }
        ucis.push(UciMove::from_move(mv, mode).to_string());
        sans.push(san);
        position = position.play(mv)?;
    }
    Ok((sans, ucis))
}
