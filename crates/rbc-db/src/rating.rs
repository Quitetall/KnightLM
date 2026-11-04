//! Storage for the rating pool: staged games, the ledger, the current table and
//! the anchors.
//!
//! The Glicko-2 arithmetic and the period rules live in `rbc-ai`, which depends
//! on this crate; this module only knows how the rows are shaped. That keeps the
//! dependency direction honest and lets the maths be tested without a database.

use anyhow::Result;
use rusqlite::{Connection, params};

use crate::Db;

/// A game waiting for, or already part of, a rating period.
#[derive(Debug, Clone, PartialEq)]
pub struct RatedGame {
    pub period: i64,
    pub competitor: String,
    pub opponent: String,
    /// `1.0` win, `0.5` draw, `0.0` loss, from `competitor`'s side.
    pub score: f64,
    pub ruleset: String,
}

/// One side's before/after snapshot for a period.
#[derive(Debug, Clone, PartialEq)]
pub struct LedgerRow {
    pub period: i64,
    pub competitor: String,
    pub opponent: String,
    pub score: f64,
    pub before: (f64, f64, f64),
    pub after: (f64, f64, f64),
}

/// A competitor's current rating.
#[derive(Debug, Clone, PartialEq)]
pub struct CurrentRating {
    pub competitor: String,
    pub rating: f64,
    pub deviation: f64,
    pub volatility: f64,
    pub games: i64,
    pub period: i64,
}

/// A rating triple: value, deviation, volatility.
pub type RatingTriple = (f64, f64, f64);

impl Db {
    pub fn stage_rated_game(&self, game: &RatedGame) -> Result<()> {
        let conn = self.conn.lock().expect("db lock");
        conn.execute(
            "INSERT INTO rating_games (period, competitor, opponent, score, ruleset, created)
             VALUES (?1, ?2, ?3, ?4, ?5, unixepoch())",
            params![
                game.period,
                game.competitor,
                game.opponent,
                game.score,
                game.ruleset
            ],
        )?;
        Ok(())
    }

    /// The staged games for one period, in insertion order.
    pub fn rated_games(&self, period: i64) -> Result<Vec<RatedGame>> {
        let conn = self.conn.lock().expect("db lock");
        let mut statement = conn.prepare(
            "SELECT period, competitor, opponent, score, ruleset
             FROM rating_games WHERE period = ?1 ORDER BY id",
        )?;
        let rows = statement
            .query_map(params![period], |row| {
                Ok(RatedGame {
                    period: row.get(0)?,
                    competitor: row.get(1)?,
                    opponent: row.get(2)?,
                    score: row.get(3)?,
                    ruleset: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Every period that has staged games, oldest first.
    pub fn rating_periods(&self) -> Result<Vec<i64>> {
        let conn = self.conn.lock().expect("db lock");
        let mut statement =
            conn.prepare("SELECT DISTINCT period FROM rating_games ORDER BY period")?;
        let rows = statement
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn set_rating_anchor(&self, competitor: &str, rating: (f64, f64, f64)) -> Result<()> {
        let conn = self.conn.lock().expect("db lock");
        conn.execute(
            "INSERT OR REPLACE INTO rating_anchors (competitor, rating, deviation, volatility)
             VALUES (?1, ?2, ?3, ?4)",
            params![competitor, rating.0, rating.1, rating.2],
        )?;
        Ok(())
    }

    pub fn rating_anchors(&self) -> Result<Vec<(String, RatingTriple)>> {
        let conn = self.conn.lock().expect("db lock");
        let mut statement =
            conn.prepare("SELECT competitor, rating, deviation, volatility FROM rating_anchors")?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get(0)?, (row.get(1)?, row.get(2)?, row.get(3)?)))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Write one competitor's current rating, replacing what was there.
    pub fn upsert_rating(
        &self,
        competitor: &str,
        rating: RatingTriple,
        games: i64,
        period: i64,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("db lock");
        conn.execute(
            "INSERT INTO ratings (competitor, rating, deviation, volatility, games, period, updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, unixepoch())
             ON CONFLICT(competitor) DO UPDATE SET
                 rating = excluded.rating,
                 deviation = excluded.deviation,
                 volatility = excluded.volatility,
                 games = ratings.games + excluded.games,
                 period = excluded.period,
                 updated = excluded.updated",
            params![competitor, rating.0, rating.1, rating.2, games, period],
        )?;
        Ok(())
    }

    /// A competitor's stored rating, if it has played.
    pub fn rating_of(&self, competitor: &str) -> Result<Option<RatingTriple>> {
        let conn = self.conn.lock().expect("db lock");
        Ok(conn
            .query_row(
                "SELECT rating, deviation, volatility FROM ratings WHERE competitor = ?1",
                params![competitor],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .ok())
    }

    pub fn append_rating_ledger(&self, row: &LedgerRow) -> Result<()> {
        let conn = self.conn.lock().expect("db lock");
        conn.execute(
            "INSERT INTO rating_ledger
             (period, competitor, opponent, score,
              before_rating, before_deviation, before_volatility,
              after_rating, after_deviation, after_volatility, created)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, unixepoch())",
            params![
                row.period,
                row.competitor,
                row.opponent,
                row.score,
                row.before.0,
                row.before.1,
                row.before.2,
                row.after.0,
                row.after.1,
                row.after.2
            ],
        )?;
        Ok(())
    }

    pub fn rating_ledger(&self) -> Result<Vec<LedgerRow>> {
        let conn = self.conn.lock().expect("db lock");
        let mut statement = conn.prepare(
            "SELECT period, competitor, opponent, score,
                    before_rating, before_deviation, before_volatility,
                    after_rating, after_deviation, after_volatility
             FROM rating_ledger ORDER BY id",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok(LedgerRow {
                    period: row.get(0)?,
                    competitor: row.get(1)?,
                    opponent: row.get(2)?,
                    score: row.get(3)?,
                    before: (row.get(4)?, row.get(5)?, row.get(6)?),
                    after: (row.get(7)?, row.get(8)?, row.get(9)?),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn current_ratings(&self) -> Result<Vec<CurrentRating>> {
        let conn = self.conn.lock().expect("db lock");
        let mut statement = conn.prepare(
            "SELECT competitor, rating, deviation, volatility, games, period FROM ratings",
        )?;
        let mut rows = statement
            .query_map([], |row| {
                Ok(CurrentRating {
                    competitor: row.get(0)?,
                    rating: row.get(1)?,
                    deviation: row.get(2)?,
                    volatility: row.get(3)?,
                    games: row.get(4)?,
                    period: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.sort_by(|a, b| {
            let rank = |r: &CurrentRating| r.rating - 2.0 * r.deviation;
            rank(b)
                .partial_cmp(&rank(a))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(rows)
    }

    /// Drop the cache and the ledger; the staging table is the history.
    pub fn clear_ratings(&self) -> Result<()> {
        let conn = self.conn.lock().expect("db lock");
        conn.execute("DELETE FROM ratings", [])?;
        conn.execute("DELETE FROM rating_ledger", [])?;
        Ok(())
    }
}

/// Nothing here reads the connection directly; this keeps `Connection` in the
/// import list for the helpers above that need its type.
#[allow(dead_code)]
fn _connection_marker(_conn: &Connection) {}
