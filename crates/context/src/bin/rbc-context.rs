//! Headless deterministic context compiler.
//!
//! ```text
//! rbc-context compile --fen FEN [--history "e4 e5"] [--preset chat-v1] [--json]
//! rbc-context batch --input positions.jsonl --output contexts.jsonl
//! rbc-context hash --input context.json
//! rbc-context validate --spec spec.json
//! rbc-context sections
//! ```

use std::fs;
use std::io::{BufRead, BufReader, Write};

use anyhow::{Context, Result, bail};
use rbc_context::{CompiledContext, ContextInput, ContextSpec, Mode, Side, compile};
use serde::{Deserialize, Serialize};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else {
        print_usage();
        return Ok(());
    };
    match command {
        "compile" => compile_command(&args[1..]),
        "batch" => batch_command(&args[1..]),
        "hash" => hash_command(&args[1..]),
        "validate" => validate_command(&args[1..]),
        "sections" => {
            for id in rbc_context::SectionId::ALL {
                println!("{}", id.as_str());
            }
            Ok(())
        }
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(())
        }
        other => bail!("unknown command `{other}`; try `rbc-context help`"),
    }
}

fn print_usage() {
    println!(
        "rbc-context <command>\n\n\
         compile  --fen FEN [--history \"e4 e5\"] [--preset chat-v1] [--mode chat] [--side white] [--budget N]\n\
         batch    --input positions.jsonl --output contexts.jsonl [--preset training-v1]\n\
         hash     --input context.json\n\
         validate --spec spec.json\n\
         sections"
    );
}

fn compile_command(args: &[String]) -> Result<()> {
    let mut fen = None;
    let mut history = Vec::new();
    let mut preset = "chat-v1".to_string();
    let mut mode = Mode::Chat;
    let mut side = None;
    let mut budget = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--fen" => {
                index += 1;
                fen = Some(args.get(index).context("--fen needs a value")?.clone());
            }
            "--history" => {
                index += 1;
                let value = args.get(index).context("--history needs a value")?;
                history = value.split_whitespace().map(str::to_owned).collect();
            }
            "--preset" => {
                index += 1;
                preset = args.get(index).context("--preset needs a value")?.clone();
            }
            "--mode" => {
                index += 1;
                mode = match args.get(index).map(String::as_str) {
                    Some("chat") => Mode::Chat,
                    Some("play") => Mode::Play,
                    Some("watch") => Mode::Watch,
                    other => bail!("unknown mode {other:?}"),
                };
            }
            "--side" => {
                index += 1;
                side = match args.get(index).map(String::as_str) {
                    Some("white") => Some(Side::White),
                    Some("black") => Some(Side::Black),
                    other => bail!("unknown side {other:?}"),
                };
            }
            "--json" => {}
            "--budget" => {
                index += 1;
                budget = Some(
                    args.get(index)
                        .context("--budget needs a value")?
                        .parse::<u32>()?,
                );
            }
            other => bail!("unknown option `{other}`"),
        }
        index += 1;
    }
    let mut spec = ContextSpec::preset(&preset)
        .with_context(|| format!("unknown preset `{preset}`"))?;
    if let Some(budget) = budget {
        spec.token_budget = budget;
    }
    let input = ContextInput {
        fen,
        history,
        mode,
        side,
        ..Default::default()
    };
    let compiled = compile(&spec, &input)?;
    println!("{}", serde_json::to_string_pretty(&compiled)?);
    Ok(())
}

#[derive(Debug, Deserialize)]
struct BatchLine {
    id: String,
    #[serde(default)]
    fen: Option<String>,
    #[serde(default)]
    history: Vec<String>,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    eval_cp: Option<i64>,
    #[serde(default)]
    meta: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct BatchOut {
    id: String,
    schema: String,
    spec_id: String,
    spec_version: u32,
    tokenizer: String,
    prompt: String,
    context: serde_json::Value,
    tokens: u32,
    sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    eval_cp: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    meta: Option<serde_json::Value>,
}

fn batch_command(args: &[String]) -> Result<()> {
    let mut input_path = None;
    let mut output_path = None;
    let mut preset = "training-v1".to_string();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--input" => {
                index += 1;
                input_path = Some(args.get(index).context("--input needs a value")?.clone());
            }
            "--output" => {
                index += 1;
                output_path = Some(args.get(index).context("--output needs a value")?.clone());
            }
            "--preset" => {
                index += 1;
                preset = args.get(index).context("--preset needs a value")?.clone();
            }
            "--json" => {}
            other => bail!("unknown option `{other}`"),
        }
        index += 1;
    }
    let input_path = input_path.context("--input is required")?;
    let output_path = output_path.context("--output is required")?;
    let spec = ContextSpec::preset(&preset)
        .with_context(|| format!("unknown preset `{preset}`"))?;
    let reader = BufReader::new(fs::File::open(&input_path)?);
    let mut out = fs::File::create(&output_path)?;
    for (line_number, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let parsed: BatchLine = serde_json::from_str(&line)
            .with_context(|| format!("line {}: invalid JSON", line_number + 1))?;
        let input = ContextInput {
            fen: parsed.fen,
            history: parsed.history,
            ..Default::default()
        };
        let compiled = compile(&spec, &input)
            .with_context(|| format!("line {}: compile failed", line_number + 1))?;
        let record = BatchOut {
            id: parsed.id,
            schema: compiled.schema,
            spec_id: compiled.spec_id,
            spec_version: compiled.spec_version,
            tokenizer: compiled.tokenizer,
            prompt: compiled.prompt,
            context: compiled.data,
            tokens: compiled.tokens,
            sha256: compiled.sha256,
            target: parsed.target,
            eval_cp: parsed.eval_cp,
            meta: parsed.meta,
        };
        writeln!(out, "{}", serde_json::to_string(&record)?)?;
    }
    Ok(())
}

fn hash_command(args: &[String]) -> Result<()> {
    let mut input_path = None;
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--input" {
            index += 1;
            input_path = Some(args.get(index).context("--input needs a value")?.clone());
        } else if args[index] == "--json" {
        } else {
            bail!("unknown option `{}`", args[index]);
        }
        index += 1;
    }
    let input_path = input_path.context("--input is required")?;
    let compiled: CompiledContext = serde_json::from_str(&fs::read_to_string(input_path)?)?;
    if compiled.verify_hash()? {
        println!("{}", compiled.sha256);
        Ok(())
    } else {
        bail!("hash does not match the compiled packet")
    }
}

fn validate_command(args: &[String]) -> Result<()> {
    let mut spec_path = None;
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--spec" {
            index += 1;
            spec_path = Some(args.get(index).context("--spec needs a value")?.clone());
        } else {
            bail!("unknown option `{}`", args[index]);
        }
        index += 1;
    }
    let spec_path = spec_path.context("--spec is required")?;
    let spec: ContextSpec = serde_json::from_str(&fs::read_to_string(spec_path)?)?;
    let _ = compile(&spec, &ContextInput::start())?;
    println!("ok {}@{}", spec.id, spec.version);
    Ok(())
}
