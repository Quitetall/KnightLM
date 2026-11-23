//! The built-in test bot: an opponent that needs no Python, no model, no GPU and no network.
//!
//! It speaks the seam contract (docs/model/seam.md) in process, so the whole play and coach
//! path works with zero setup. It is NOT a model. Its moves are pseudo-random legal moves and
//! its chat is a short canned reply that says so. The app labels it as a test bot everywhere
//! and never presents it as KnightLM.

use serde_json::{Value, json};

use crate::protocol::{ErrorInfo, StopReason, ToolCall, TurnRole, Usage};
use crate::provider::{
    Cancellation, CompletionRequest, CompletionResponse, Provider, ProviderEvent, error, error_code,
};

/// The seat provider id of the test bot.
pub const TEST_BOT_PROVIDER: &str = "local:rbc-test-bot";
/// The seat model id of the test bot.
pub const TEST_BOT_MODEL: &str = "rbc-test-bot";
/// The label every surface shows for it.
pub const TEST_BOT_LABEL: &str = "Test bot (random moves)";
/// The `knightlm.manifest_hash` it reports, so a record never reads as a model's.
pub const TEST_BOT_MANIFEST: &str = "rbc-test-bot";
/// The `knightlm.path` it reports.
pub const TEST_BOT_PATH: &str = "test_bot";

/// Whether a seat is the test bot.
pub fn is_test_bot(provider: &str) -> bool {
    provider == TEST_BOT_PROVIDER
}

/// A seeded random-move opponent.
#[derive(Debug, Clone)]
pub struct TestBotProvider {
    seed: u64,
}

impl TestBotProvider {
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    /// The move for this packet text: a function of the seed and the position, so a test is
    /// repeatable and the same position always gets the same answer.
    fn pick(&self, legal: &[String], packet_text: &str) -> String {
        // FNV-1a over the seed and the packet, then a splitmix finish.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325 ^ self.seed;
        for byte in packet_text.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash = (hash ^ (hash >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        hash = (hash ^ (hash >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        hash ^= hash >> 31;
        legal[(hash % legal.len() as u64) as usize].clone()
    }
}

/// The packet in a user message: the whole text, or what follows the play prompt's marker.
fn packet_of(text: &str) -> Option<Value> {
    let body = text
        .rsplit_once("Position packet:\n")
        .map_or(text, |(_, rest)| rest);
    serde_json::from_str(body.trim()).ok()
}

fn protocol(message: &str) -> ErrorInfo {
    error(error_code::PROTOCOL, message, false)
}

fn short(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= 80 {
        flat
    } else {
        format!("{}...", flat.chars().take(80).collect::<String>())
    }
}

impl Provider for TestBotProvider {
    fn name(&self) -> &str {
        "rbc_test_bot"
    }

    fn complete(
        &self,
        request: &CompletionRequest,
        _cancel: &Cancellation,
        _on_event: &mut dyn FnMut(ProviderEvent),
    ) -> Result<CompletionResponse, ErrorInfo> {
        let packet_text = request
            .messages
            .iter()
            .rev()
            .find(|message| message.role == TurnRole::User)
            .and_then(|message| message.content.clone())
            .ok_or_else(|| protocol("the test bot got no position"))?;
        let packet =
            packet_of(&packet_text).ok_or_else(|| protocol("the test bot got no position"))?;
        let legal: Vec<String> = packet
            .get("legal_moves")
            .and_then(Value::as_array)
            .map(|moves| {
                moves
                    .iter()
                    .filter_map(|mv| mv.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        if legal.is_empty() {
            return Err(protocol("the test bot has no legal move in this position"));
        }
        let mv = self.pick(&legal, &packet_text);
        let extension = request.knightlm.as_ref();
        let coach = extension
            .and_then(|ext| ext.get("turn"))
            .and_then(Value::as_str)
            == Some("coach");
        let ext = |coach_part: Option<Value>| {
            let mut out = json!({
                "path": TEST_BOT_PATH, "code": "ok", "raw_code": "ok", "legal": true,
                "manifest_hash": TEST_BOT_MANIFEST
            });
            if let Some(part) = coach_part {
                out["coach"] = part;
            }
            out
        };
        if !coach {
            return Ok(CompletionResponse {
                content: String::new(),
                reasoning: None,
                tool_calls: vec![ToolCall {
                    id: "test_bot_1".into(),
                    name: "submit_move".into(),
                    arguments: json!({
                        "reasoning": "Test bot: a random legal move, no thinking behind it.",
                        "move": mv
                    }),
                }],
                usage: Usage::default(),
                stop_reason: StopReason::ToolCalls,
                provider_request_id: None,
                knightlm: Some(ext(None)),
            });
        }
        // Coach turn: the last thing the person said, before the packet.
        let said = request
            .messages
            .iter()
            .rev()
            .skip(1)
            .find(|message| message.role == TurnRole::User)
            .and_then(|message| message.content.as_deref().map(short));
        let mut reply = String::from(
            "I am the RBC test bot, not a language model. I pick random legal moves and \
             cannot discuss the position.",
        );
        if let Some(said) = said {
            reply.push_str(&format!(" You said: \"{said}\"."));
        }
        let text = format!("Provisional: {mv}\n{reply}");
        let to = mv.get(2..4).unwrap_or("e4").to_owned();
        Ok(CompletionResponse {
            content: text,
            reasoning: None,
            tool_calls: vec![ToolCall {
                id: "test_bot_2".into(),
                name: "mark_square".into(),
                arguments: json!({"square": to, "color": "blue"}),
            }],
            usage: Usage::default(),
            stop_reason: StopReason::EndTurn,
            provider_request_id: None,
            knightlm: Some(ext(Some(json!({
                "move": mv, "provisional": [mv], "rejected": 0,
                "fallback": false, "refused_calls": []
            })))),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ChatMessage;

    fn packet(moves: &[&str]) -> String {
        json!({"legal_moves": moves}).to_string()
    }

    fn ask(bot: &TestBotProvider, request: CompletionRequest) -> CompletionResponse {
        bot.complete(&request, &Cancellation::new(), &mut |_| {})
            .expect("answer")
    }

    #[test]
    fn it_plays_a_legal_move_and_the_same_one_for_the_same_seed() {
        let moves = ["e2e4", "d2d4", "g1f3", "b1c3"];
        let request = |text: String| {
            CompletionRequest::new("rbc-test-bot", vec![ChatMessage::user(text)])
                .with_knightlm(json!({"turn": "play", "mode": "raw"}))
        };
        let play = format!("Do the thing.\nPosition packet:\n{}", packet(&moves));
        let first = ask(&TestBotProvider::new(7), request(play.clone()));
        let again = ask(&TestBotProvider::new(7), request(play));
        assert_eq!(first, again, "seeded: repeatable");
        let mv = first.tool_calls[0].arguments["move"].as_str().unwrap();
        assert!(moves.contains(&mv));
        assert_eq!(first.knightlm.unwrap()["manifest_hash"], TEST_BOT_MANIFEST);
    }

    #[test]
    fn different_seeds_do_not_all_pick_the_same_move() {
        let moves: Vec<String> = (0..20)
            .map(|n| format!("a{}a{}", n % 8 + 1, n % 7 + 1))
            .collect();
        let refs: Vec<&str> = moves.iter().map(String::as_str).collect();
        let text = packet(&refs);
        let picks: std::collections::HashSet<String> = (0..16)
            .map(|seed| TestBotProvider::new(seed).pick(&moves, &text))
            .collect();
        assert!(picks.len() > 1);
    }

    #[test]
    fn a_coach_turn_says_it_is_a_test_bot_and_ends_in_a_provisional_move() {
        let request = CompletionRequest::new(
            "rbc-test-bot",
            vec![
                ChatMessage::user("Are you a real model?"),
                ChatMessage::user(packet(&["e7e5"])),
            ],
        )
        .with_knightlm(json!({"turn": "coach", "mode": "raw"}));
        let response = ask(&TestBotProvider::new(1), request);
        assert!(response.content.starts_with("Provisional: e7e5\n"));
        assert!(response.content.contains("test bot"));
        assert!(response.content.contains("Are you a real model?"));
        let ext = response.knightlm.unwrap();
        assert_eq!(ext["coach"]["move"], "e7e5");
        assert_eq!(response.tool_calls[0].arguments["square"], "e5");
    }

    #[test]
    fn it_refuses_a_position_without_legal_moves() {
        let request = CompletionRequest::new("rbc-test-bot", vec![ChatMessage::user(packet(&[]))]);
        assert!(
            TestBotProvider::new(1)
                .complete(&request, &Cancellation::new(), &mut |_| {})
                .is_err()
        );
    }
}
