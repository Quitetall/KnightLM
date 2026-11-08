//! Playing a match: who may ask for what, and how a model's move is obtained.
//!
//! A match has two seats and two kinds of game. The difference is a policy, not
//! a scattering of `if`s:
//!
//! - **Casual** is a conversation with a board. The model answers questions,
//!   offers hints, may use an engine, and a takeback is a normal thing to do.
//! - **Competitive** is a game. Only moves are exchanged, reasoning is not
//!   shown, exported or even asked for, and neither side may take a move back.
//!
//! The driver asks the model for one move, validates it against the real
//! position, and feeds a refusal back rather than accepting whatever came out:
//! a model that answers with an illegal move gets [`MatchConfig::max_retries`]
//! chances, and the match never records a move the rules refuse.

use knightlm_core::state::{GameRecord, GameState, PacketOptions, START_FEN};

use crate::game::{Match, MatchConfig, MatchMove, Player};
use crate::protocol::{ErrorInfo, ModelRef};
use crate::provider::{
    Cancellation, ChatMessage, CompletionRequest, Provider, ProviderEvent, error, error_code,
};

/// Which kind of game this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    /// Conversation plus a board: chat, hints, engine, takebacks.
    Casual,
    /// Move-only: no chat, no hints, no engine, no takebacks, hidden reasoning.
    Competitive,
}

impl MatchKind {
    /// Stable spelling for storage and logs.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Casual => "casual",
            Self::Competitive => "competitive",
        }
    }
}

/// What a seat may do, and what the transcript keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayPolicy {
    pub kind: MatchKind,
    /// Ask the model for its reasoning at all.
    pub request_reasoning: bool,
    /// Show reasoning in the transcript.
    pub show_reasoning: bool,
    /// Include reasoning in an exported PGN or report.
    pub export_reasoning: bool,
    /// Let a person talk to the model during the game.
    pub allow_chat: bool,
    /// Let the model be asked for a hint.
    pub allow_hints: bool,
    /// Let the model or the person consult an engine.
    pub allow_engine: bool,
    /// Let either side take a move back.
    pub allow_takeback: bool,
}

impl PlayPolicy {
    /// A competitive game: moves and nothing else.
    pub const fn competitive() -> Self {
        Self {
            kind: MatchKind::Competitive,
            request_reasoning: false,
            show_reasoning: false,
            export_reasoning: false,
            allow_chat: false,
            allow_hints: false,
            allow_engine: false,
            allow_takeback: false,
        }
    }

    /// A casual game, where talking about the position is the point.
    pub const fn casual() -> Self {
        Self {
            kind: MatchKind::Casual,
            request_reasoning: true,
            show_reasoning: true,
            export_reasoning: true,
            allow_chat: true,
            allow_hints: true,
            allow_engine: true,
            allow_takeback: true,
        }
    }

    /// Reasoning the caller is allowed to keep, given this policy.
    pub fn keep_reasoning(&self, reasoning: Option<String>) -> Option<String> {
        if self.show_reasoning || self.export_reasoning {
            reasoning
        } else {
            None
        }
    }

    /// Whether an action is allowed during the game.
    pub fn allows(&self, action: PlayAction) -> bool {
        match action {
            PlayAction::Chat => self.allow_chat,
            PlayAction::Hint => self.allow_hints,
            PlayAction::Engine => self.allow_engine,
            PlayAction::Takeback => self.allow_takeback,
        }
    }

    /// The refusal for a disallowed action, or `None` when it is allowed.
    pub fn check(&self, action: PlayAction) -> Option<ErrorInfo> {
        if self.allows(action) {
            return None;
        }
        Some(error(
            error_code::UNSUPPORTED,
            format!(
                "{} is not available in a {} game",
                action.as_str(),
                self.kind.as_str()
            ),
            false,
        ))
    }
}

/// Something a person may want to do mid-game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayAction {
    Chat,
    Hint,
    Engine,
    Takeback,
}

impl PlayAction {
    /// Stable spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Hint => "hint",
            Self::Engine => "engine",
            Self::Takeback => "takeback",
        }
    }
}

/// One move obtained from a model, with what the policy allows keeping.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelMove {
    pub mv: MatchMove,
    /// Present only when the policy shows or exports reasoning.
    pub reasoning: Option<String>,
    /// What the model actually said, for the transcript.
    pub raw: String,
    /// How many attempts the model needed.
    pub attempts: u32,
}

/// The prompt a model is given for a move.
///
/// It names the position, the legal replies and the exact format expected, so a
/// refusal is about the move and not about the framing.
pub fn move_prompt(game: &Match, policy: &PlayPolicy) -> String {
    let mut prompt = String::new();
    prompt.push_str("You are playing a chess game.\n");
    prompt.push_str(&format!("FEN: {}\n", game.fen()));
    prompt.push_str(&format!(
        "Your side: {}\n",
        if game.turn() == shakmaty::Color::White {
            "white"
        } else {
            "black"
        }
    ));
    let legal = game.legal_moves();
    prompt.push_str(&format!("Legal moves: {}\n", legal.join(" ")));
    if policy.request_reasoning {
        prompt.push_str("Think briefly, then reply with the move.\n");
    }
    prompt.push_str(
        "Reply with exactly one move in SAN or UCI and nothing else — \
         for example `Nf3` or `g1f3`.\n",
    );
    prompt
}

/// Match the pilot's single user message and canonical packet serialization.
fn knightlm_move_request(
    game: &Match,
    model: &ModelRef,
) -> anyhow::Result<(CompletionRequest, Vec<String>)> {
    let config = game.config();
    anyhow::ensure!(
        config.ruleset == "standard-v1" && config.time_control.is_none(),
        "the KnightLM pilot plays untimed standard chess"
    );
    let state = GameState::replay(&GameRecord {
        schema_version: knightlm_core::SCHEMA_VERSION,
        game_id: "rbc-match".into(),
        source: "rbc-play".into(),
        initial_fen: config.start_fen.clone(),
        history_complete: config.start_fen == START_FEN,
        moves: game.moves().iter().map(|mv| mv.uci.clone()).collect(),
        variant: "standard".into(),
    })?;
    let packet = state.compile(PacketOptions {
        legal_choices: true,
        derived_features: false,
    });
    let legal = game.legal_uci_moves();
    anyhow::ensure!(
        game.result().is_none() && !legal.is_empty(),
        "KnightLM has no legal move in this position"
    );
    // The entries are canonical UCI from the rules engine, including promotion
    // suffixes. Permit one leading space to match common move tokens.
    let alternatives = legal
        .iter()
        .map(|mv| format!("{mv:?}"))
        .collect::<Vec<_>>()
        .join(" | ");
    let mut request = CompletionRequest::new(
        model.model.clone(),
        vec![ChatMessage::user(format!(
            "Choose a legal chess move. Reply with just the UCI move.\n{}",
            serde_json::to_value(packet)?
        ))],
    )
    .with_temperature(0.0)
    .with_max_tokens(20)
    .with_stream(false);
    request.grammar = Some(format!("root ::= \" \"? ({alternatives})"));
    Ok((request, legal))
}

fn complete_knightlm_move(
    provider: &dyn Provider,
    request: &CompletionRequest,
    legal: &[String],
    cancel: &Cancellation,
) -> Result<String, ErrorInfo> {
    let response = provider.complete(request, cancel, &mut |_| {})?;
    let raw = response.content.trim();
    if !legal.iter().any(|mv| mv == raw) {
        return Err(error(
            error_code::PROTOCOL,
            "KnightLM's constrained decoder returned a response outside the legal move list",
            false,
        ));
    }
    Ok(raw.to_owned())
}

fn apply_knightlm_move(game: &mut Match, uci: String) -> Result<ModelMove, ErrorInfo> {
    let mut mv = game
        .apply(&uci)
        .map_err(|failure| error(error_code::PROTOCOL, failure.to_string(), false))?;
    game.note_attempts(mv.ply, 1);
    mv.attempts = 1;
    Ok(ModelMove {
        mv,
        reasoning: None,
        raw: uci,
        attempts: 1,
    })
}

/// Ask a model for one move and record it on the match.
///
/// KnightLM generates against the legal UCI grammar. For other models an
/// unparseable or illegal reply is retried up to [`MatchConfig::max_retries`].
/// The match is untouched on failure.
pub fn request_move(
    game: &mut Match,
    provider: &dyn Provider,
    model: &ModelRef,
    policy: &PlayPolicy,
    cancel: &Cancellation,
) -> Result<ModelMove, ErrorInfo> {
    if model.provider == "local:LAMU" && model.model == "knightlm-smolvlm-500m" {
        let (request, legal) = knightlm_move_request(game, model)
            .map_err(|failure| error(error_code::PROTOCOL, failure.to_string(), false))?;
        let uci = complete_knightlm_move(provider, &request, &legal, cancel)?;
        return apply_knightlm_move(game, uci);
    }
    let budget = game.config().max_retries.max(1) as u32;
    let mut messages = vec![
        ChatMessage::system(move_prompt(game, policy)),
        ChatMessage::user("Your move."),
    ];
    let mut last_error = None;
    for attempt in 1..=budget {
        let request = CompletionRequest::new(model.model.clone(), messages.clone())
            .with_temperature(0.0)
            .with_stream(false);
        let response = provider.complete(&request, cancel, &mut |_event: ProviderEvent| {})?;
        let raw = response.content.trim().to_string();
        match game.apply(&raw) {
            Ok(mv) => {
                game.note_attempts(mv.ply, attempt);
                return Ok(ModelMove {
                    mv,
                    reasoning: policy.keep_reasoning(response.reasoning),
                    raw,
                    attempts: attempt,
                });
            }
            Err(failure) => {
                // Tell the model what was wrong and try again.
                let reason = failure.to_string();
                messages.push(ChatMessage::assistant(raw.clone()));
                messages.push(ChatMessage::user(format!(
                    "That was not a legal move: {reason}. Reply with one legal move only."
                )));
                last_error = Some(error(
                    error_code::PROTOCOL,
                    format!("the model did not give a legal move: {reason}"),
                    true,
                ));
            }
        }
    }
    Err(last_error
        .unwrap_or_else(|| error(error_code::PROTOCOL, "the model gave no legal move", true)))
}

/// Evidence for comparing a sampled turn against the pilot's direct reply.
#[derive(Debug, serde::Serialize)]
pub struct KnightLmVote {
    pub samples: u8,
    pub legal_samples: u8,
    pub winning_votes: u8,
    pub direct_move: String,
}

/// Keep the trained prompt and choose among legal replies by frequency.
/// The direct reply wins ties when included among the tied moves. Every
/// sample uses the same legal-move grammar for the current position.
pub fn request_sampled_knightlm_move(
    game: &mut Match,
    provider: &dyn Provider,
    model: &ModelRef,
    cancel: &Cancellation,
    samples: u8,
    temperature: f32,
) -> Result<(ModelMove, KnightLmVote), ErrorInfo> {
    if !(2..=9).contains(&samples) || !(0.1..=1.5).contains(&temperature) {
        return Err(error(
            error_code::PROTOCOL,
            "KnightLM needs 2–9 samples and a temperature from 0.1 to 1.5",
            false,
        ));
    }
    let (request, legal) = knightlm_move_request(game, model)
        .map_err(|failure| error(error_code::PROTOCOL, failure.to_string(), false))?;
    let direct_move = complete_knightlm_move(provider, &request, &legal, cancel)?;
    let mut candidates = vec![(direct_move.clone(), 1u8)];
    let request = request.with_temperature(temperature);
    for _ in 1..samples {
        let uci = complete_knightlm_move(provider, &request, &legal, cancel)?;
        if let Some((_, votes)) = candidates.iter_mut().find(|(mv, _)| mv == &uci) {
            *votes += 1;
        } else {
            candidates.push((uci, 1));
        }
    }
    // max_by_key takes the last equal element, so reverse the insertion order.
    let (winner, winning_votes) = candidates
        .into_iter()
        .rev()
        .max_by_key(|(_, votes)| *votes)
        .expect("the direct move is always a candidate");
    let played = apply_knightlm_move(game, winner)?;
    Ok((
        played,
        KnightLmVote {
            samples,
            legal_samples: samples,
            winning_votes,
            direct_move,
        },
    ))
}

/// Ask a model to talk about the position.
///
/// Refused outright in a competitive game: the policy is the gate, not the
/// prompt, so nothing has to trust the model to stay quiet.
pub fn request_chat(
    game: &Match,
    provider: &dyn Provider,
    model: &ModelRef,
    policy: &PlayPolicy,
    message: &str,
    cancel: &Cancellation,
) -> Result<String, ErrorInfo> {
    if let Some(refusal) = policy.check(PlayAction::Chat) {
        return Err(refusal);
    }
    let system = format!(
        "You are playing a casual chess game and may talk about the position.\n\
         FEN: {}\nLegal moves: {}\nKeep answers short.",
        game.fen(),
        game.legal_moves().join(" ")
    );
    let request = CompletionRequest::new(
        model.model.clone(),
        vec![
            ChatMessage::system(system),
            ChatMessage::user(message.to_string()),
        ],
    )
    .with_temperature(0.3)
    .with_stream(false);
    let response = provider.complete(&request, cancel, &mut |_event: ProviderEvent| {})?;
    Ok(response.content)
}

/// The seat a model holds in a match, if any.
pub fn model_seat(config: &MatchConfig) -> Option<ModelRef> {
    let seat = match (&config.white, &config.black) {
        (Player::Model { provider, model }, _) => Some((provider, model)),
        (_, Player::Model { provider, model }) => Some((provider, model)),
        _ => None,
    };
    seat.map(|(provider, model)| ModelRef::new(provider.clone(), model.clone()))
}

// --- watching two models play ----------------------------------------------

/// Both seats of a watched game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchSeats {
    pub white: ModelRef,
    pub black: ModelRef,
}

/// The two model seats, if this match is a watched game at all.
pub fn watch_seats(config: &MatchConfig) -> Option<WatchSeats> {
    match (&config.white, &config.black) {
        (
            Player::Model {
                provider: wp,
                model: wm,
            },
            Player::Model {
                provider: bp,
                model: bm,
            },
        ) => Some(WatchSeats {
            white: ModelRef::new(wp.clone(), wm.clone()),
            black: ModelRef::new(bp.clone(), bm.clone()),
        }),
        _ => None,
    }
}

/// One ply of a watched game.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WatchStep {
    pub mv: MatchMove,
    pub reasoning: Option<String>,
    pub attempts: u32,
    /// Whether the reasoning was written into the PGN as a comment.
    pub annotated: bool,
}

/// Play one ply for whichever side is to move.
///
/// A watched game refuses moves while paused, and each side keeps its reasoning
/// only if its own toggle is on, so one side can be annotated and the other
/// left silent in the same game.
pub fn watch_step(
    game: &mut Match,
    provider: &dyn Provider,
    seats: &WatchSeats,
    cancel: &Cancellation,
) -> Result<WatchStep, ErrorInfo> {
    if game.is_paused() {
        return Err(error(error_code::UNSUPPORTED, "the game is paused", false));
    }
    let color = game.turn();
    let model = if color == shakmaty::Color::White {
        &seats.white
    } else {
        &seats.black
    };
    let keep = game.config().watch.for_side(color);
    // A watched game is moves-only as far as the policy is concerned; the
    // reasoning flag controls whether the note is kept, not whether talking is
    // allowed.
    let policy = PlayPolicy {
        kind: MatchKind::Competitive,
        request_reasoning: keep,
        show_reasoning: keep,
        export_reasoning: keep,
        allow_chat: false,
        allow_hints: false,
        allow_engine: false,
        allow_takeback: false,
    };
    let played = request_move(game, provider, model, &policy, cancel)?;
    let annotated = match played.reasoning.as_deref() {
        Some(reasoning) if !reasoning.trim().is_empty() => {
            game.annotate_ply(played.mv.ply, reasoning)
        }
        _ => false,
    };
    Ok(WatchStep {
        mv: played.mv,
        reasoning: played.reasoning,
        attempts: played.attempts,
        annotated,
    })
}
