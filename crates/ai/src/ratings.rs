//! The rating pool's rules: which games count, how a period is applied, and how
//! the cache is rebuilt from the history.
//!
//! Storage is `rbc_db::rating`; the arithmetic is `crate::glicko`. This module
//! is where the two meet, and it is deliberately the only place that decides
//! what a period *is*, so a tournament can close one period over many games and
//! an ad-hoc match can be a period of one.

use std::collections::BTreeMap;

use anyhow::Result;
use rbc_db::rating::{CurrentRating, LedgerRow, RatedGame};

use crate::glicko::{Game, GameKind, GlickoConfig, Rating, is_rateable, unrateable_reason};

/// One pool per ruleset; only `standard-v1` exists today.
pub const RATED_RULESET: &str = "standard-v1";

/// What a game needs to be staged.
#[derive(Debug, Clone, PartialEq)]
pub struct RatingGame {
    /// When this game happened, as a period number.
    pub period: i64,
    pub white: String,
    pub black: String,
    pub kind: GameKind,
    /// `1.0` white wins, `0.5` draw, `0.0` black wins.
    pub score: f64,
}

/// Refuse a game that must not feed the pool, saying why.
pub fn stage(db: &rbc_db::Db, game: &RatingGame) -> Result<()> {
    if !is_rateable(game.kind, RATED_RULESET) {
        anyhow::bail!(
            "{}",
            unrateable_reason(game.kind, RATED_RULESET).unwrap_or_else(|| "not rateable".into())
        );
    }
    // Both sides, from their own point of view.
    let white = RatedGame {
        period: game.period,
        competitor: game.white.clone(),
        opponent: game.black.clone(),
        score: game.score,
        ruleset: RATED_RULESET.to_string(),
    };
    let black = RatedGame {
        period: game.period,
        competitor: game.black.clone(),
        opponent: game.white.clone(),
        score: 1.0 - game.score,
        ruleset: RATED_RULESET.to_string(),
    };
    db.stage_rated_game(&white)?;
    db.stage_rated_game(&black)?;
    Ok(())
}

/// Apply one period: one update per competitor, from the whole period.
pub fn apply_period(db: &rbc_db::Db, period: i64) -> Result<Vec<LedgerRow>> {
    // One pool per ruleset: a row for another variant must not leak into it.
    let staged: Vec<_> = db
        .rated_games(period)?
        .into_iter()
        .filter(|game| game.ruleset == RATED_RULESET)
        .collect();
    if staged.is_empty() {
        return Ok(Vec::new());
    }
    let anchors: BTreeMap<String, Rating> = db
        .rating_anchors()?
        .into_iter()
        .map(|(who, (r, rd, vol))| (who, Rating::new(r, rd, vol)))
        .collect();

    // Everything is measured against the state as the period opens, so the order
    // games were staged in cannot change the answer.
    let mut opening: BTreeMap<String, Rating> = BTreeMap::new();
    for game in &staged {
        for who in [&game.competitor, &game.opponent] {
            if !opening.contains_key(who) {
                let state = match anchors.get(who) {
                    Some(anchor) => *anchor,
                    None => db
                        .rating_of(who)?
                        .map(|(r, rd, vol)| Rating::new(r, rd, vol))
                        .unwrap_or_default(),
                };
                opening.insert(who.clone(), state);
            }
        }
    }

    let mut by_competitor: BTreeMap<String, Vec<&RatedGame>> = BTreeMap::new();
    for game in &staged {
        by_competitor
            .entry(game.competitor.clone())
            .or_default()
            .push(game);
    }

    let mut ledger = Vec::new();
    for (competitor, games) in by_competitor {
        let before = opening[&competitor];
        let played: Vec<Game> = games
            .iter()
            .map(|game| Game {
                opponent: opening[&game.opponent],
                score: game.score,
            })
            .collect();
        let computed = crate::glicko::update(before, &played, GlickoConfig::default());
        // An anchor's own rating never moves; its opponents still feel the game.
        let after = if anchors.contains_key(&competitor) {
            before
        } else {
            computed
        };

        if !anchors.contains_key(&competitor) {
            db.upsert_rating(
                &competitor,
                (after.rating, after.deviation, after.volatility),
                games.len() as i64,
                period,
            )?;
        }
        for game in &games {
            let row = LedgerRow {
                period,
                competitor: competitor.clone(),
                opponent: game.opponent.clone(),
                score: game.score,
                before: (before.rating, before.deviation, before.volatility),
                after: (after.rating, after.deviation, after.volatility),
            };
            db.append_rating_ledger(&row)?;
            ledger.push(row);
        }
    }
    Ok(ledger)
}

/// Rebuild the cache and the ledger from the staged history.
///
/// If the cache and a replay ever disagree, the replay is right: the staging
/// rows are the record, `ratings` is only a cache over them.
pub fn rebuild(db: &rbc_db::Db) -> Result<Vec<CurrentRating>> {
    db.clear_ratings()?;
    for period in db.rating_periods()? {
        apply_period(db, period)?;
    }
    db.current_ratings()
}

/// The current ratings, best conservative rank first.
pub fn leaderboard(db: &rbc_db::Db) -> Result<Vec<CurrentRating>> {
    db.current_ratings()
}

// --- the leaderboard and its metrics ---------------------------------------

/// One row of the leaderboard: a rating plus what it took to get it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LeaderboardRow {
    pub competitor: String,
    pub rating: f64,
    pub deviation: f64,
    pub volatility: f64,
    /// `rating - 2*RD`, the rank a leaderboard sorts and shows.
    pub conservative: f64,
    pub games: i64,
    pub period: i64,
    /// The last change from the ledger, if this competitor has one.
    pub last_change: Option<f64>,
}

/// What a model's turns cost and how often it needed correcting.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize)]
pub struct ModelMetrics {
    pub model: String,
    pub turns: i64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub errors: i64,
    pub average_latency_ms: f64,
    pub duration_ms: i64,
    /// Model plies taken from matches, and how many were legal first time.
    pub plies: i64,
    pub clean_plies: i64,
    /// `clean_plies / plies`, or `None` when the model has not played.
    pub legal_rate: Option<f64>,
}

/// The leaderboard, best conservative rank first.
pub fn leaderboard_rows(db: &rbc_db::Db) -> Result<Vec<LeaderboardRow>> {
    let ledger = db.rating_ledger()?;
    let mut last: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for row in &ledger {
        // The ledger is oldest first, so the last write wins.
        last.insert(row.competitor.clone(), (row.before.0, row.after.0));
    }
    Ok(db
        .current_ratings()?
        .into_iter()
        .map(|row| LeaderboardRow {
            conservative: crate::glicko::conservative_rank(crate::glicko::Rating::new(
                row.rating,
                row.deviation,
                row.volatility,
            )),
            competitor: row.competitor.clone(),
            rating: row.rating,
            deviation: row.deviation,
            volatility: row.volatility,
            games: row.games,
            period: row.period,
            last_change: last
                .get(&row.competitor)
                .map(|(before, after)| after - before),
        })
        .collect())
}

/// Usage per model, with the match-side legal rate folded in.
///
/// Turns are counted from the session log; plies and attempts come from the
/// stored matches, which is where a correction is recorded.
pub fn metrics(db: &rbc_db::Db) -> Result<Vec<ModelMetrics>> {
    let mut by_model: BTreeMap<String, ModelMetrics> = BTreeMap::new();
    for row in db.ai_usage_by_model()? {
        by_model.insert(
            row.model.clone(),
            ModelMetrics {
                model: row.model,
                turns: row.turns,
                tokens_in: row.tokens_in,
                tokens_out: row.tokens_out,
                errors: row.errors,
                average_latency_ms: row.average_latency_ms,
                duration_ms: row.duration_ms,
                ..ModelMetrics::default()
            },
        );
    }
    for row in db.ai_match_attempts()? {
        let entry = by_model
            .entry(row.model.clone())
            .or_insert_with(|| ModelMetrics {
                model: row.model.clone(),
                ..ModelMetrics::default()
            });
        entry.plies += row.plies;
        entry.clean_plies += row.clean_plies;
    }
    for entry in by_model.values_mut() {
        entry.legal_rate = (entry.plies > 0).then(|| entry.clean_plies as f64 / entry.plies as f64);
    }
    Ok(by_model.into_values().collect())
}
