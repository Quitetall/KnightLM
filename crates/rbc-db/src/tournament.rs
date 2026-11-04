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
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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

/// Usage per model, aggregated from the session log.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageRow {
    pub model: String,
    pub turns: i64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub errors: i64,
    pub average_latency_ms: f64,
    pub duration_ms: i64,
}

/// How often a model's moves were legal first time, per match.
#[derive(Debug, Clone, PartialEq)]
pub struct AttemptsRow {
    pub model: String,
    pub plies: i64,
    pub clean_plies: i64,
}

impl Db {
    /// Usage per model, from the turns the session log already holds.
    pub fn ai_usage_by_model(&self) -> Result<Vec<UsageRow>> {
        let conn = self.conn.lock().expect("db lock");
        let mut statement = conn.prepare(
            "SELECT
                 COALESCE(json_extract(model_json, '$.model'), '') AS model,
                 COUNT(*) AS turns,
                 SUM(tokens_in) AS tokens_in,
                 SUM(tokens_out) AS tokens_out,
                 SUM(CASE WHEN error_json IS NOT NULL AND error_json != '' THEN 1 ELSE 0 END) AS errors,
                 AVG(latency_ms) AS latency,
                 SUM(latency_ms) AS duration
             FROM ai_turns
             WHERE role = 'assistant'
             GROUP BY model
             ORDER BY turns DESC",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok(UsageRow {
                    model: row.get(0)?,
                    turns: row.get(1)?,
                    tokens_in: row.get(2)?,
                    tokens_out: row.get(3)?,
                    errors: row.get(4)?,
                    average_latency_ms: row.get(5)?,
                    duration_ms: row.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Plies played per model seat, and how many needed only one attempt.
    ///
    /// The attempts live inside each match's state JSON, which is why this walks
    /// the recent matches rather than aggregating in SQL.
    pub fn ai_match_attempts(&self) -> Result<Vec<AttemptsRow>> {
        let conn = self.conn.lock().expect("db lock");
        let mut statement = conn.prepare(
            "SELECT white_json, black_json, state_json FROM ai_matches ORDER BY created DESC LIMIT 200",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        drop(conn);

        let mut totals: std::collections::BTreeMap<String, (i64, i64)> =
            std::collections::BTreeMap::new();
        for (white_json, black_json, state_json) in rows {
            let seat = |json: &str| -> String {
                serde_json::from_str::<serde_json::Value>(json)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("model")
                            .and_then(|m| m.as_str())
                            .map(str::to_string)
                    })
                    .unwrap_or_default()
            };
            let white = seat(&white_json);
            let black = seat(&black_json);
            let Ok(state) = serde_json::from_str::<serde_json::Value>(&state_json) else {
                continue;
            };
            let Some(moves) = state.get("moves").and_then(|m| m.as_array()) else {
                continue;
            };
            for mv in moves {
                let color = mv.get("color").and_then(|c| c.as_str()).unwrap_or("");
                let attempts = mv.get("attempts").and_then(|a| a.as_u64()).unwrap_or(0);
                if attempts == 0 {
                    continue; // a human or a replayed move
                }
                let model = if color == "white" { &white } else { &black };
                if model.is_empty() {
                    continue;
                }
                let entry = totals.entry(model.clone()).or_insert((0, 0));
                entry.0 += 1;
                if attempts == 1 {
                    entry.1 += 1;
                }
            }
        }
        Ok(totals
            .into_iter()
            .map(|(model, (plies, clean))| AttemptsRow {
                model,
                plies,
                clean_plies: clean,
            })
            .collect())
    }
}
