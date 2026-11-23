//! One model turn under the modes-and-tools contract (docs/model/modes-and-tools.md).
//!
//! Pure pieces, no provider: the mode prompts and their fixed prefixes, the exact
//! play-mode completion format and its parser, the coach `Provisional:` commit
//! rule, the read-query executor on [`RuleSession`], the bounded chat context and
//! the deterministic chat summarizer. `play.rs` wires them to a provider.
//!
//! Prompt text, the completion format and the summarizer exist twice, here and in
//! `model/prompt.py`. `crates/contract/schemas/model_prompts.json`, `play_completion_cases.json`
//! and `chat_summary_cases.json` pin the two together: Rust generates or checks
//! them and the Python tests read the same files.

use std::fmt;

use knightlm_core::state::{ReadReply, RuleSession};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::{
    ModelCall, ModelMode, SubmitMoveGate, SubmitOutcome, TOOL_CALL_CLOSE, TOOL_CALL_OPEN,
    model_tools_json,
};

/// Rule queries a play turn may spend, shared by every read tool and counted by
/// the same [`RuleSession`] budget the CLI fixtures use.
pub const READ_BUDGET: usize = 8;
pub const MAX_REASONING_CHARS: usize = 400;
pub const MAX_SUMMARY_CHARS: usize = 1200;
pub const MAX_RECENT_MESSAGES: usize = 6;
pub const MAX_MESSAGE_CHARS: usize = 800;
/// Longest excerpt of one message inside a rolling summary.
pub const EXCERPT_CHARS: usize = 120;

pub const PLAY_INSTRUCTION: &str = "You are playing chess in competition mode. Reply with exactly one tool call: call submit_move with a short reasoning field first, then a legal UCI move. Use only the tools listed.\n";
pub const COACH_INSTRUCTION: &str = "You are a chess coach playing a game. Your first line must be `Provisional: <legal UCI move>`. You may think aloud and later write another `Provisional: <legal UCI move>` line to change it; the last legal one is your move when you stop. Then reply to the user. Use only the tools listed.\n";
pub const COMBINED_INSTRUCTION: &str = "You are a chess coach that also plays competitively. The line `Mode: play` or `Mode: coach` after the tool lists names this turn's mode; call only that mode's tools. In play mode reply with exactly one submit_move call, reasoning field first, then a legal UCI move. In coach mode your first line must be `Provisional: <legal UCI move>`, which a later `Provisional:` line may revise (the last legal one is your move), then reply to the user.\n";
pub const TOOL_CALL_FORMAT: &str =
    "<tool_call>{\"name\": \"<tool>\", \"arguments\": {...}}</tool_call>\n";

fn tools_blob(mode: ModelMode) -> String {
    let all = model_tools_json();
    serde_json::to_string(&all["modes"][mode.as_str()]).expect("tool schemas serialize")
}

/// The fixed part of a prompt for one mode: instruction, call format, this
/// mode's schema. Never mentions the other mode's tools.
pub fn mode_prefix(mode: ModelMode) -> String {
    let instruction = match mode {
        ModelMode::Play => PLAY_INSTRUCTION,
        ModelMode::Coach => COACH_INSTRUCTION,
    };
    format!(
        "{instruction}{TOOL_CALL_FORMAT}Tools:\n{}\n",
        tools_blob(mode)
    )
}

/// The fixed prefix of a long chat: both schemas, labelled. Each call is still
/// checked against the schema of the mode named on the `Mode:` line.
pub fn combined_prefix() -> String {
    format!(
        "{COMBINED_INSTRUCTION}{TOOL_CALL_FORMAT}Play tools:\n{}\nCoach tools:\n{}\n",
        tools_blob(ModelMode::Play),
        tools_blob(ModelMode::Coach)
    )
}

/// One chat message kept verbatim in the recent window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatLine {
    /// `user` or `assistant`.
    pub role: String,
    pub text: String,
}

impl ChatLine {
    pub fn new(role: &str, text: &str) -> Self {
        Self {
            role: role.into(),
            text: text.into(),
        }
    }
}

/// Bounded rolling chat context. A summary or message over its bound is an
/// error; only the last [`MAX_RECENT_MESSAGES`] messages are shown.
pub fn chat_context(summary: Option<&str>, recent: &[ChatLine]) -> Result<String, String> {
    let mut out = String::new();
    if let Some(summary) = summary.filter(|s| !s.is_empty()) {
        if summary.chars().count() > MAX_SUMMARY_CHARS {
            return Err("chat summary exceeds MAX_SUMMARY_CHARS".into());
        }
        out.push_str(&format!("Conversation summary: {summary}\n"));
    }
    let skip = recent.len().saturating_sub(MAX_RECENT_MESSAGES);
    for message in &recent[skip..] {
        if message.role != "user" && message.role != "assistant" {
            return Err(format!("unknown chat role {:?}", message.role));
        }
        if message.text.chars().count() > MAX_MESSAGE_CHARS {
            return Err("chat message exceeds MAX_MESSAGE_CHARS".into());
        }
        out.push_str(&format!("{}: {}\n", message.role, message.text));
    }
    Ok(out)
}

/// Play mode: instruction, play schema, packet last. Nothing else.
pub fn play_prompt(packet_json: &str) -> String {
    format!(
        "{}Position packet:\n{packet_json}",
        mode_prefix(ModelMode::Play)
    )
}

/// Coach mode: fixed prefix, bounded chat context, packet last.
pub fn coach_prompt(
    packet_json: &str,
    summary: Option<&str>,
    recent: &[ChatLine],
) -> Result<String, String> {
    Ok(format!(
        "{}{}Position packet:\n{packet_json}",
        mode_prefix(ModelMode::Coach),
        chat_context(summary, recent)?
    ))
}

/// Long chat: both schemas, then the turn's mode, chat context, packet last.
pub fn combined_prompt(
    packet_json: &str,
    mode: ModelMode,
    summary: Option<&str>,
    recent: &[ChatLine],
) -> Result<String, String> {
    Ok(format!(
        "{}Mode: {}\n{}Position packet:\n{packet_json}",
        combined_prefix(),
        mode.as_str(),
        chat_context(summary, recent)?
    ))
}

/// Everything Python must agree with, generated from the Rust values.
pub fn model_prompts_json() -> Value {
    let recent = [
        ChatLine::new("user", "Why not Nf3?"),
        ChatLine::new("assistant", "It blocks the f-pawn."),
    ];
    let packet = r#"{"fen":"8/8/8/8/8/8/8/K6k w - - 0 1"}"#;
    json!({
        "constants": {
            "play_instruction": PLAY_INSTRUCTION,
            "coach_instruction": COACH_INSTRUCTION,
            "combined_instruction": COMBINED_INSTRUCTION,
            "tool_call_format": TOOL_CALL_FORMAT,
            "max_summary_chars": MAX_SUMMARY_CHARS,
            "max_recent_messages": MAX_RECENT_MESSAGES,
            "max_message_chars": MAX_MESSAGE_CHARS,
            "max_reasoning_chars": MAX_REASONING_CHARS,
            "excerpt_chars": EXCERPT_CHARS,
        },
        "prefixes": {
            "play": mode_prefix(ModelMode::Play),
            "coach": mode_prefix(ModelMode::Coach),
            "combined": combined_prefix(),
        },
        "example": {
            "packet": packet,
            "summary": "We discussed the Ruy Lopez.",
            "recent": [
                {"role": "user", "text": "Why not Nf3?"},
                {"role": "assistant", "text": "It blocks the f-pawn."},
            ],
            "play": play_prompt(packet),
            "coach": coach_prompt(packet, Some("We discussed the Ruy Lopez."), &recent)
                .expect("example is in bounds"),
            "combined_play": combined_prompt(
                packet, ModelMode::Play, Some("We discussed the Ruy Lopez."), &recent
            ).expect("example is in bounds"),
            "combined_coach": combined_prompt(
                packet, ModelMode::Coach, Some("We discussed the Ruy Lopez."), &recent
            ).expect("example is in bounds"),
        },
    })
}

// --- play-mode completion format ---------------------------------------------

/// Why text is not a canonical play-mode completion. [`Self::code`] is the
/// spelling the Python parser shares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayCompletionError {
    NotOneCall,
    ExtraText,
    Malformed,
    WrongTool,
    KeyOrder,
    ReasoningMissing,
    ReasoningTooLong,
    BadMove,
}

impl PlayCompletionError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::NotOneCall => "not_one_call",
            Self::ExtraText => "extra_text",
            Self::Malformed => "malformed",
            Self::WrongTool => "wrong_tool",
            Self::KeyOrder => "key_order",
            Self::ReasoningMissing => "reasoning_missing",
            Self::ReasoningTooLong => "reasoning_too_long",
            Self::BadMove => "bad_move",
        }
    }
}

impl fmt::Display for PlayCompletionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl std::error::Error for PlayCompletionError {}

/// A parsed canonical play completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayCompletion {
    pub reasoning: String,
    pub mv: String,
}

/// The exact play-mode completion the model is trained to write: one
/// `submit_move` call whose arguments are `reasoning` then `move`, and nothing
/// outside the block.
pub fn format_play_completion(reasoning: &str, mv: &str) -> String {
    format!(
        "{TOOL_CALL_OPEN}{{\"name\":\"submit_move\",\"arguments\":{{\"reasoning\":{},\"move\":{}}}}}{TOOL_CALL_CLOSE}",
        serde_json::to_string(reasoning).expect("string serializes"),
        serde_json::to_string(mv).expect("string serializes"),
    )
}

/// An object whose keys keep their order (and duplicates).
struct Pairs(Vec<(String, Value)>);

impl<'de> Deserialize<'de> for Pairs {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visit;
        impl<'de> serde::de::Visitor<'de> for Visit {
            type Value = Pairs;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Pairs, A::Error> {
                let mut pairs = Vec::new();
                while let Some(entry) = map.next_entry::<String, Value>()? {
                    pairs.push(entry);
                }
                Ok(Pairs(pairs))
            }
        }
        deserializer.deserialize_map(Visit)
    }
}

fn is_uci_shape(mv: &str) -> bool {
    let b = mv.as_bytes();
    let square = |f: u8, r: u8| (b'a'..=b'h').contains(&f) && (b'1'..=b'8').contains(&r);
    (b.len() == 4 || (b.len() == 5 && matches!(b[4], b'q' | b'r' | b'b' | b'n')))
        && square(b[0], b[1])
        && square(b[2], b[3])
}

/// Parse the canonical play completion. Stricter than [`crate::tools::parse_model_call`]:
/// no text outside the block, the call is `submit_move`, `reasoning` is present
/// (1..=400 characters) and comes before `move`, and `move` has UCI shape.
/// Legality is the gate's job, not the parser's.
pub fn parse_play_completion(text: &str) -> Result<PlayCompletion, PlayCompletionError> {
    use PlayCompletionError as E;
    if text.matches(TOOL_CALL_OPEN).count() != 1 || text.matches(TOOL_CALL_CLOSE).count() != 1 {
        return Err(E::NotOneCall);
    }
    let start = text.find(TOOL_CALL_OPEN).expect("counted above");
    let end = text.find(TOOL_CALL_CLOSE).expect("counted above");
    if end < start {
        return Err(E::NotOneCall);
    }
    let after = end + TOOL_CALL_CLOSE.len();
    if !text[..start].trim().is_empty() || !text[after..].trim().is_empty() {
        return Err(E::ExtraText);
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Top {
        name: String,
        arguments: Box<serde_json::value::RawValue>,
    }
    let top: Top = serde_json::from_str(text[start + TOOL_CALL_OPEN.len()..end].trim())
        .map_err(|_| E::Malformed)?;
    let name = top.name;
    let args: Pairs = serde_json::from_str(top.arguments.get()).map_err(|_| E::Malformed)?;
    if name != "submit_move" {
        return Err(E::WrongTool);
    }
    let keys: Vec<&str> = args.0.iter().map(|(k, _)| k.as_str()).collect();
    if keys != ["reasoning", "move"] {
        return Err(match keys.as_slice() {
            ["move", "reasoning"] => E::KeyOrder,
            ["move"] => E::ReasoningMissing,
            _ => E::Malformed,
        });
    }
    let reasoning = match &args.0[0].1 {
        Value::String(s) if !s.trim().is_empty() => s,
        _ => return Err(E::ReasoningMissing),
    };
    if reasoning.chars().count() > MAX_REASONING_CHARS {
        return Err(E::ReasoningTooLong);
    }
    match &args.0[1].1 {
        Value::String(mv) if is_uci_shape(mv) => Ok(PlayCompletion {
            reasoning: reasoning.clone(),
            mv: mv.clone(),
        }),
        _ => Err(E::BadMove),
    }
}

// --- coach: the provisional commit rule ---------------------------------------

/// The move named by a `Provisional: <token>` line, if `line` is one. The token
/// is whatever the model wrote; the gate decides whether it is legal.
pub fn provisional_token(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("Provisional:")?;
    let mut parts = rest.split_whitespace();
    let token = parts.next()?;
    parts.next().is_none().then_some(token)
}

/// Coach text with the `Provisional:` lines removed: the reply a person sees.
pub fn strip_provisional_lines(text: &str) -> String {
    text.lines()
        .filter(|line| provisional_token(line).is_none())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

/// Applies the owner's coach commit rule. Every `Provisional:` line goes through
/// the same [`SubmitMoveGate`] as a play `submit_move`; the last legal one is the
/// committed move. A later illegal line never erases an earlier legal one.
#[derive(Debug, Clone)]
pub struct ProvisionalTracker {
    gate: SubmitMoveGate,
    committed: Option<String>,
    accepted: u32,
    rejected: u32,
    exhausted: bool,
    feedback: Vec<String>,
}

impl ProvisionalTracker {
    pub fn new(legal: Vec<String>, max_attempts: u32) -> Self {
        Self {
            gate: SubmitMoveGate::new(legal, max_attempts),
            committed: None,
            accepted: 0,
            rejected: 0,
            exhausted: false,
            feedback: Vec::new(),
        }
    }

    /// Read the provisional lines of `text`, in order. `finished` is false for a
    /// stream that stopped early (tokens, time): an unterminated last line may be
    /// cut mid-move and is then ignored. Lines after the attempts run out are
    /// ignored. Returns the errors to show the model, if another round follows.
    pub fn feed(&mut self, text: &str, finished: bool) -> Vec<String> {
        self.feedback.clear();
        let mut lines: Vec<&str> = text.split('\n').collect();
        if !finished {
            lines.pop(); // the unterminated tail
        }
        for line in lines {
            let Some(token) = provisional_token(line.trim_end_matches('\r')) else {
                continue;
            };
            if self.exhausted {
                break;
            }
            match self.gate.submit(token) {
                SubmitOutcome::Accepted(mv) => {
                    self.accepted += 1;
                    self.committed = Some(mv);
                }
                SubmitOutcome::Rejected { error, .. } => {
                    self.rejected += 1;
                    self.feedback.push(error);
                }
                SubmitOutcome::Fallback => {
                    self.rejected += 1;
                    self.exhausted = true;
                }
            }
        }
        self.feedback.clone()
    }

    /// The last legal provisional move, if any.
    pub fn committed(&self) -> Option<&str> {
        self.committed.as_deref()
    }

    pub fn attempts(&self) -> u32 {
        self.gate.attempts()
    }

    /// Legal provisional lines read (a revision is every one after the first).
    pub fn accepted(&self) -> u32 {
        self.accepted
    }

    pub fn rejected(&self) -> u32 {
        self.rejected
    }

    pub fn exhausted(&self) -> bool {
        self.exhausted
    }
}

// --- play: read queries on the shared RuleSession budget ----------------------

/// Run one validated read call (`list_legal_moves`, `what_if`, `attackers_of`,
/// `defenders_of`, `gives_check`) against the session. Every call spends the
/// session's budget, so the eight-call limit is shared; `Err` is the text to give
/// the model back (budget spent, or not a read tool).
pub fn execute_read(session: &mut RuleSession, call: &ModelCall) -> Result<ReadReply, String> {
    let text = |key: &str| {
        call.arguments
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
    };
    let outcome = match call.name.as_str() {
        "list_legal_moves" => session.list_legal_moves(),
        "what_if" => {
            let moves: Vec<String> = call
                .arguments
                .get("moves")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            session.what_if(&moves)
        }
        "attackers_of" => session.attackers_of(text("square")),
        "defenders_of" => session.defenders_of(text("square")),
        "gives_check" => session.gives_check(text("move")),
        other => return Err(format!("`{other}` is not a read tool")),
    };
    outcome.map_err(|err| err.to_string())
}

/// A read result as the model sees it.
pub fn format_tool_result(reply: &ReadReply) -> String {
    format!(
        "<tool_result>{}</tool_result>",
        serde_json::to_string(reply).expect("reply serializes")
    )
}

// --- the deterministic chat summarizer -----------------------------------------

fn excerpt(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= EXCERPT_CHARS {
        return flat;
    }
    let mut short: String = flat.chars().take(EXCERPT_CHARS - 1).collect();
    short.push('…');
    short
}

/// Fold dropped messages into the rolling summary. No model call: each message
/// becomes `U: <excerpt>` or `A: <excerpt>` (whitespace flattened, at most
/// [`EXCERPT_CHARS`] characters), entries are joined with ` | `, and when the
/// result exceeds [`MAX_SUMMARY_CHARS`] the oldest whole entries are dropped and
/// the summary starts with `…`. The same input always gives the same output.
pub fn summarize_chat(previous: Option<&str>, dropped: &[ChatLine]) -> String {
    let mut parts: Vec<String> = previous
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .into_iter()
        .collect();
    for message in dropped {
        let tag = if message.role == "user" { "U" } else { "A" };
        parts.push(format!("{tag}: {}", excerpt(&message.text)));
    }
    let joined = parts.join(" | ");
    let total = joined.chars().count();
    if total <= MAX_SUMMARY_CHARS {
        return joined;
    }
    let tail: String = joined
        .chars()
        .skip(total - (MAX_SUMMARY_CHARS - 1))
        .collect();
    let tail = match tail.find(" | ") {
        Some(cut) => &tail[cut + 3..],
        None => &tail,
    };
    format!("…{tail}")
}

/// Keep the last [`MAX_RECENT_MESSAGES`] messages and fold older ones into the
/// summary. Returns the new summary (if any) and the recent window.
pub fn roll_chat(summary: Option<&str>, messages: &[ChatLine]) -> (Option<String>, Vec<ChatLine>) {
    let skip = messages.len().saturating_sub(MAX_RECENT_MESSAGES);
    let recent = messages[skip..].to_vec();
    if skip == 0 {
        return (summary.map(str::to_owned), recent);
    }
    (Some(summarize_chat(summary, &messages[..skip])), recent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use knightlm_core::state::{GameRecord, GameState, START_FEN};

    fn legal() -> Vec<String> {
        vec!["e2e4".into(), "g1f3".into(), "d2d4".into()]
    }

    // -- coach commit rule

    #[test]
    fn revising_replaces_the_earlier_provisional() {
        let mut t = ProvisionalTracker::new(legal(), 5);
        t.feed(
            "Provisional: e2e4\nhmm, wait\nProvisional: g1f3\nbecause...",
            true,
        );
        assert_eq!(t.committed(), Some("g1f3"));
        assert_eq!(t.accepted(), 2);
    }

    #[test]
    fn an_illegal_later_provisional_keeps_the_legal_earlier_one() {
        let mut t = ProvisionalTracker::new(legal(), 5);
        let feedback = t.feed(
            "Provisional: e2e4\nProvisional: e2e5\nProvisional: zzzz\n",
            true,
        );
        assert_eq!(t.committed(), Some("e2e4"));
        assert_eq!(t.rejected(), 2);
        assert_eq!(feedback.len(), 2);
        assert!(feedback[0].contains("not a legal move"));
    }

    #[test]
    fn no_valid_provisional_means_no_commit() {
        let mut t = ProvisionalTracker::new(legal(), 2);
        t.feed("Provisional: e2e5\nProvisional: h7h1\n", true);
        assert_eq!(t.committed(), None, "caller uses the constrained fallback");
        assert!(t.exhausted());
        // Text with no provisional line at all is the same: nothing committed.
        let mut t = ProvisionalTracker::new(legal(), 2);
        t.feed("Move: e2e4\nI think e4.", true);
        assert_eq!((t.committed(), t.attempts()), (None, 0));
    }

    #[test]
    fn exhausted_retries_commit_the_last_valid_and_ignore_later_lines() {
        let mut t = ProvisionalTracker::new(legal(), 3);
        t.feed(
            "Provisional: d2d4\nProvisional: bad1\nProvisional: bad2\nProvisional: e2e4\n",
            true,
        );
        assert!(t.exhausted());
        assert_eq!(
            t.committed(),
            Some("d2d4"),
            "the legal line after exhaustion is ignored"
        );
        assert_eq!(t.attempts(), 3);
    }

    #[test]
    fn a_stream_cut_mid_line_ignores_the_partial_line() {
        let mut t = ProvisionalTracker::new(legal(), 5);
        // Out of tokens in the middle of the second line.
        t.feed("Provisional: e2e4\nProvisional: g1f", false);
        assert_eq!(t.committed(), Some("e2e4"));
        assert_eq!(t.attempts(), 1, "the cut line was not even an attempt");
        // The same text from a finished stream treats the last line as complete.
        let mut t = ProvisionalTracker::new(legal(), 5);
        t.feed("Provisional: e2e4\nProvisional: g1f3", true);
        assert_eq!(t.committed(), Some("g1f3"));
    }

    #[test]
    fn only_whole_provisional_lines_count() {
        assert_eq!(provisional_token("Provisional: e2e4"), Some("e2e4"));
        assert_eq!(provisional_token("Provisional:e2e4  "), Some("e2e4"));
        assert_eq!(provisional_token("Provisional: I think e4"), None);
        assert_eq!(provisional_token("My Provisional: e2e4"), None);
        assert_eq!(provisional_token("Provisional:"), None);
        assert_eq!(
            strip_provisional_lines("Provisional: e2e4\nGood move.\n"),
            "Good move."
        );
    }

    // -- play completion

    #[test]
    fn play_completion_round_trips_and_refuses_variants() {
        let text = format_play_completion("Develop the knight.", "g1f3");
        assert_eq!(
            text,
            r#"<tool_call>{"name":"submit_move","arguments":{"reasoning":"Develop the knight.","move":"g1f3"}}</tool_call>"#
        );
        let parsed = parse_play_completion(&text).unwrap();
        assert_eq!(
            (parsed.reasoning.as_str(), parsed.mv.as_str()),
            ("Develop the knight.", "g1f3")
        );
        let code = |t: &str| parse_play_completion(t).unwrap_err().code();
        assert_eq!(code("g1f3"), "not_one_call");
        assert_eq!(code(&format!("I think:\n{text}")), "extra_text");
        assert_eq!(
            code(&text.replace("submit_move", "list_legal_moves")),
            "wrong_tool"
        );
        assert_eq!(
            code(
                r#"<tool_call>{"name":"submit_move","arguments":{"move":"g1f3","reasoning":"x"}}</tool_call>"#
            ),
            "key_order"
        );
        assert_eq!(
            code(r#"<tool_call>{"name":"submit_move","arguments":{"move":"g1f3"}}</tool_call>"#),
            "reasoning_missing"
        );
        assert_eq!(
            code(&format_play_completion(&"x".repeat(401), "g1f3")),
            "reasoning_too_long"
        );
        assert_eq!(code(&format_play_completion("ok", "Nf3")), "bad_move");
        assert_eq!(code("<tool_call>{nope</tool_call>"), "malformed");
    }

    // -- read queries

    fn session(moves: &[&str], budget: usize) -> RuleSession {
        let state = GameState::replay(&GameRecord {
            schema_version: knightlm_core::SCHEMA_VERSION,
            game_id: "t".into(),
            source: "t".into(),
            initial_fen: START_FEN.into(),
            history_complete: true,
            moves: moves.iter().map(|m| m.to_string()).collect(),
            variant: "standard".into(),
        })
        .unwrap();
        RuleSession::new(state, budget)
    }

    fn call(name: &str, arguments: Value) -> ModelCall {
        ModelCall {
            name: name.into(),
            arguments,
        }
    }

    fn result(reply: &ReadReply) -> Value {
        reply.result.clone().expect("query succeeded")
    }

    #[test]
    fn queries_match_shakmaty_and_share_the_budget() {
        // After 1.e4 e5 2.Qh5 Nc6 3.Bc4: white threatens f7; black to move after Nf6?? is not needed.
        let mut s = session(&["e2e4", "e7e5", "d1h5", "b8c6", "f1c4"], 8);
        let legal = result(&execute_read(&mut s, &call("list_legal_moves", json!({}))).unwrap());
        let moves: Vec<&str> = legal["moves"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m.as_str().unwrap())
            .collect();
        assert!(moves.contains(&"g8f6") && !moves.contains(&"e1e2"));
        assert_eq!(moves.len(), 28);
        // Black's f7 is attacked by Qh5 and Bc4 and defended by the king.
        let attackers =
            result(&execute_read(&mut s, &call("attackers_of", json!({"square": "f7"}))).unwrap());
        assert_eq!(attackers["squares"], json!(["c4", "h5"]));
        let defenders =
            result(&execute_read(&mut s, &call("defenders_of", json!({"square": "f7"}))).unwrap());
        assert_eq!(defenders["squares"], json!(["e8"]));
        // Qxf7 gives check (and mate); a quiet move does not.
        let mate =
            result(&execute_read(&mut s, &call("gives_check", json!({"move": "g8f6"}))).unwrap());
        assert_eq!(mate["gives_check"], false);
        let what_if = result(
            &execute_read(&mut s, &call("what_if", json!({"moves": ["g8f6", "h5f7"]}))).unwrap(),
        );
        assert_eq!(what_if["in_check"], true);
        assert!(what_if["fen"].as_str().unwrap().contains("Q"));
        assert_eq!(s.calls(), 5);
        // No score, evaluation or ranking appears in any answer.
        for value in [&legal, &attackers, &defenders, &mate, &what_if] {
            let text = value.to_string().to_lowercase();
            for banned in ["score", "eval", "cp", "best", "rank"] {
                assert!(!text.contains(&format!("\"{banned}")), "{banned} in {text}");
            }
        }
    }

    #[test]
    fn every_query_spends_budget_and_the_ninth_is_refused() {
        let mut s = session(&[], 8);
        for _ in 0..8 {
            execute_read(&mut s, &call("list_legal_moves", json!({}))).unwrap();
        }
        assert_eq!(s.remaining(), 0);
        for query in [
            call("list_legal_moves", json!({})),
            call("what_if", json!({"moves": ["e2e4"]})),
            call("attackers_of", json!({"square": "e4"})),
            call("gives_check", json!({"move": "e2e4"})),
        ] {
            let err = execute_read(&mut s, &query).unwrap_err();
            assert!(err.contains("budget exhausted"), "{err}");
        }
        assert_eq!(s.calls(), 8, "a refused call is not charged twice");
        // Bad requests also consume budget, like simulate().
        let mut s = session(&[], 2);
        let bad = execute_read(&mut s, &call("gives_check", json!({"move": "e2e5"}))).unwrap();
        assert!(!bad.ok && bad.error.is_some());
        assert!(
            !execute_read(&mut s, &call("attackers_of", json!({"square": "e4"})))
                .unwrap()
                .ok
        );
        assert_eq!(s.remaining(), 0);
        assert!(execute_read(&mut s, &call("submit_move", json!({}))).is_err());
    }

    // -- chat

    #[test]
    fn summarizer_is_deterministic_and_bounded() {
        let long = "word ".repeat(100);
        let msgs: Vec<ChatLine> = (0..40)
            .map(|i| {
                ChatLine::new(
                    if i % 2 == 0 { "user" } else { "assistant" },
                    &format!("{i} {long}"),
                )
            })
            .collect();
        let a = summarize_chat(None, &msgs);
        assert_eq!(a, summarize_chat(None, &msgs));
        assert!(a.chars().count() <= MAX_SUMMARY_CHARS);
        assert!(a.starts_with('…') && a.contains("U: ") && a.contains("39 "));
        // The newest entry survives, the oldest is gone.
        assert!(a.contains("A: 39 ") && !a.contains("U: 0 "));
        let small = summarize_chat(
            Some("U: hi"),
            &[ChatLine::new("assistant", "  hello \n there ")],
        );
        assert_eq!(small, "U: hi | A: hello there");
        let (summary, recent) = roll_chat(None, &msgs[..3]);
        assert_eq!((summary, recent.len()), (None, 3));
        let (summary, recent) = roll_chat(None, &msgs);
        assert_eq!(recent, msgs[34..]);
        assert!(summary.unwrap().chars().count() <= MAX_SUMMARY_CHARS);
    }

    #[test]
    fn chat_context_enforces_its_bounds() {
        assert!(chat_context(Some(&"x".repeat(1201)), &[]).is_err());
        assert!(chat_context(None, &[ChatLine::new("user", &"x".repeat(801))]).is_err());
        assert!(chat_context(None, &[ChatLine::new("system", "x")]).is_err());
        let many: Vec<ChatLine> = (0..10)
            .map(|i| ChatLine::new("user", &format!("m{i}")))
            .collect();
        let ctx = chat_context(None, &many).unwrap();
        assert!(ctx.contains("m9") && !ctx.contains("m3"));
    }

    // -- prompts

    #[test]
    fn prefixes_are_disjoint_by_mode_and_the_combined_one_shows_both() {
        let play = mode_prefix(ModelMode::Play);
        let coach = mode_prefix(ModelMode::Coach);
        let both = combined_prefix();
        let names = |mode| -> Vec<String> {
            crate::tools::model_tool_specs(mode)
                .into_iter()
                .map(|t| t.name)
                .collect()
        };
        for n in names(ModelMode::Coach) {
            assert!(
                !play.contains(&n) && coach.contains(&n) && both.contains(&n),
                "{n}"
            );
        }
        for n in names(ModelMode::Play) {
            assert!(
                !coach.contains(&n) && play.contains(&n) && both.contains(&n),
                "{n}"
            );
        }
        let a = combined_prompt("{\"a\":1}", ModelMode::Play, None, &[]).unwrap();
        let b = combined_prompt("{\"b\":2}", ModelMode::Coach, Some("s"), &[]).unwrap();
        assert!(a.starts_with(&both) && b.starts_with(&both));
        assert!(
            a.ends_with("{\"a\":1}") && a.contains("Mode: play\n") && b.contains("Mode: coach\n")
        );
        assert_eq!(a.matches("Position packet:").count(), 1);
    }

    fn check_fixture(file: &str, generated: &Value) {
        let path = format!("{}/{file}", knightlm_contract::schemas_dir());
        if std::env::var_os("UPDATE_MODEL_TOOLS").is_some() {
            std::fs::write(
                &path,
                serde_json::to_string_pretty(generated).unwrap() + "\n",
            )
            .unwrap();
        }
        let on_disk: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            &on_disk, generated,
            "regenerate with UPDATE_MODEL_TOOLS=1 cargo test -p rbc-ai --features core model_turn"
        );
    }

    #[test]
    fn checked_in_prompt_fixture_matches_rust() {
        check_fixture("model_prompts.json", &model_prompts_json());
    }

    /// Cases both parsers answer identically; Python reads the same file.
    fn completion_cases() -> Value {
        let ok = |reasoning: &str, mv: &str| json!({"text": format_play_completion(reasoning, mv), "ok": {"reasoning": reasoning, "move": mv}});
        let bad = |text: &str, code: &str| json!({"text": text, "error": code});
        let call = |inner: &str| format!("<tool_call>{inner}</tool_call>");
        json!([
            ok("Develop the knight toward the center.", "g1f3"),
            ok("Promote: a quote \" a backslash \\ and an accent é.\nNew line.", "a7a8q"),
            ok(&"x".repeat(400), "e2e4"),
            {"text": format!("  \n{}\n ", format_play_completion("Spaced.", "e2e4")), "ok": {"reasoning": "Spaced.", "move": "e2e4"}},
            bad("g1f3", "not_one_call"),
            bad(&format!("{0}{0}", format_play_completion("a", "e2e4")), "not_one_call"),
            bad(&format!("Thinking first.\n{}", format_play_completion("a", "e2e4")), "extra_text"),
            bad(&format!("{} done", format_play_completion("a", "e2e4")), "extra_text"),
            bad("<tool_call>{nope</tool_call>", "malformed"),
            bad(&call(r#"[1,2]"#), "malformed"),
            bad(&call(r#"{"name":"submit_move","arguments":{"reasoning":"a","move":"e2e4"},"z":1}"#), "malformed"),
            bad(&call(r#"{"name":"submit_move"}"#), "malformed"),
            bad(&call(r#"{"name":"submit_move","arguments":[]}"#), "malformed"),
            bad(&call(r#"{"name":"submit_move","arguments":{"reasoning":"a","move":"e2e4","extra":1}}"#), "malformed"),
            bad(&call(r#"{"name":"submit_move","arguments":{"reasoning":"a","reasoning":"b"}}"#), "malformed"),
            bad(&call(r#"{"name":"list_legal_moves","arguments":{}}"#), "wrong_tool"),
            bad(&call(r#"{"name":"submit_move","arguments":{"move":"e2e4","reasoning":"a"}}"#), "key_order"),
            bad(&call(r#"{"name":"submit_move","arguments":{"move":"e2e4"}}"#), "reasoning_missing"),
            bad(&call(r#"{"name":"submit_move","arguments":{"reasoning":"   ","move":"e2e4"}}"#), "reasoning_missing"),
            bad(&call(r#"{"name":"submit_move","arguments":{"reasoning":5,"move":"e2e4"}}"#), "reasoning_missing"),
            bad(&format_play_completion(&"x".repeat(401), "e2e4"), "reasoning_too_long"),
            bad(&format_play_completion("a", "Nf3"), "bad_move"),
            bad(&format_play_completion("a", "e2e9"), "bad_move"),
            bad(&format_play_completion("a", "e7e8k"), "bad_move"),
            bad(&call(r#"{"name":"submit_move","arguments":{"reasoning":"a","move":4}}"#), "bad_move"),
        ])
    }

    #[test]
    fn checked_in_completion_cases_match_the_rust_parser_and_formatter() {
        let cases = completion_cases();
        check_fixture("play_completion_cases.json", &cases);
        for case in cases.as_array().unwrap() {
            let text = case["text"].as_str().unwrap();
            match (case.get("ok"), case.get("error")) {
                (Some(ok), None) => {
                    let parsed = parse_play_completion(text).unwrap();
                    assert_eq!(parsed.reasoning, ok["reasoning"].as_str().unwrap());
                    assert_eq!(parsed.mv, ok["move"].as_str().unwrap());
                    assert_eq!(
                        text.trim(),
                        format_play_completion(&parsed.reasoning, &parsed.mv)
                    );
                }
                (None, Some(code)) => {
                    assert_eq!(
                        parse_play_completion(text).unwrap_err().code(),
                        code,
                        "{text}"
                    );
                }
                _ => panic!("case needs exactly one of ok / error"),
            }
        }
    }

    #[test]
    fn checked_in_summary_cases_match_the_rust_summarizer() {
        let long = "word ".repeat(100);
        let many: Vec<Value> = (0..40)
            .map(|i| json!({"role": if i % 2 == 0 { "user" } else { "assistant" }, "text": format!("{i} {long}")}))
            .collect();
        let inputs = [
            (
                Value::Null,
                json!([{"role": "assistant", "text": "  hello \n there "}]),
            ),
            (
                json!("U: hi"),
                json!([{"role": "user", "text": "Why not Nf3?"}]),
            ),
            (
                Value::Null,
                json!([{"role": "user", "text": "y".repeat(300)}]),
            ),
            (Value::Null, Value::Array(many)),
        ];
        let cases: Vec<Value> = inputs
            .iter()
            .map(|(prev, msgs)| {
                let lines: Vec<ChatLine> = msgs
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|m| ChatLine::new(m["role"].as_str().unwrap(), m["text"].as_str().unwrap()))
                    .collect();
                json!({"previous": prev, "messages": msgs, "summary": summarize_chat(prev.as_str(), &lines)})
            })
            .collect();
        check_fixture("chat_summary_cases.json", &Value::Array(cases));
    }

    /// Provisional-line cases both trackers answer identically; Python reads the same file.
    #[test]
    fn checked_in_provisional_cases_match_the_rust_tracker() {
        let legal = ["e2e4", "g1f3", "d2d4", "e7e8q"];
        let inputs: Vec<(u32, &str, bool)> = vec![
            (5, "Provisional: e2e4\nThen the reply.", true),
            (
                5,
                "Provisional: e2e4\nhmm, wait\nProvisional: g1f3\nbecause...",
                true,
            ),
            (
                5,
                "Provisional: e2e4\nProvisional: e2e5\nProvisional: zzzz\n",
                true,
            ),
            (2, "Provisional: e2e5\nProvisional: h7h1\n", true),
            (2, "No provisional line at all.", true),
            (
                3,
                "Provisional: d2d4\nProvisional: bad1\nProvisional: bad2\nProvisional: e2e4\n",
                true,
            ),
            (5, "Provisional: e2e4\nProvisional: g1f", false),
            (5, "Provisional: e2e4\nProvisional: g1f3", true),
            (5, "Provisional: g1f3", false),
            (5, "Provisional:e2e4  \r\nProvisional: g1f3\r\nok", true),
            (
                5,
                "Provisional: I think e4\nMy Provisional: e2e4\nProvisional:\n",
                true,
            ),
            (5, "Provisional: E2E4\nProvisional: e2e4 e2e4\n", true),
            (1, "Provisional: e2e5\nProvisional: e2e4\n", true),
            (5, "Provisional: e7e8q\nPromoted.\n", true),
            (5, "", true),
        ];
        let cases: Vec<Value> = inputs
            .iter()
            .map(|(max, text, finished)| {
                let mut tracker =
                    ProvisionalTracker::new(legal.iter().map(|m| (*m).to_owned()).collect(), *max);
                let feedback = tracker.feed(text, *finished);
                let committed = tracker.committed().map(str::to_owned);
                let reply = committed.is_some().then(|| strip_provisional_lines(text));
                json!({
                    "legal": legal,
                    "max_attempts": max,
                    "text": text,
                    "finished": finished,
                    "committed": committed,
                    "fallback": tracker.committed().is_none(),
                    "attempts": tracker.attempts(),
                    "accepted": tracker.accepted(),
                    "rejected": tracker.rejected(),
                    "exhausted": tracker.exhausted(),
                    "feedback": feedback,
                    "reply": reply,
                })
            })
            .collect();
        check_fixture("provisional_cases.json", &Value::Array(cases));
    }
}
