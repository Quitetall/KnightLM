//! Play-schema read tools run on a FEN, for the tool-quoting reasoning corpus.
//!
//! stdin is JSONL `{"fen": "...", "calls": [{"name": "attackers_of", "arguments": {"square": "f7"}}, ...]}`;
//! stdout is one JSON line per input: `{"ok": true, "rendered": ["<tool_result>...</tool_result>", ...]}`.
//! Each call goes through `RuleSession` (the same executors the harness uses, at most eight per
//! position) and is rendered exactly as `model_turn::format_tool_result` renders it. Rules facts only.

use anyhow::{Context, Result, bail};
use knightlm_core::{SCHEMA_VERSION, state::*};
use serde_json::{Value, json};
use std::io::{BufRead, Read, Write};

fn run_call(session: &mut RuleSession, call: &Value) -> Result<ReadReply> {
    let name = call["name"].as_str().context("call needs a name")?;
    let args = &call["arguments"];
    let text = |key: &str| {
        args.get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    };
    match name {
        "list_legal_moves" => session.list_legal_moves(),
        "what_if" => {
            let moves: Vec<String> = args
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
        "attackers_of" => session.attackers_of(&text("square")),
        "defenders_of" => session.defenders_of(&text("square")),
        "gives_check" => session.gives_check(&text("move")),
        other => bail!("`{other}` is not a read tool"),
    }
}

fn one(request: &Value) -> Result<Value> {
    let fen = request["fen"].as_str().context("fen must be a string")?;
    let calls = request["calls"]
        .as_array()
        .context("calls must be an array")?;
    let record = GameRecord {
        schema_version: SCHEMA_VERSION,
        game_id: "corpus-reads".into(),
        source: "corpus-reads".into(),
        initial_fen: fen.into(),
        history_complete: false,
        moves: Vec::new(),
        variant: "standard".into(),
    };
    let mut session = RuleSession::new(GameState::replay(&record)?, 8);
    let mut rendered = Vec::new();
    for call in calls {
        let reply = run_call(&mut session, call)?;
        rendered.push(format!(
            "<tool_result>{}</tool_result>",
            serde_json::to_string(&reply)?
        ));
    }
    Ok(json!({"ok": true, "rendered": rendered}))
}

pub fn write_reads(input: impl Read, mut output: impl Write) -> Result<()> {
    for line in std::io::BufReader::new(input).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = serde_json::from_str(&line).context("bad request line")?;
        let reply = match one(&request) {
            Ok(done) => done,
            Err(error) => json!({"ok": false, "error": error.to_string()}),
        };
        writeln!(output, "{reply}")?;
    }
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_reads_like_the_harness() {
        let request = json!({"fen": "r1bqkbnr/pppp1ppp/2n5/4p2Q/2B1P3/8/PPPP1PPP/RNB1K1NR b KQkq - 3 3",
            "calls": [{"name": "attackers_of", "arguments": {"square": "f7"}},
                      {"name": "defenders_of", "arguments": {"square": "f7"}},
                      {"name": "gives_check", "arguments": {"move": "g8f6"}}]});
        let done = one(&request).unwrap();
        let out: Vec<&str> = done["rendered"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(out[0].contains("\"squares\":[\"c4\",\"h5\"]"), "{}", out[0]);
        assert!(out[1].contains("\"squares\":[\"e8\"]"), "{}", out[1]);
        assert!(out[2].contains("\"gives_check\":false"));
        assert!(out[0].starts_with("<tool_result>{\"call\":1,"));
    }

    #[test]
    fn a_bad_fen_is_reported_on_its_line() {
        let mut out = Vec::new();
        write_reads(&b"{\"fen\":\"nope\",\"calls\":[]}\n"[..], &mut out).unwrap();
        assert!(String::from_utf8(out).unwrap().contains("\"ok\":false"));
    }
}
