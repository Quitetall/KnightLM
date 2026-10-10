use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use knightlm_core::{data::*, experiment::*, state::*};
use knightlm_model::{ModelProfile, smoke, verify_model};
use std::{fs::File, io::BufReader, path::PathBuf};

#[derive(Parser)]
#[command(
    version,
    about = "KnightLM research scaffold — fixture evidence, no pretrained model yet"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Render a deterministic digital-board SVG fixture (no image recognition).
    Render {
        game: PathBuf,
        #[arg(long)]
        black_bottom: bool,
        #[arg(long)]
        blue_theme: bool,
    },
    /// Replay a normalized game and compile its current packet.
    Compile {
        game: PathBuf,
        #[arg(long)]
        legal_choices: bool,
        #[arg(long)]
        features: bool,
        #[arg(long)]
        compact: bool,
    },
    /// Decode a packet and emit sorted compact JSON for cross-language parity.
    Canonicalize { packet: PathBuf },
    /// Generate deterministic move-effect probes with targets kept separate.
    Probes { game: PathBuf },
    /// Run all eight arms on fixture decisions. This does not measure a model.
    EvaluateFixtures { game: PathBuf },
    /// Validate a Lichess evaluation JSONL fixture (teacher data only).
    ValidateEvals { file: PathBuf },
    /// Detect split leakage by game, family and canonical scored position.
    AuditSplits { file: PathBuf },
    /// Validate a corpus provenance manifest array.
    ValidateCorpora { file: PathBuf },
    /// Check observed board placement against the authoritative ledger.
    VerifyObservation { game: PathBuf, observation: PathBuf },
    /// Apply a bounded sequence of single-ply hypothetical moves.
    Simulate {
        game: PathBuf,
        moves: Vec<String>,
        #[arg(long, default_value_t = 8)]
        budget: usize,
    },
    /// Exercise Burn CPU gradients and checkpoint persistence; no LLM claim.
    TrainSmoke { checkpoint: PathBuf },
    /// Print missing admission gates, then return a failing exit code.
    VerifyModel { profile: PathBuf },
}

fn read<T: serde::de::DeserializeOwned>(path: &PathBuf) -> Result<T> {
    Ok(serde_json::from_reader(BufReader::new(
        File::open(path).with_context(|| path.display().to_string())?,
    ))?)
}
fn game(path: &PathBuf) -> Result<GameState> {
    GameState::replay(&read::<GameRecord>(path)?)
}
fn emit<T: serde::Serialize>(value: &T, compact: bool) -> Result<()> {
    let value = serde_json::to_value(value)?;
    println!(
        "{}",
        if compact {
            serde_json::to_string(&value)?
        } else {
            serde_json::to_string_pretty(&value)?
        }
    );
    Ok(())
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Render {
            game: path,
            black_bottom,
            blue_theme,
        } => print!(
            "{}",
            knightlm_core::vision::render_svg(&game(&path)?, black_bottom, blue_theme)
        ),
        Command::Compile {
            game: path,
            legal_choices,
            features,
            compact,
        } => emit(
            &game(&path)?.compile(PacketOptions {
                legal_choices,
                derived_features: features,
            }),
            compact,
        )?,
        Command::Canonicalize { packet } => {
            let packet: ContextPacket = read(&packet)?;
            ensure!(
                packet.schema_version == 1 && packet.ruleset == "standard-untimed-v1",
                "unsupported packet contract"
            );
            emit(&packet, true)?;
        }
        Command::Probes { game: path } => emit(&generate_probes(&game(&path)?)?, false)?,
        Command::EvaluateFixtures { game: path } => {
            let state = game(&path)?;
            let selected = state
                .legal_moves()
                .first()
                .context("fixture needs nonterminal position")?
                .clone();
            let mut reports = Vec::new();
            for arm in Arm::ALL {
                let config = ExperimentConfig::fixture(arm);
                let request = prepare(&state, &config, &[], &FixtureTokenizer)?;
                let mut calls = 0;
                if arm.tools() {
                    let mut session = RuleSession::new(state.clone(), config.tool_budget);
                    ensure!(
                        session.simulate(0, &selected)?.legal,
                        "fixture simulation failed"
                    );
                    calls = session.calls();
                }
                let decision = Decision {
                    schema_version: 1,
                    uci_move: selected.clone(),
                    unmasked_uci_move: arm.masking().then(|| "a1a1".into()),
                    explanation:
                        "Scripted fixture selected first legal move; no learned reasoning occurred."
                            .into(),
                    claims: vec![FactClaim::LegalMove {
                        uci: selected.clone(),
                        legal: true,
                    }],
                    output_tokens: selected.chars().count(),
                    diagnostic_output_tokens: if arm.masking() { 4 } else { 0 },
                    tool_calls: calls,
                    interventions: usize::from(arm.masking()),
                    latency_ms: 0,
                };
                reports.push(score_decision(&state, &config, &request, decision)?);
            }
            emit(
                &serde_json::json!({"evidence":"fixture_only", "tokenizer":"fixture_character_counter", "model_evaluated":false, "runs":reports}),
                false,
            )?;
        }
        Command::ValidateEvals { file } => {
            let rows: Vec<EvaluationRow> = read_jsonl(BufReader::new(File::open(file)?))?;
            for row in &rows {
                row.validate()?;
            }
            emit(
                &serde_json::json!({"validated":rows.len(),"evidence":"teacher_schema_only","history_complete":false}),
                false,
            )?;
        }
        Command::AuditSplits { file } => {
            let items: Vec<SplitItem> = read(&file)?;
            audit_splits(&items)?;
            emit(
                &serde_json::json!({"items":items.len(),"cross_split_leaks":0}),
                false,
            )?;
        }
        Command::ValidateCorpora { file } => {
            let items: Vec<CorpusManifest> = read(&file)?;
            validate_manifests(&items)?;
            emit(&serde_json::json!({"validated":items.len()}), false)?;
        }
        Command::VerifyObservation {
            game: path,
            observation,
        } => {
            game(&path)?.verify_observation(&read::<BoardObservation>(&observation)?)?;
            emit(
                &serde_json::json!({"placement_matches":true,"repaired":false}),
                false,
            )?;
        }
        Command::Simulate {
            game: path,
            moves,
            budget,
        } => {
            let mut session = RuleSession::new(game(&path)?, budget);
            let mut branch = 0;
            let mut replies = Vec::new();
            for mv in moves {
                let reply = session.simulate(branch, &mv)?;
                if let Some(next) = reply.branch {
                    branch = next;
                }
                replies.push(reply);
            }
            emit(&replies, false)?;
        }
        Command::TrainSmoke { checkpoint } => emit(&smoke::train_smoke(&checkpoint)?, false)?,
        Command::VerifyModel { profile } => {
            let report = verify_model(&read::<ModelProfile>(&profile)?)?;
            emit(&report, false)?;
            ensure!(report.admitted, "pretrained model is not admitted");
        }
    }
    Ok(())
}
