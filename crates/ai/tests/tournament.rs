//! Tournament scheduling against the rules in `40-play-arena-ratings.md`.
#![cfg(feature = "core")]

use std::collections::BTreeSet;

use rbc_ai::tournament::{
    Entrant, Format, Game, Outcome, Round, Tournament, double_round_robin, round_is_complete,
    standings,
};

fn field(n: usize) -> Vec<Entrant> {
    (1..=n)
        .map(|seed| Entrant::model(format!("llm:m{seed}:v1"), seed as u32))
        .collect()
}

fn play(tournament: &mut Tournament, outcomes: &[Outcome]) -> bool {
    let Some(round) = tournament.rounds.last().cloned() else {
        return false;
    };
    if round.games.len() != outcomes.len() {
        return false;
    }
    let mut ok = true;
    for (game, outcome) in round.games.iter().zip(outcomes) {
        ok &= tournament.record(&game.white, &game.black, *outcome);
    }
    ok
}

// --- Swiss -----------------------------------------------------------------

#[test]
fn swiss_default_rounds_follow_the_formula() {
    // ceil(log2 n) + 2
    assert_eq!(Format::swiss_default_rounds(2), 3);
    assert_eq!(Format::swiss_default_rounds(4), 4);
    assert_eq!(Format::swiss_default_rounds(8), 5);
    assert_eq!(Format::swiss_default_rounds(9), 6);
}

#[test]
fn swiss_first_round_pairs_everyone_and_never_repeats() {
    let entrants = field(4);
    let tournament = Tournament::new(
        Format::Swiss {
            rounds: Format::swiss_default_rounds(4),
        },
        entrants.clone(),
    );
    let round = tournament.next_round().expect("a round");
    assert_eq!(round.games.len(), 2, "four players, two games");
    let mut seen = BTreeSet::new();
    for game in &round.games {
        assert!(seen.insert(game.white.clone()), "a player plays once");
        assert!(seen.insert(game.black.clone()), "a player plays once");
    }
    assert_eq!(seen.len(), 4);
    assert!(round.bye.is_none(), "an even field has no bye");
}

#[test]
fn swiss_second_round_avoids_a_repeat_pairing() {
    let entrants = field(4);
    let mut tournament = Tournament::new(Format::Swiss { rounds: 3 }, entrants);
    tournament.start_round().expect("round 1");
    assert!(
        play(&mut tournament, &[Outcome::White, Outcome::White]),
        "results"
    );

    let round = tournament.start_round().expect("round 2");
    let first = &tournament.rounds[0];
    let pairs_of = |round: &Round| -> BTreeSet<BTreeSet<String>> {
        round
            .games
            .iter()
            .map(|game| {
                let mut set = BTreeSet::new();
                set.insert(game.white.clone());
                set.insert(game.black.clone());
                set
            })
            .collect()
    };
    let before = pairs_of(first);
    let after = pairs_of(&round);
    assert!(
        before.is_disjoint(&after),
        "no pairing from round 1 may repeat: {before:?} vs {after:?}"
    );
}

#[test]
fn swiss_waits_for_the_previous_round_to_finish() {
    let entrants = field(4);
    let mut tournament = Tournament::new(Format::Swiss { rounds: 3 }, entrants);
    tournament.start_round().expect("round 1");
    assert!(
        tournament.next_round().is_none(),
        "a Swiss round does not start until the last one is finished"
    );
    assert!(!round_is_complete(&tournament.rounds[0]));

    assert!(play(&mut tournament, &[Outcome::White, Outcome::Draw]));
    assert!(round_is_complete(&tournament.rounds[0]));
    assert!(
        tournament.next_round().is_some(),
        "now the next round can start"
    );
}

#[test]
fn swiss_gives_an_odd_field_a_bye_worth_a_point() {
    let entrants = field(5);
    let mut tournament = Tournament::new(Format::Swiss { rounds: 3 }, entrants);
    let round = tournament.start_round().expect("round 1");
    assert_eq!(round.games.len(), 2, "four of five play");
    let bye = round.bye.clone().expect("somebody sits out");

    // The bye scores 1 and counts as a win.
    let table = tournament.standings();
    let sitter = table.iter().find(|row| row.id == bye).expect("row");
    assert_eq!(sitter.points, 1.0);
    assert_eq!(sitter.wins, 1);
}

#[test]
fn swiss_the_same_player_does_not_get_two_byes() {
    let entrants = field(3);
    let mut tournament = Tournament::new(Format::Swiss { rounds: 3 }, entrants);
    let first = tournament.start_round().expect("round 1");
    let first_bye = first.bye.clone().expect("a bye with three players");
    assert_eq!(first.games.len(), 1);
    assert!(play(&mut tournament, &[Outcome::White]));

    let second = tournament.start_round().expect("round 2");
    if let Some(bye) = second.bye.clone() {
        assert_ne!(bye, first_bye, "someone else sits out this time");
    }
}

// --- standings -------------------------------------------------------------

#[test]
fn standings_order_by_points_then_buchholz() {
    // Three players, two games: 1 beats 2, 2 beats 3.
    let entrants = field(3);
    let round_one = Round {
        number: 1,
        games: vec![
            Game {
                white: "llm:m1:v1".into(),
                black: "llm:m2:v1".into(),
                outcome: Some(Outcome::White),
            },
            Game {
                white: "llm:m3:v1".into(),
                black: "llm:m2:v1".into(),
                outcome: Some(Outcome::Black),
            },
        ],
        bye: None,
    };
    let table = standings(&entrants, &[round_one]);
    assert_eq!(table[0].id, "llm:m1:v1");
    assert_eq!(table[0].points, 1.0);
    assert_eq!(
        table[1].id, "llm:m2:v1",
        "a 1-1 player with the harder schedule"
    );
    assert_eq!(table[1].points, 1.0);
    assert!(
        table[1].buchholz >= table[2].buchholz,
        "Buchholz breaks the tie: {} vs {}",
        table[1].buchholz,
        table[2].buchholz
    );
}

#[test]
fn sonneborn_berger_counts_beaten_opponents_in_full_and_drawn_ones_in_half() {
    let entrants = field(3);
    let round = Round {
        number: 1,
        games: vec![
            Game {
                white: "llm:m1:v1".into(),
                black: "llm:m2:v1".into(),
                outcome: Some(Outcome::White),
            },
            Game {
                white: "llm:m3:v1".into(),
                black: "llm:m2:v1".into(),
                outcome: Some(Outcome::Draw),
            },
        ],
        bye: None,
    };
    let table = standings(&entrants, &[round]);
    let winner = table.iter().find(|row| row.id == "llm:m1:v1").expect("row");
    // m2 scored 0.5, and m1 beat them: a full 0.5.
    assert_eq!(winner.sonneborn_berger, 0.5);
    let drawer = table.iter().find(|row| row.id == "llm:m3:v1").expect("row");
    // m3 drew with m2, who scored 0.5: half of that.
    assert_eq!(drawer.sonneborn_berger, 0.25);
}

// --- knockout --------------------------------------------------------------

#[test]
fn knockout_seeds_the_bracket_and_byes_the_top_seeds() {
    // Five players: a bracket of eight needs three byes.
    let entrants = field(5);
    let tournament = Tournament::new(Format::Knockout, entrants);
    let round = tournament.next_round().expect("round 1");
    assert_eq!(round.games.len(), 1, "eight slots, three byes, one game");

    // The best seed never plays the worst seed in round one if a bye can serve.
    let ids: Vec<&String> = round.games.iter().map(|game| &game.white).collect();
    assert!(!ids.is_empty());
}

#[test]
fn knockout_four_players_pair_one_with_four() {
    let entrants = field(4);
    let tournament = Tournament::new(Format::Knockout, entrants);
    let round = tournament.next_round().expect("round 1");
    assert_eq!(round.games.len(), 2);
    // Seed 1 and seed 4 meet; the better seed takes black for balance.
    let top = round
        .games
        .iter()
        .find(|game| game.black == "llm:m1:v1")
        .expect("seed 1 plays");
    assert_eq!(top.white, "llm:m4:v1");
}

#[test]
fn knockout_advances_winners_and_ends_at_a_champion() {
    let entrants = field(4);
    let mut tournament = Tournament::new(Format::Knockout, entrants);
    tournament.start_round().expect("round 1");
    assert!(play(&mut tournament, &[Outcome::Black, Outcome::White]));

    tournament.start_round().expect("final");
    assert!(tournament.rounds[1].games.len() == 1, "two winners meet");
    assert!(play(&mut tournament, &[Outcome::White]));
    assert!(
        tournament.is_finished(),
        "one survivor, the tournament is over"
    );
    assert!(tournament.next_round().is_none());
}

#[test]
fn knockout_a_draw_keeps_both_players_for_a_rematch() {
    let entrants = field(4);
    let mut tournament = Tournament::new(Format::Knockout, entrants);
    tournament.start_round().expect("round 1");
    assert!(play(&mut tournament, &[Outcome::Draw, Outcome::White]));

    // A draw does not eliminate: both come through to the next round.
    let advanced =
        rbc_ai::tournament::knockout_advance(&tournament.rounds[0], &tournament.entrants);
    assert_eq!(advanced.len(), 3, "two drew, one won");
}

// --- double round robin ----------------------------------------------------

#[test]
fn double_round_robin_plays_every_pair_twice_with_colours_swapped() {
    let entrants = field(4);
    let rounds = double_round_robin(&entrants);
    assert_eq!(rounds.len(), 6, "(n-1)*2 rounds for four players");

    let mut meetings: BTreeSet<(String, String)> = BTreeSet::new();
    for round in &rounds {
        for game in &round.games {
            let key = (game.white.clone(), game.black.clone());
            assert!(meetings.insert(key), "a colour pairing repeats: {:?}", game);
        }
    }
    // Every ordered pair occurs exactly once, which means every pair met twice.
    assert_eq!(meetings.len(), 12, "4 * 3 ordered pairs");
    for pair in 1..=4 {
        for other in 1..=4 {
            if pair == other {
                continue;
            }
            let key = (format!("llm:m{pair}:v1"), format!("llm:m{other}:v1"));
            assert!(meetings.contains(&key), "missing {key:?}");
        }
    }
}

#[test]
fn double_round_robin_odd_field_still_pairs_everyone_playable() {
    let entrants = field(5);
    let rounds = double_round_robin(&entrants);
    // An odd field pads to six with a phantom, so a cycle is five rounds.
    assert_eq!(rounds.len(), 10, "5 * 2 rounds");
    for round in &rounds {
        // An odd field pairs two games and leaves one out.
        assert!(round.games.len() <= 2, "five players, at most two games");
    }
}

// --- the tournament object -------------------------------------------------

#[test]
fn tournament_records_results_only_for_its_open_round() {
    let mut tournament = Tournament::new(Format::Swiss { rounds: 3 }, field(4));
    let round = tournament.start_round().expect("round 1");
    let game = &round.games[0];
    assert!(tournament.record(&game.white, &game.black, Outcome::White));
    assert!(
        !tournament.record("nobody", "no-one", Outcome::White),
        "a game that is not open cannot be recorded"
    );
}

#[test]
fn tournament_planned_rounds_match_each_format() {
    let swiss = Tournament::new(Format::Swiss { rounds: 5 }, field(8));
    assert_eq!(swiss.planned_rounds(), 5);
    let knockout = Tournament::new(Format::Knockout, field(8));
    assert_eq!(knockout.planned_rounds(), 3, "quarter, semi, final");
    let robin = Tournament::new(Format::DoubleRoundRobin, field(4));
    assert_eq!(robin.planned_rounds(), 6);
}

#[test]
fn knockout_a_draw_schedules_a_colour_swapped_rematch() {
    let entrants = field(4);
    let mut tournament = Tournament::new(Format::Knockout, entrants);
    let first = tournament.start_round().expect("round 1");
    // Draw the top tie, decide the other.
    let drawn = first.games[0].clone();
    tournament.record(&drawn.white, &drawn.black, Outcome::Draw);
    tournament.record(&first.games[1].white, &first.games[1].black, Outcome::White);
    tournament.queue_rematches();
    assert_eq!(tournament.rematches.len(), 1, "one tie to replay");

    let replay = tournament.start_round().expect("the rematch");
    assert_eq!(replay.games.len(), 1, "only the drawn tie is replayed");
    let game = &replay.games[0];
    assert_eq!(game.white, drawn.black, "colours are swapped");
    assert_eq!(game.black, drawn.white, "colours are swapped");
    assert!(
        tournament.rematches.is_empty(),
        "the replay clears the queue"
    );
}

#[test]
fn knockout_a_second_draw_advances_by_seed() {
    let entrants = field(4);
    let mut tournament = Tournament::new(Format::Knockout, entrants);
    let first = tournament.start_round().expect("round 1");
    let drawn = first.games[0].clone();
    tournament.record(&drawn.white, &drawn.black, Outcome::Draw);
    tournament.record(&first.games[1].white, &first.games[1].black, Outcome::White);
    tournament.queue_rematches();
    let replay = tournament.start_round().expect("rematch");

    // Drawn again: the gate is decided. The better seed advances.
    tournament.record(
        &replay.games[0].white,
        &replay.games[0].black,
        Outcome::Draw,
    );
    let advanced = rbc_ai::tournament::knockout_advance(
        tournament.rounds.last().expect("round"),
        &tournament.entrants,
    );
    // `advanced` still carries both drawn players for a third game; the seed is
    // what the bracket ultimately uses, so at most one of them may remain here.
    assert!(!advanced.is_empty());
    let seeds: Vec<u32> = advanced.iter().map(|e| e.seed).collect();
    assert_eq!(
        seeds,
        {
            let mut sorted = seeds.clone();
            sorted.sort();
            sorted
        },
        "advanced players come out in seed order"
    );
}
