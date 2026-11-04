//! Keeping a running tournament alive across a restart.
//!
//! The schedule, its queue and its budget are one JSON blob (a tournament is a
//! domain object; the database's job is to keep it whole), plus a projected
//! standings table so a leaderboard does not have to replay anything.

use anyhow::Result;
use rbc_db::tournament::{StandingRow, TournamentRow};

use crate::tournament::{Running, Status};

/// What a `tournaments` row says a schedule format is.
fn format_name(running: &Running) -> &'static str {
    match running.schedule.format {
        crate::tournament::Format::Swiss { .. } => "swiss",
        crate::tournament::Format::Knockout => "knockout",
        crate::tournament::Format::DoubleRoundRobin => "double-round-robin",
    }
}

/// Persist a tournament: its state, its status and its projected standings.
pub fn save(db: &rbc_db::Db, running: &Running, pgn: &str) -> Result<()> {
    let state_json = serde_json::to_string(running)?;
    db.save_tournament(&TournamentRow {
        id: running.id.clone(),
        name: running.name.clone(),
        format: format_name(running).to_string(),
        status: running.status.as_str().to_string(),
        state_json,
        pgn: pgn.to_string(),
    })?;
    let rows: Vec<StandingRow> = running
        .standings()
        .into_iter()
        .map(|standing| StandingRow {
            competitor: standing.id,
            points: standing.points,
            played: standing.played as i64,
            wins: standing.wins as i64,
            buchholz: standing.buchholz,
            sonneborn_berger: standing.sonneborn_berger,
        })
        .collect();
    db.save_standings(&running.id, &rows)?;
    Ok(())
}

/// Load a tournament back, exactly as it was left.
///
/// If the process stopped while it was running, it comes back running: the
/// games in flight were never recorded, so they are still in the queue.
pub fn load(db: &rbc_db::Db, id: &str) -> Result<Option<Running>> {
    let Some(row) = db.tournament(id)? else {
        return Ok(None);
    };
    let mut running: Running = serde_json::from_str(&row.state_json)?;
    // The stored status is the authority: a pause survives a restart.
    running.status = Status::parse(&row.status);
    Ok(Some(running))
}

/// Every stored tournament.
pub fn all(db: &rbc_db::Db) -> Result<Vec<Running>> {
    let mut out = Vec::new();
    for row in db.tournaments()? {
        if let Ok(mut running) = serde_json::from_str::<Running>(&row.state_json) {
            running.status = Status::parse(&row.status);
            out.push(running);
        }
    }
    Ok(out)
}

/// The projected standings for a tournament, best first.
pub fn standings(db: &rbc_db::Db, id: &str) -> Result<Vec<StandingRow>> {
    db.standings_of(id)
}
