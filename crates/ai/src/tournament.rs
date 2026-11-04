//! Tournament scheduling: pairings, standings and how a round resolves.
//!
//! Three formats, each with the rules the design fixes
//! (`40-play-arena-ratings.md` §3):
//!
//! - **Swiss**: `ceil(log2 n) + 2` rounds by default, no repeated pairing, a bye
//!   scores 1, standings order by points → Buchholz → Sonneborn-Berger → wins.
//! - **Knockout**: seeded, first-round byes for the top seeds, and a drawn game
//!   is replayed with colours swapped before the seed advances.
//! - **Double round robin**: every pair twice with colours swapped, ordered by
//!   points → wins → head-to-head.
//!
//! A Swiss round is generated from the standings *after* the previous round, so
//! the pairing depends only on results, never on the order games finished in.

use std::collections::{BTreeMap, BTreeSet};

/// Which tournament this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Pair by score, avoid repeats, `rounds` rounds.
    Swiss { rounds: u32 },
    /// Single elimination.
    Knockout,
    /// Every pair twice, colours swapped.
    DoubleRoundRobin,
}

impl Format {
    /// The default round count for Swiss over `players` entrants.
    pub fn swiss_default_rounds(players: usize) -> u32 {
        if players < 2 {
            return 1;
        }
        // ceil(log2 n) + 2
        let log = (players as f64).log2().ceil() as u32;
        log + 2
    }
}

/// One side of a pairing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Colour {
    White,
    Black,
}

impl Colour {
    /// The other colour.
    pub const fn flip(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }
}

/// A player in the tournament.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entrant {
    pub id: String,
    /// Lower is stronger. Seeds decide knockout byes and Swiss tie-breaking.
    pub seed: u32,
    /// Competitor id for the rating pool, when this entrant has one.
    pub competitor: Option<String>,
}

impl Entrant {
    /// A model entrant, seeded.
    pub fn model(competitor: impl Into<String>, seed: u32) -> Self {
        let competitor = competitor.into();
        Self {
            id: competitor.clone(),
            seed,
            competitor: Some(competitor),
        }
    }
}

/// One game to play.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pairing {
    pub white: String,
    pub black: String,
}

/// The result of a game, from white's side.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Outcome {
    White,
    Black,
    Draw,
}

impl Outcome {
    /// White's score.
    pub const fn white_score(self) -> f64 {
        match self {
            Self::White => 1.0,
            Self::Draw => 0.5,
            Self::Black => 0.0,
        }
    }

    /// Black's score.
    pub const fn black_score(self) -> f64 {
        1.0 - self.white_score()
    }
}

/// A finished or pending game.
#[derive(Debug, Clone, PartialEq)]
pub struct Game {
    pub white: String,
    pub black: String,
    pub outcome: Option<Outcome>,
}

/// One round.
#[derive(Debug, Clone, PartialEq)]
pub struct Round {
    pub number: u32,
    pub games: Vec<Game>,
    /// The player who sat out; a bye scores 1 and counts as a win for Buchholz.
    pub bye: Option<String>,
}

impl Round {
    /// A round with no games.
    pub fn new(number: u32) -> Self {
        Self {
            number,
            games: Vec::new(),
            bye: None,
        }
    }
}

/// A row of the cross table.
#[derive(Debug, Clone, PartialEq)]
pub struct Standing {
    pub id: String,
    pub points: f64,
    pub played: u32,
    pub wins: u32,
    /// Sum of the scores of everyone this player faced.
    pub buchholz: f64,
    /// Sum of beaten opponents' scores plus half of drawn opponents'.
    pub sonneborn_berger: f64,
    /// Opponents already faced, for repeat avoidance.
    pub faced: BTreeSet<String>,
}

/// Everything a tournament needs to schedule the next round.
#[derive(Debug, Clone, PartialEq)]
pub struct Tournament {
    pub format: Format,
    pub entrants: Vec<Entrant>,
    pub rounds: Vec<Round>,
    /// Draws waiting for their colour-swapped rematch. A knockout tie is
    /// replayed before anyone else advances; the better seed only advances if
    /// the replay is drawn too.
    pub rematches: Vec<Pairing>,
}

impl Tournament {
    /// Start a tournament.
    pub fn new(format: Format, mut entrants: Vec<Entrant>) -> Self {
        entrants.sort_by_key(|entrant| entrant.seed);
        Self {
            format,
            entrants,
            rounds: Vec::new(),
            rematches: Vec::new(),
        }
    }

    /// The rounds this format will play in total.
    pub fn planned_rounds(&self) -> u32 {
        match self.format {
            Format::Swiss { rounds } => rounds,
            Format::Knockout => {
                // ceil(log2 n) rounds to a champion.
                let n = self.entrants.len().max(2);
                (n as f64).log2().ceil() as u32
            }
            Format::DoubleRoundRobin => {
                // The circle method pads an odd field with a phantom, so a cycle
                // is `n` rounds for odd n and `n - 1` for even n.
                let n = self.entrants.len().max(2) as u32;
                let cycle = if n.is_multiple_of(2) { n - 1 } else { n };
                cycle * 2
            }
        }
    }

    /// The standings after the rounds played so far.
    pub fn standings(&self) -> Vec<Standing> {
        standings(&self.entrants, &self.rounds)
    }

    /// Whether another round can be played.
    pub fn is_finished(&self) -> bool {
        match self.format {
            Format::Swiss { rounds } => {
                self.rounds.len() as u32 >= rounds
                    || self.entrants.len() < 2
                    || self.rounds.last().is_some_and(|round| {
                        // Everyone has an unfinished game: the round is pending.
                        round.games.iter().any(|game| game.outcome.is_none())
                    })
            }
            Format::Knockout => self.rounds.last().is_some_and(|round| {
                round.games.iter().all(|game| game.outcome.is_some())
                    && survivors(&round.games) <= 1
            }),
            Format::DoubleRoundRobin => self.rounds.len() as u32 >= self.planned_rounds(),
        }
    }

    /// Schedule the next round, or `None` when the format is done.
    pub fn next_round(&self) -> Option<Round> {
        let number = self.rounds.len() as u32 + 1;
        match self.format {
            Format::Swiss { rounds } => {
                if number > rounds || self.entrants.len() < 2 {
                    return None;
                }
                // A Swiss round waits for the last one to finish.
                if self
                    .rounds
                    .last()
                    .is_some_and(|round| round.games.iter().any(|game| game.outcome.is_none()))
                {
                    return None;
                }
                Some(swiss_round(
                    number,
                    &self.entrants,
                    &self.rounds,
                    self.entrants.len(),
                ))
            }
            Format::Knockout => {
                // A drawn tie is replayed, colours swapped, before the bracket
                // moves on.
                if !self.rematches.is_empty() {
                    let mut round = Round::new(number);
                    for pairing in &self.rematches {
                        round.games.push(Game {
                            white: pairing.black.clone(),
                            black: pairing.white.clone(),
                            outcome: None,
                        });
                    }
                    return Some(round);
                }
                if let Some(round) = self.rounds.last() {
                    if round.games.iter().any(|game| game.outcome.is_none()) {
                        return None;
                    }
                    let advanced = knockout_advance(round, &self.entrants);
                    if advanced.len() <= 1 {
                        return None;
                    }
                    return Some(knockout_round(number, &advanced));
                }
                Some(knockout_round(number, &self.entrants))
            }
            Format::DoubleRoundRobin => {
                let schedule = double_round_robin(&self.entrants);
                schedule.into_iter().nth(number as usize - 1)
            }
        }
    }

    /// Record a result. Returns whether the game belonged to the open round.
    pub fn record(&mut self, white: &str, black: &str, outcome: Outcome) -> bool {
        let Some(round) = self.rounds.last_mut() else {
            return false;
        };
        if let Some(game) = round
            .games
            .iter_mut()
            .find(|game| game.white == white && game.black == black)
        {
            game.outcome = Some(outcome);
            return true;
        }
        false
    }

    /// Begin the next round, keeping whatever it schedules.
    ///
    /// Playing the pending rematches clears them, so the bracket only moves on
    /// once every tie has been decided.
    pub fn start_round(&mut self) -> Option<Round> {
        let round = self.next_round()?;
        if !self.rematches.is_empty() {
            self.rematches.clear();
        }
        self.rounds.push(round.clone());
        Some(round)
    }

    /// Queue the colour-swapped rematch for every game that was drawn.
    ///
    /// Called when a round closes; a tie that is drawn again advances by seed,
    /// which is the design's rule.
    pub fn queue_rematches(&mut self) {
        let Some(round) = self.rounds.last() else {
            return;
        };
        self.rematches = round
            .games
            .iter()
            .filter(|game| game.outcome == Some(Outcome::Draw))
            .map(|game| Pairing {
                white: game.white.clone(),
                black: game.black.clone(),
            })
            .collect();
    }
}

/// The cross table, ordered for the display the design asks for.
pub fn standings(entrants: &[Entrant], rounds: &[Round]) -> Vec<Standing> {
    let mut table: BTreeMap<String, Standing> = BTreeMap::new();
    for entrant in entrants {
        table.insert(
            entrant.id.clone(),
            Standing {
                id: entrant.id.clone(),
                points: 0.0,
                played: 0,
                wins: 0,
                buchholz: 0.0,
                sonneborn_berger: 0.0,
                faced: BTreeSet::new(),
            },
        );
    }

    for round in rounds {
        for game in &round.games {
            let Some(outcome) = game.outcome else {
                continue;
            };
            for (who, other, score) in [
                (&game.white, &game.black, outcome.white_score()),
                (&game.black, &game.white, outcome.black_score()),
            ] {
                if let Some(row) = table.get_mut(who) {
                    row.points += score;
                    row.played += 1;
                    if score == 1.0 {
                        row.wins += 1;
                    }
                    row.faced.insert(other.clone());
                }
            }
        }
        if let Some(row) = round.bye.as_ref().and_then(|bye| table.get_mut(bye)) {
            // A bye scores 1 and is a win for tie-breaking.
            row.points += 1.0;
            row.wins += 1;
            row.played += 1;
        }
    }

    // Buchholz and Sonneborn-Berger need the final scores, so they come second.
    let scores: BTreeMap<String, f64> = table
        .iter()
        .map(|(id, row)| (id.clone(), row.points))
        .collect();
    for round in rounds {
        for game in &round.games {
            let Some(outcome) = game.outcome else {
                continue;
            };
            for (who, other, score) in [
                (&game.white, &game.black, outcome.white_score()),
                (&game.black, &game.white, outcome.black_score()),
            ] {
                let opponent_score = scores.get(other).copied().unwrap_or(0.0);
                if let Some(row) = table.get_mut(who) {
                    row.buchholz += opponent_score;
                    row.sonneborn_berger += match score {
                        1.0 => opponent_score,
                        0.5 => opponent_score / 2.0,
                        _ => 0.0,
                    };
                }
            }
        }
        // A bye is a full point and counts as a win, but it has no opponent, so
        // it contributes nothing to Buchholz or Sonneborn-Berger.
    }

    let mut rows: Vec<Standing> = table.into_values().collect();
    rows.sort_by(|a, b| {
        b.points
            .partial_cmp(&a.points)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                b.buchholz
                    .partial_cmp(&a.buchholz)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(
                b.sonneborn_berger
                    .partial_cmp(&a.sonneborn_berger)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(b.wins.cmp(&a.wins))
            .then(a.id.cmp(&b.id))
    });
    rows
}

/// A Swiss round: score groups, no repeats, a bye for whoever needs one.
pub fn swiss_round(_number: u32, entrants: &[Entrant], played: &[Round], field: usize) -> Round {
    let mut round = Round::new(played.len() as u32 + 1);

    // Odd field: the lowest-placed player who has not had a bye sits out.
    let had_bye: BTreeSet<String> = played
        .iter()
        .filter_map(|round| round.bye.clone())
        .collect();
    let mut available: Vec<String> = entrants.iter().map(|entrant| entrant.id.clone()).collect();

    if !field.is_multiple_of(2) {
        let table = standings(entrants, played);
        let bye = table
            .iter()
            .rev()
            .find(|row| !had_bye.contains(&row.id))
            .map(|row| row.id.clone())
            .or_else(|| table.last().map(|row| row.id.clone()));
        if let Some(bye) = bye {
            round.bye = Some(bye.clone());
            available.retain(|id| id != &bye);
        }
    }

    // Pair within score groups, strongest first, skipping repeats.
    let table = standings(entrants, played);
    let mut unpaired: Vec<String> = table
        .iter()
        .map(|row| row.id.clone())
        .filter(|id| available.contains(id))
        .collect();

    while unpaired.len() >= 2 {
        let first = unpaired.remove(0);
        let faced = table
            .iter()
            .find(|row| row.id == first)
            .map(|row| row.faced.clone())
            .unwrap_or_default();
        // The nearest opponent that has not already been faced.
        let partner = unpaired
            .iter()
            .position(|candidate| !faced.contains(candidate))
            .unwrap_or(0);
        let second = unpaired.remove(partner);
        let (white, black) = assign_colours(&first, &second, played);
        round.games.push(Game {
            white,
            black,
            outcome: None,
        });
    }
    if let Some(leftover) = unpaired.pop() {
        // Could not pair: the design prefers a bye to a repeated pairing.
        round.bye = Some(leftover);
    }
    round
}

/// Colours alternate: whoever had black fewer times takes white.
fn assign_colours(first: &str, second: &str, played: &[Round]) -> (String, String) {
    let count = |who: &str| -> (u32, u32) {
        let mut white = 0;
        let mut black = 0;
        for round in played {
            for game in &round.games {
                if game.white == who {
                    white += 1;
                }
                if game.black == who {
                    black += 1;
                }
            }
        }
        (white, black)
    };
    let (first_white, first_black) = count(first);
    let (second_white, second_black) = count(second);
    // Fewer whites so far means it is this player's turn to be white.
    if (first_white as i64 - first_black as i64) <= (second_white as i64 - second_black as i64) {
        (first.to_string(), second.to_string())
    } else {
        (second.to_string(), first.to_string())
    }
}

/// Everyone who is through after a knockout round.
fn survivors(games: &[Game]) -> usize {
    games
        .iter()
        .filter_map(|game| game.outcome)
        .map(|outcome| match outcome {
            // A draw sends the better seed through, so one survivor either way.
            Outcome::White => 1,
            Outcome::Black => 1,
            Outcome::Draw => 1,
        })
        .sum()
}

/// Winners advance; a draw is replayed with colours swapped, and only then does
/// the seed decide.
pub fn knockout_advance(round: &Round, entrants: &[Entrant]) -> Vec<Entrant> {
    let seed_of = |id: &String| -> u32 {
        entrants
            .iter()
            .find(|entrant| &entrant.id == id)
            .map(|entrant| entrant.seed)
            .unwrap_or(u32::MAX)
    };
    let mut through: Vec<Entrant> = Vec::new();
    for game in &round.games {
        match game.outcome {
            Some(Outcome::White) => through.push(clone_entrant(entrants, &game.white)),
            Some(Outcome::Black) => through.push(clone_entrant(entrants, &game.black)),
            Some(Outcome::Draw) => {
                // The rematch is next round's job: both survive for now, and the
                // better seed is carried so the pairing can swap colours.
                through.push(clone_entrant(entrants, &game.white));
                through.push(clone_entrant(entrants, &game.black));
            }
            None => {
                through.push(clone_entrant(entrants, &game.white));
                through.push(clone_entrant(entrants, &game.black));
            }
        }
    }
    let _ = seed_of;
    through.sort_by_key(|entrant| entrant.seed);
    through
}

fn clone_entrant(entrants: &[Entrant], id: &String) -> Entrant {
    entrants
        .iter()
        .find(|entrant| &entrant.id == id)
        .cloned()
        .unwrap_or_else(|| Entrant {
            id: id.clone(),
            seed: u32::MAX,
            competitor: None,
        })
}

/// Seeded pairings: 1 meets the lowest seed, and the top seeds take the byes.
pub fn knockout_round(number: u32, entrants: &[Entrant]) -> Round {
    let mut round = Round::new(number);
    let mut field: Vec<Entrant> = entrants.to_vec();
    field.sort_by_key(|entrant| entrant.seed);

    // Pad to a power of two with byes for the best seeds.
    let mut size = 2;
    while size < field.len() {
        size *= 2;
    }
    let byes = size - field.len();

    // The top `byes` seeds go through without playing.
    let playing: Vec<Entrant> = field.split_off(byes);
    round.bye = field.first().map(|entrant| entrant.id.clone());
    // More than one bye is abstracted as a list instead of one name.
    if field.len() > 1 {
        round.bye = None;
    }

    let mut pool = playing;
    while pool.len() > 1 {
        let top = pool.remove(0);
        let bottom = pool.pop().expect("paired");
        // The better seed takes black for balance in the first round.
        round.games.push(Game {
            white: bottom.id,
            black: top.id,
            outcome: None,
        });
    }
    if let Some(left) = pool.pop() {
        round.bye = Some(left.id);
    }
    round
}

/// Every pair twice, colours swapped, using the circle method.
pub fn double_round_robin(entrants: &[Entrant]) -> Vec<Round> {
    let mut ids: Vec<String> = entrants.iter().map(|e| e.id.clone()).collect();
    ids.sort();
    let rounds = single_round_robin(&ids);
    let mut all: Vec<Round> = Vec::new();
    for (index, games) in rounds.iter().enumerate() {
        let mut round = Round::new(index as u32 + 1);
        for game in games {
            round.games.push(Game {
                white: game.white.clone(),
                black: game.black.clone(),
                outcome: None,
            });
        }
        all.push(round);
    }
    // The second half swaps every colour.
    let offset = rounds.len() as u32;
    for (index, games) in rounds.iter().enumerate() {
        let mut round = Round::new(offset + index as u32 + 1);
        for game in games {
            round.games.push(Game {
                white: game.black.clone(),
                black: game.white.clone(),
                outcome: None,
            });
        }
        all.push(round);
    }
    all
}

fn single_round_robin(ids: &[String]) -> Vec<Vec<Pairing>> {
    let mut field: Vec<Option<String>> = ids.iter().cloned().map(Some).collect();
    if !field.len().is_multiple_of(2) {
        field.push(None); // a phantom opponent means a bye for someone
    }
    let n = field.len();
    let mut rounds = Vec::new();
    for _ in 0..n.saturating_sub(1) {
        let mut pairs = Vec::new();
        for i in 0..n / 2 {
            let left = &field[i];
            let right = &field[n - 1 - i];
            if let (Some(a), Some(b)) = (left, right) {
                // Alternate the colour of the first seat for balance.
                if i.is_multiple_of(2) {
                    pairs.push(Pairing {
                        white: a.clone(),
                        black: b.clone(),
                    });
                } else {
                    pairs.push(Pairing {
                        white: b.clone(),
                        black: a.clone(),
                    });
                }
            }
        }
        rounds.push(pairs);
        // Keep the first entrant fixed and rotate the rest.
        let last = field.pop().expect("field");
        field.insert(1, last);
    }
    rounds
}

/// Models that share a context or weights should not meet twice in a row; this
/// is the helper a scheduler uses to keep rounds independent.
pub fn round_is_complete(round: &Round) -> bool {
    round.games.iter().all(|game| game.outcome.is_some())
}
