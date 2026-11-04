//! Glicko-2 ratings.
//!
//! The algorithm is Glickman's (2013), implemented on the Glicko-2 scale and
//! converted back at the edges so callers only ever see a rating, a rating
//! deviation and a volatility. Two properties matter more than the arithmetic:
//!
//! - **A period, not a game.** Glicko-2 updates a player once per rating period
//!   from every game in it, so the update is computed from the whole period and
//!   then applied. Updating game by game would produce different numbers.
//! - **A known vector.** The paper's worked example is a test
//!   (`glicko_known_vector_from_the_paper`); if the iteration drifts, that test
//!   says so instead of everything quietly shifting.

/// The scale factor between the Glicko and Glicko-2 representations.
pub const SCALE: f64 = 173.7178;

/// A player's rating state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rating {
    /// Rating on the familiar scale, centred on 1500.
    pub rating: f64,
    /// Rating deviation: how uncertain the rating is.
    pub deviation: f64,
    /// Volatility: how erratic the results have been.
    pub volatility: f64,
}

impl Default for Rating {
    fn default() -> Self {
        Self {
            rating: 1500.0,
            deviation: 350.0,
            volatility: 0.06,
        }
    }
}

impl Rating {
    /// A rating with the given values.
    pub const fn new(rating: f64, deviation: f64, volatility: f64) -> Self {
        Self {
            rating,
            deviation,
            volatility,
        }
    }

    /// The Glicko-2 internal pair `(μ, φ)`.
    fn inner(self) -> (f64, f64) {
        ((self.rating - 1500.0) / SCALE, self.deviation / SCALE)
    }

    /// A rating deviation at or below this reads as established.
    pub fn is_established(self) -> bool {
        self.deviation <= 110.0
    }
}

/// How a rating period is configured.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlickoConfig {
    /// Constrains how much volatility may move in one period. 0.5 is the
    /// paper's default; smaller is steadier.
    pub tau: f64,
    /// Convergence tolerance for the volatility iteration.
    pub epsilon: f64,
}

impl Default for GlickoConfig {
    fn default() -> Self {
        Self {
            tau: 0.5,
            epsilon: 0.000001,
        }
    }
}

/// One game inside a period, from the rated player's side.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Game {
    /// The opponent's rating as it stood when the game was played.
    pub opponent: Rating,
    /// `1.0` win, `0.5` draw, `0.0` loss.
    pub score: f64,
}

impl Game {
    pub const fn win(opponent: Rating) -> Self {
        Self {
            opponent,
            score: 1.0,
        }
    }

    pub const fn draw(opponent: Rating) -> Self {
        Self {
            opponent,
            score: 0.5,
        }
    }

    pub const fn loss(opponent: Rating) -> Self {
        Self {
            opponent,
            score: 0.0,
        }
    }
}

/// The `g` factor: how much a rating deviation damps the comparison.
fn g(phi: f64) -> f64 {
    1.0 / (1.0 + 3.0 * phi * phi / (std::f64::consts::PI * std::f64::consts::PI)).sqrt()
}

/// The expected score against an opponent.
fn expected(mu: f64, mu_j: f64, phi_j: f64) -> f64 {
    1.0 / (1.0 + (-g(phi_j) * (mu - mu_j)).exp())
}

/// Apply one rating period.
///
/// `games` is every rated game the player played in the period. An empty period
/// is the "no games" case the paper describes: the deviation grows, and the
/// rating and volatility stay put.
pub fn update(player: Rating, games: &[Game], config: GlickoConfig) -> Rating {
    let (mu, phi) = player.inner();
    let sigma = player.volatility;

    if games.is_empty() {
        // φ* = sqrt(φ² + σ²): uncertainty grows while nothing is played.
        let phi_star = (phi * phi + sigma * sigma).sqrt();
        return Rating {
            rating: player.rating,
            deviation: phi_star * SCALE,
            volatility: sigma,
        };
    }

    let mut v_inverse = 0.0;
    let mut delta_sum = 0.0;
    for game in games {
        let (mu_j, phi_j) = game.opponent.inner();
        let e = expected(mu, mu_j, phi_j);
        let g_j = g(phi_j);
        v_inverse += g_j * g_j * e * (1.0 - e);
        delta_sum += g_j * (game.score - e);
    }
    let v = 1.0 / v_inverse;
    let delta = v * delta_sum;

    let sigma_prime = new_volatility(sigma, phi, v, delta, config);
    let phi_star = (phi * phi + sigma_prime * sigma_prime).sqrt();
    let phi_prime = 1.0 / (1.0 / (phi_star * phi_star) + 1.0 / v).sqrt();
    let mu_prime = mu + phi_prime * phi_prime * delta_sum;

    Rating {
        rating: SCALE * mu_prime + 1500.0,
        deviation: SCALE * phi_prime,
        volatility: sigma_prime,
    }
}

/// The volatility iteration, as the paper specifies it (Illinois algorithm).
fn new_volatility(sigma: f64, phi: f64, v: f64, delta: f64, config: GlickoConfig) -> f64 {
    let a = (sigma * sigma).ln();
    let tau = config.tau;
    let epsilon = config.epsilon;

    let f = |x: f64| -> f64 {
        let e_x = x.exp();
        let numerator = e_x * (delta * delta - phi * phi - v - e_x);
        let denominator = 2.0 * (phi * phi + v + e_x).powi(2);
        numerator / denominator - (x - a) / (tau * tau)
    };

    let mut upper = a;
    let mut lower;
    if delta * delta > phi * phi + v {
        lower = (delta * delta - phi * phi - v).ln();
    } else {
        let mut k = 1.0;
        while f(a - k * tau) < 0.0 && k < 100.0 {
            k += 1.0;
        }
        lower = a - k * tau;
    }

    let mut f_lower = f(lower);
    let mut f_upper = f(upper);
    let mut guard = 0;
    while (upper - lower).abs() > epsilon && guard < 200 {
        guard += 1;
        let c = lower + (lower - upper) * f_lower / (f_upper - f_lower);
        let f_c = f(c);
        if f_c * f_upper <= 0.0 {
            lower = upper;
            f_lower = f_upper;
        } else {
            f_lower /= 2.0;
        }
        upper = c;
        f_upper = f_c;
    }
    (upper.exp()).sqrt()
}

/// The conservative rank a leaderboard shows: two deviations below the rating,
/// so an uncertain competitor does not outrank a proven one on one good result.
pub fn conservative_rank(rating: Rating) -> f64 {
    rating.rating - 2.0 * rating.deviation
}

/// The competitor identity for a model, per the ratings contract.
///
/// A model or version bump is a **new competitor**, not a new rating for the old
/// one, so the version belongs in the id.
pub fn competitor_id(provider: &str, model: &str, version: &str) -> String {
    format!("llm:{provider}:{model}:{version}")
}

/// How a game was played, which is what decides whether it feeds a rating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameKind {
    /// Two models, both seats, no help and no failure.
    ModelVsModel,
    /// A person took one of the seats. Human versus AI is always unrated.
    Human,
    /// A casual game, whatever the seats.
    Casual,
    /// An engine or tool touched the game.
    ToolAssisted,
    /// The game was abandoned before it reached a result.
    Aborted,
    /// The provider or the infrastructure failed, so nobody's play decided it.
    ProviderFailed,
}

impl GameKind {
    /// Stable spelling for storage and logs.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ModelVsModel => "model-vs-model",
            Self::Human => "human",
            Self::Casual => "casual",
            Self::ToolAssisted => "tool-assisted",
            Self::Aborted => "aborted",
            Self::ProviderFailed => "provider-failed",
        }
    }
}

/// Whether a game feeds the rating pool.
///
/// Two conditions, and both are refusals rather than adjustments: only
/// model-versus-model games, and only under `standard-v1`. Everything else is
/// excluded, not rated at a discount.
pub fn is_rateable(kind: GameKind, ruleset: &str) -> bool {
    kind == GameKind::ModelVsModel && ruleset == "standard-v1"
}

/// Why a game is not rateable, for a log or a match record.
pub fn unrateable_reason(kind: GameKind, ruleset: &str) -> Option<String> {
    if is_rateable(kind, ruleset) {
        return None;
    }
    if ruleset != "standard-v1" {
        return Some(format!("ruleset {ruleset} is not rated"));
    }
    Some(format!("{} games are unrated", kind.as_str()))
}

/// A rating that does not move: an engine profile pinned at its nominal Elo.
///
/// Its opponents' ratings still change; its own never does, which is what makes
/// it a reference rather than a competitor. A rebuild must honour this too, so
/// the anchor is data, not a runtime flag.
#[derive(Debug, Clone, PartialEq)]
pub struct Anchor {
    pub competitor: String,
    pub rating: Rating,
}

impl Anchor {
    /// An engine profile at its nominal Elo with a narrow deviation, because a
    /// nominal strength is a claim rather than a measurement.
    pub fn engine(competitor: impl Into<String>, elo: f64) -> Self {
        Self {
            competitor: competitor.into(),
            rating: Rating::new(elo, 50.0, 0.06),
        }
    }
}
