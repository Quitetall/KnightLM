//! Glicko-2 against the paper's own numbers, and the rules around a rating.
#![cfg(feature = "core")]

use rbc_ai::glicko::{
    Anchor, Game, GameKind, GlickoConfig, Rating, competitor_id, conservative_rank, is_rateable,
    unrateable_reason, update,
};

fn close(actual: f64, expected: f64, tolerance: f64) -> bool {
    (actual - expected).abs() <= tolerance
}

#[test]
fn glicko_known_vector_from_the_paper() {
    // Glickman, "Example of the Glicko-2 system": a player at 1500 with RD 200
    // and volatility 0.06 plays three opponents in one period, winning one and
    // losing two. The paper's answer is 1464.06 / 151.52 / 0.05999.
    let player = Rating::new(1500.0, 200.0, 0.06);
    let games = [
        Game::win(Rating::new(1400.0, 30.0, 0.06)),
        Game::loss(Rating::new(1550.0, 100.0, 0.06)),
        Game::loss(Rating::new(1700.0, 300.0, 0.06)),
    ];

    let after = update(player, &games, GlickoConfig::default());

    assert!(
        close(after.rating, 1464.06, 0.05),
        "rating {} should be 1464.06",
        after.rating
    );
    assert!(
        close(after.deviation, 151.52, 0.05),
        "deviation {} should be 151.52",
        after.deviation
    );
    assert!(
        close(after.volatility, 0.05999, 0.00001),
        "volatility {} should be 0.05999",
        after.volatility
    );
}

#[test]
fn glicko_a_quiet_period_only_widens_the_deviation() {
    let player = Rating::new(1600.0, 120.0, 0.06);
    let after = update(player, &[], GlickoConfig::default());

    assert_eq!(after.rating, player.rating, "no games, no rating change");
    assert_eq!(after.volatility, player.volatility);
    assert!(
        after.deviation > player.deviation,
        "an idle player is less certain: {} -> {}",
        player.deviation,
        after.deviation
    );
}

#[test]
fn glicko_a_win_against_an_equal_player_gains_and_narrows() {
    let player = Rating::new(1500.0, 100.0, 0.06);
    let opponent = Rating::new(1500.0, 100.0, 0.06);
    let after = update(player, &[Game::win(opponent)], GlickoConfig::default());

    assert!(after.rating > 1500.0, "a win gains: {}", after.rating);
    assert!(
        after.deviation < player.deviation,
        "one more result is more certainty: {} -> {}",
        player.deviation,
        after.deviation
    );
}

#[test]
fn glicko_a_loss_against_an_equal_player_loses_the_same_amount() {
    let player = Rating::new(1500.0, 100.0, 0.06);
    let opponent = Rating::new(1500.0, 100.0, 0.06);
    let win = update(player, &[Game::win(opponent)], GlickoConfig::default());
    let loss = update(player, &[Game::loss(opponent)], GlickoConfig::default());

    let gained = win.rating - 1500.0;
    let lost = 1500.0 - loss.rating;
    assert!(
        close(gained, lost, 0.01),
        "symmetry: gained {gained}, lost {lost}"
    );
}

#[test]
fn glicko_beating_a_stronger_player_is_worth_more() {
    let player = Rating::new(1500.0, 100.0, 0.06);
    let stronger = update(
        player,
        &[Game::win(Rating::new(1800.0, 60.0, 0.06))],
        GlickoConfig::default(),
    );
    let weaker = update(
        player,
        &[Game::win(Rating::new(1200.0, 60.0, 0.06))],
        GlickoConfig::default(),
    );
    assert!(
        stronger.rating > weaker.rating,
        "beating 1800 ({}) should beat beating 1200 ({})",
        stronger.rating,
        weaker.rating
    );
}

#[test]
fn glicko_one_period_is_not_the_same_as_two_updates() {
    // The paper's point: a period is applied once, from the whole period. Doing
    // it game by game gives a different, wrong, answer.
    let player = Rating::new(1500.0, 200.0, 0.06);
    let games = [
        Game::win(Rating::new(1400.0, 30.0, 0.06)),
        Game::loss(Rating::new(1550.0, 100.0, 0.06)),
    ];
    let period = update(player, &games, GlickoConfig::default());
    let sequential = update(
        update(player, &games[..1], GlickoConfig::default()),
        &games[1..],
        GlickoConfig::default(),
    );

    assert!(
        !close(period.rating, sequential.rating, 0.01),
        "period {} and sequential {} must differ",
        period.rating,
        sequential.rating
    );
}

#[test]
fn glicko_tau_bounds_how_far_volatility_moves() {
    let player = Rating::new(1500.0, 200.0, 0.06);
    let games = [
        Game::win(Rating::new(1400.0, 30.0, 0.06)),
        Game::loss(Rating::new(1700.0, 300.0, 0.06)),
    ];
    let steady = update(
        player,
        &games,
        GlickoConfig {
            tau: 0.3,
            ..GlickoConfig::default()
        },
    );
    let jumpy = update(
        player,
        &games,
        GlickoConfig {
            tau: 1.2,
            ..GlickoConfig::default()
        },
    );
    // What tau controls is how far volatility may move from where it started,
    // not which side of 0.06 it lands on.
    let moved = |after: rbc_ai::glicko::Rating| (after.volatility - 0.06).abs();
    assert!(
        moved(jumpy) > moved(steady),
        "a larger tau allows more movement: {} vs {}",
        moved(jumpy),
        moved(steady)
    );
}

// --- what is rated, and what a leaderboard shows ---------------------------

#[test]
fn rating_only_accepts_model_versus_model_under_standard_v1() {
    assert!(is_rateable(GameKind::ModelVsModel, "standard-v1"));
    assert!(
        !is_rateable(GameKind::Human, "standard-v1"),
        "human vs AI is unrated"
    );
    assert!(!is_rateable(GameKind::Casual, "standard-v1"));
    assert!(!is_rateable(GameKind::ToolAssisted, "standard-v1"));
    assert!(!is_rateable(GameKind::Aborted, "standard-v1"));
    assert!(
        !is_rateable(GameKind::ProviderFailed, "standard-v1"),
        "a provider failure is not a result"
    );
    assert!(
        !is_rateable(GameKind::ModelVsModel, "chess960-v1"),
        "only standard-v1 has a pool"
    );
}

#[test]
fn every_unrated_game_can_say_why() {
    assert_eq!(
        unrateable_reason(GameKind::ModelVsModel, "standard-v1"),
        None
    );
    assert_eq!(
        unrateable_reason(GameKind::ModelVsModel, "chess960-v1").as_deref(),
        Some("ruleset chess960-v1 is not rated")
    );
    assert_eq!(
        unrateable_reason(GameKind::ProviderFailed, "standard-v1").as_deref(),
        Some("provider-failed games are unrated")
    );
}

#[test]
fn conservative_rank_sits_two_deviations_below_the_rating() {
    assert_eq!(conservative_rank(Rating::new(1600.0, 100.0, 0.06)), 1400.0);
    // A new competitor is not ranked as though 1500 were measured.
    let new_player = Rating::default();
    assert_eq!(conservative_rank(new_player), 800.0);
}

#[test]
fn a_competitor_id_names_the_version() {
    assert_eq!(
        competitor_id("openrouter", "qwen3-4b", "2026-10"),
        "llm:openrouter:qwen3-4b:2026-10"
    );
    // A different version is a different competitor, not a rating carried over.
    assert_ne!(
        competitor_id("openrouter", "qwen3-4b", "2026-10"),
        competitor_id("openrouter", "qwen3-4b", "2026-11")
    );
}

#[test]
fn an_engine_anchor_is_pinned_at_its_nominal_strength() {
    let anchor = Anchor::engine("stockfish-19@1.5", 2500.0);
    assert_eq!(anchor.competitor, "stockfish-19@1.5");
    assert_eq!(anchor.rating.rating, 2500.0);
    assert!(anchor.rating.is_established(), "a narrow deviation");
    // An anchor's own rating never moves; honouring that is the ledger's job.
    let moved = update(
        anchor.rating,
        &[Game::loss(Rating::new(1200.0, 60.0, 0.06))],
        GlickoConfig::default(),
    );
    assert!(
        moved.rating < anchor.rating.rating,
        "the maths would move it, which is why the anchor is data"
    );
}

// --- the pool: staging, periods, ledger and rebuild ------------------------

#[cfg(feature = "host")]
mod pool {
    use rbc_ai::glicko::{GameKind, Rating};
    use rbc_ai::ratings::{RATED_RULESET, RatingGame, apply_period, leaderboard, rebuild, stage};

    fn game(period: i64, white: &str, black: &str, score: f64) -> RatingGame {
        RatingGame {
            period,
            white: white.to_string(),
            black: black.to_string(),
            kind: GameKind::ModelVsModel,
            score,
        }
    }

    #[test]
    fn pool_one_game_is_one_period_and_the_ledger_says_so() {
        let db = rbc_db::Db::open_memory().expect("db");
        stage(&db, &game(1, "llm:a:1", "llm:b:1", 1.0)).expect("stage");
        let ledger = apply_period(&db, 1).expect("apply");

        assert_eq!(ledger.len(), 2, "one row per side");
        let winner = ledger
            .iter()
            .find(|row| row.competitor == "llm:a:1")
            .expect("winner row");
        assert_eq!(winner.before.0, 1500.0, "a new competitor starts at 1500");
        assert!(winner.after.0 > 1500.0, "a win gains: {}", winner.after.0);
        assert!(
            winner.after.1 < winner.before.1,
            "and narrows the deviation"
        );

        let board = leaderboard(&db).expect("leaderboard");
        assert_eq!(board.len(), 2);
        assert_eq!(board[0].competitor, "llm:a:1", "the winner leads");
        assert_eq!(board[0].games, 1);
    }

    #[test]
    fn pool_refuses_a_game_that_must_not_be_rated() {
        let db = rbc_db::Db::open_memory().expect("db");
        let mut casual = game(1, "llm:a:1", "llm:b:1", 1.0);
        casual.kind = GameKind::Human;
        let failure = stage(&db, &casual).expect_err("human games are unrated");
        assert!(
            failure.to_string().contains("human"),
            "the refusal names the reason: {failure}"
        );
        assert!(
            db.rating_periods().expect("periods").is_empty(),
            "nothing was staged"
        );
    }

    #[test]
    fn pool_an_anchor_never_moves_but_its_opponent_does() {
        let db = rbc_db::Db::open_memory().expect("db");
        db.set_rating_anchor("engine:stockfish", (3000.0, 50.0, 0.06))
            .expect("anchor");
        stage(&db, &game(1, "llm:a:1", "engine:stockfish", 1.0)).expect("stage");
        apply_period(&db, 1).expect("apply");

        let board = leaderboard(&db).expect("leaderboard");
        let fixed = board.iter().find(|r| r.competitor == "engine:stockfish");
        assert!(fixed.is_none(), "an anchor has no movable row");
        let anchors = db.rating_anchors().expect("anchors");
        assert_eq!(
            anchors[0].1.0, 3000.0,
            "the anchor is exactly where it was set"
        );

        // Beating 3000 is worth a lot more than beating 1500.
        let gains = [("llm:a:1", 1.0)];
        let _ = gains;
        let learner = board
            .iter()
            .find(|r| r.competitor == "llm:a:1")
            .expect("row");
        assert!(
            learner.rating > 1600.0,
            "beat 3000 and you gain: {}",
            learner.rating
        );
    }

    #[test]
    fn pool_rebuild_reproduces_the_cache_from_history() {
        let db = rbc_db::Db::open_memory().expect("db");
        for period in 1..=3 {
            stage(
                &db,
                &game(
                    period,
                    "llm:a:1",
                    "llm:b:1",
                    if period == 2 { 0.0 } else { 1.0 },
                ),
            )
            .expect("stage");
            apply_period(&db, period).expect("apply");
        }
        let before = leaderboard(&db).expect("before");
        let ledger_before = db.rating_ledger().expect("ledger");

        let after = rebuild(&db).expect("rebuild");
        let ledger_after = db.rating_ledger().expect("ledger");

        assert_eq!(before.len(), after.len());
        for (a, b) in before.iter().zip(after.iter()) {
            assert_eq!(a.competitor, b.competitor);
            assert!(
                (a.rating - b.rating).abs() < 1e-9,
                "{} was {} and rebuilt to {}",
                a.competitor,
                a.rating,
                b.rating
            );
            assert!(a.games == b.games, "game counts match");
        }
        assert_eq!(
            ledger_before.len(),
            ledger_after.len(),
            "the ledger is reproducible"
        );
    }

    #[test]
    fn pool_a_period_of_many_games_matches_its_own_rebuild() {
        // A tournament: two competitors, several games, one period.
        let db = rbc_db::Db::open_memory().expect("db");
        stage(&db, &game(1, "llm:a:1", "llm:b:1", 1.0)).expect("one");
        stage(&db, &game(1, "llm:a:1", "llm:c:1", 0.5)).expect("two");
        stage(&db, &game(1, "llm:b:1", "llm:c:1", 1.0)).expect("three");
        let ledger = apply_period(&db, 1).expect("apply");
        assert_eq!(ledger.len(), 6, "three games, two rows each");

        let direct = leaderboard(&db).expect("direct");
        let replayed = rebuild(&db).expect("rebuild");
        for (a, b) in direct.iter().zip(replayed.iter()) {
            assert_eq!(a.competitor, b.competitor);
            assert!(
                (a.rating - b.rating).abs() < 1e-9,
                "{} direct {} vs rebuilt {}",
                a.competitor,
                a.rating,
                b.rating
            );
        }
    }

    #[test]
    fn pool_rating_is_per_ruleset_and_only_standard_is_rated() {
        let db = rbc_db::Db::open_memory().expect("db");
        let mut odd = game(1, "llm:a:1", "llm:b:1", 1.0);
        odd.kind = GameKind::ModelVsModel;
        // A non-standard ruleset has no pool at all.
        let mut staged = rbc_db::rating::RatedGame {
            period: 1,
            competitor: "x".into(),
            opponent: "y".into(),
            score: 1.0,
            ruleset: "chess960-v1".into(),
        };
        db.stage_rated_game(&staged)
            .expect("storage allows any row");
        let applied = apply_period(&db, 1).expect("apply");
        assert!(applied.is_empty(), "a non-standard row is not applied");

        staged.ruleset = RATED_RULESET.to_string();
        db.stage_rated_game(&staged).expect("standard row");
        assert_eq!(apply_period(&db, 1).expect("apply").len(), 1);
        let _ = odd;
        assert_eq!(Rating::default().rating, 1500.0);
    }
}
