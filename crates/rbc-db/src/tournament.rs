//! Storage for tournaments: the state a restart resumes from, and a standings
//! projection a leaderboard can read without replaying anything.
//!
//! The state itself is JSON, the same way a match stores its snapshot: the
//! schedule is a domain object and the database's job is to keep it whole.

use anyhow::Result;
use rusqlite::params;

use crate::Db;

/// A stored tournament.
#[derive(Debug, Clone, PartialEq)]
pub struct TournamentRow {
    pub id: String,
    pub name: String,
    pub format: String,
    pub status: String,
    pub state_json: String,
    pub pgn: String,
}

/// One projected standing.
#[derive(Debug, Clone, PartialEq)]
pub struct StandingRow {
    pub competitor: String,
    pub points: f64,
    pub played: i64,
    pub wins: i64,
    pub buchholz: f64,
    pub sonneborn_berger: f64,
}

impl Db {
    /// Create or replace a tournament's stored state.
    pub fn save_tournament(&self, row: &TournamentRow) -> Result<()> {
        let conn = self.conn.lock().expect("db lock");
        conn.execute(
            "INSERT INTO tournaments (id, name, format, status, state_json, pgn, created, updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, unixepoch(), unixepoch())
             ON CONFLICT(id) DO UPDATE SET
                 name = excluded.name,
                 format = excluded.format,
                 status = excluded.status,
                 state_json = excluded.state_json,
                 pgn = excluded.pgn,
                 updated = excluded.updated",
            params![
                row.id,
                row.name,
                row.format,
                row.status,
                row.state_json,
                row.pgn
            ],
        )?;
        Ok(())
    }

    /// Load one tournament, or `None` if it is not there.
    pub fn tournament(&self, id: &str) -> Result<Option<TournamentRow>> {
        let conn = self.conn.lock().expect("db lock");
        Ok(conn
            .query_row(
                "SELECT id, name, format, status, state_json, pgn FROM tournaments WHERE id = ?1",
                params![id],
                |row| {
                    Ok(TournamentRow {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        format: row.get(2)?,
                        status: row.get(3)?,
                        state_json: row.get(4)?,
                        pgn: row.get(5)?,
                    })
                },
            )
            .ok())
    }

    /// Every tournament, newest first.
    pub fn tournaments(&self) -> Result<Vec<TournamentRow>> {
        let conn = self.conn.lock().expect("db lock");
        let mut statement = conn.prepare(
            "SELECT id, name, format, status, state_json, pgn FROM tournaments ORDER BY created DESC",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok(TournamentRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    format: row.get(2)?,
                    status: row.get(3)?,
                    state_json: row.get(4)?,
                    pgn: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Replace a tournament's projected standings.
    pub fn save_standings(&self, tournament_id: &str, rows: &[StandingRow]) -> Result<()> {
        let conn = self.conn.lock().expect("db lock");
        conn.execute(
            "DELETE FROM tournament_standings WHERE tournament_id = ?1",
            params![tournament_id],
        )?;
        for row in rows {
            conn.execute(
                "INSERT INTO tournament_standings
                 (tournament_id, competitor, points, played, wins, buchholz, sonneborn_berger, calculated)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, unixepoch())",
                params![
                    tournament_id,
                    row.competitor,
                    row.points,
                    row.played,
                    row.wins,
                    row.buchholz,
                    row.sonneborn_berger
                ],
            )?;
        }
        Ok(())
    }

    /// The standings as last projected, best first.
    pub fn standings_of(&self, tournament_id: &str) -> Result<Vec<StandingRow>> {
        let conn = self.conn.lock().expect("db lock");
        let mut statement = conn.prepare(
            "SELECT competitor, points, played, wins, buchholz, sonneborn_berger
             FROM tournament_standings WHERE tournament_id = ?1
             ORDER BY points DESC, buchholz DESC, sonneborn_berger DESC, wins DESC, competitor",
        )?;
        let rows = statement
            .query_map(params![tournament_id], |row| {
                Ok(StandingRow {
                    competitor: row.get(0)?,
                    points: row.get(1)?,
                    played: row.get(2)?,
                    wins: row.get(3)?,
                    buchholz: row.get(4)?,
                    sonneborn_berger: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
