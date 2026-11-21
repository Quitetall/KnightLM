use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use knightlm_core::{data::*, experiment::*, state::*};
use knightlm_model::{ModelProfile, smoke, verify_model};
use std::{fs::File, io::BufReader, path::PathBuf};

mod board_labels;
mod corpus;
mod facts;
mod score_cmd;

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
    /// Stream one compiled decision per eligible Lichess game from PGN on stdin.
    CorpusCandidates {
        #[arg(long)]
        source: String,
        #[arg(long)]
        limit: usize,
    },
    /// Replay PGN main lines on notation; emit comment rows and quarantine failures.
    CorpusCommentary {
        #[arg(long)]
        source: String,
    },
    /// Rules-only position facts (side, check, material, captures, a move's effect) for JSONL on stdin.
    CorpusFacts,
    /// Rules-QA labels (squares, attack maps, legal moves, check, material) for JSONL `{fen}` on stdin; board-encoder pretraining.
    BoardLabels,
    /// Render a position as Chess Graph Text (cgt/1); see docs/model/graph-text.md.
    Cgt {
        #[arg(long)]
        fen: String,
        /// `lite` (board + legal moves per piece) or `full` (plus edges).
        #[arg(long, default_value = "full")]
        variant: String,
        /// Repetition count from the packet; omitted means `rep ?` (unknown).
        #[arg(long)]
        rep: Option<u32>,
    },
    /// Deterministic random playouts from the start position (plus any six-field
    /// FENs on stdin); one JSON row `{fen, legal_moves}` per position, legal moves
    /// in UCI from the compiler's rules core. Test data for the ctok/1 tokenizer.
    Playouts {
        #[arg(long, default_value_t = 20)]
        games: usize,
        #[arg(long, default_value_t = 120)]
        plies: usize,
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Also read FENs (one per line) from stdin.
        #[arg(long)]
        stdin_fens: bool,
    },
    /// Replay JSONL SAN lines ({id, fen?, sans}) and report validated FENs.
    ReplayLines,
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
    /// Post-decision scoring: centipawn loss of each `{id?, fen, move}` JSONL row
    /// under a fixed-depth, single-thread Stockfish. Reads stdin when no file.
    ScoreMoves {
        #[arg(long)]
        engine: Option<String>,
        #[arg(long, default_value_t = 12)]
        depth: u32,
        /// Treat --depth as a node count (`go nodes N`); for lc0 and Maia.
        #[arg(long)]
        nodes: bool,
        #[arg(long, default_value_t = 1)]
        threads: u32,
        /// Hash size in MB; 0 sends no Hash option (lc0 has none).
        #[arg(long, default_value_t = 64)]
        hash_mb: u32,
        /// Extra UCI option `NAME=VALUE` (repeatable), e.g. WeightsFile=/path.
        #[arg(long = "option", value_parser = parse_option)]
        options: Vec<(String, String)>,
        input: Option<PathBuf>,
    },
    /// MultiPV ranking of each `{id?, fen}` JSONL row (tactical-set derivation).
    RankPositions {
        #[arg(long)]
        engine: Option<String>,
        #[arg(long, default_value_t = 12)]
        depth: u32,
        #[arg(long, default_value_t = 2)]
        lines: u32,
        /// Treat --depth as a node count (`go nodes N`); for lc0 and Maia.
        #[arg(long)]
        nodes: bool,
        #[arg(long, default_value_t = 1)]
        threads: u32,
        /// Hash size in MB; 0 sends no Hash option (lc0 has none).
        #[arg(long, default_value_t = 64)]
        hash_mb: u32,
        /// Extra UCI option `NAME=VALUE` (repeatable), e.g. WeightsFile=/path.
        #[arg(long = "option", value_parser = parse_option)]
        options: Vec<(String, String)>,
        input: Option<PathBuf>,
    },
    /// Print missing admission gates, then return a failing exit code.
    VerifyModel { profile: PathBuf },
}

fn parse_option(raw: &str) -> Result<(String, String), String> {
    match raw.split_once('=') {
        Some((name, value)) if !name.trim().is_empty() && !value.trim().is_empty() => {
            Ok((name.trim().to_owned(), value.trim().to_owned()))
        }
        _ => Err(format!("expected NAME=VALUE, got {raw:?}")),
    }
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

fn playouts(games: usize, plies: usize, seed: u64, stdin_fens: bool) -> Result<()> {
    use knightlm_notation::fen::{CastlingStyle, parse_fen, write_fen};
    use shakmaty::{CastlingMode, Chess, Position, uci::UciMove};
    use std::io::{BufRead, Write};
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    let mut row = |pos: &Chess| -> Result<()> {
        let mut moves: Vec<String> = pos
            .legal_moves()
            .iter()
            .map(|m| UciMove::from_move(*m, CastlingMode::Standard).to_string())
            .collect();
        moves.sort();
        let value = serde_json::json!({
            "fen": write_fen(pos, CastlingStyle::XFen),
            "legal_moves": moves,
        });
        writeln!(out, "{value}")?;
        Ok(())
    };
    let mut state = seed.max(1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..games {
        let mut pos = Chess::default();
        row(&pos)?;
        for _ in 0..plies {
            let moves = pos.legal_moves();
            if moves.is_empty() {
                break;
            }
            let mv = moves[(next() % moves.len() as u64) as usize];
            pos.play_unchecked(mv);
            row(&pos)?;
        }
    }
    if stdin_fens {
        for line in std::io::stdin().lock().lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let parsed = parse_fen(line.trim())?;
            ensure!(parsed.has_clocks, "playouts needs six-field FENs");
            row(&parsed.position)?;
        }
    }
    out.flush()?;
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
                ..PacketOptions::default()
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
        Command::CorpusCandidates { source, limit } => corpus::write_candidates(
            std::io::stdin().lock(),
            std::io::BufWriter::new(std::io::stdout().lock()),
            &source,
            limit,
        )?,
        Command::CorpusCommentary { source } => corpus::write_commentary(
            std::io::stdin().lock(),
            std::io::BufWriter::new(std::io::stdout().lock()),
            &source,
        )?,
        Command::CorpusFacts => facts::write_facts(
            std::io::stdin().lock(),
            std::io::BufWriter::new(std::io::stdout().lock()),
        )?,
        Command::BoardLabels => board_labels::write_labels(
            std::io::stdin().lock(),
            std::io::BufWriter::new(std::io::stdout().lock()),
        )?,
        Command::Cgt { fen, variant, rep } => {
            let variant = match variant.as_str() {
                "lite" => knightlm_notation::cgt::CgtVariant::Lite,
                "full" => knightlm_notation::cgt::CgtVariant::Full,
                other => anyhow::bail!("unknown cgt variant {other:?}; use lite or full"),
            };
            let parsed = knightlm_notation::fen::parse_fen(&fen)?;
            ensure!(
                parsed.has_clocks,
                "cgt needs a six-field FEN (halfmove and fullmove clocks)"
            );
            let facts = knightlm_notation::cgt::CgtFacts {
                repetition_count: rep,
                terminal_reason: None,
            };
            let text = knightlm_notation::cgt::render_cgt(&parsed.position, &facts, variant);
            // Refuse to print anything the parser would not read back.
            knightlm_notation::cgt::parse_cgt(&text)?;
            println!("{text}");
        }
        Command::Playouts {
            games,
            plies,
            seed,
            stdin_fens,
        } => playouts(games, plies, seed, stdin_fens)?,
        Command::ReplayLines => corpus::replay_lines(
            std::io::stdin().lock(),
            std::io::BufWriter::new(std::io::stdout().lock()),
        )?,
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
        Command::ScoreMoves {
            engine,
            depth,
            nodes,
            threads,
            hash_mb,
            options,
            input,
        } => score_cmd::run(
            engine,
            depth,
            None,
            input,
            score_cmd::OracleSettings {
                nodes,
                threads,
                hash_mb,
                options,
            },
        )?,
        Command::RankPositions {
            engine,
            depth,
            lines,
            nodes,
            threads,
            hash_mb,
            options,
            input,
        } => score_cmd::run(
            engine,
            depth,
            Some(lines),
            input,
            score_cmd::OracleSettings {
                nodes,
                threads,
                hash_mb,
                options,
            },
        )?,
        Command::VerifyModel { profile } => {
            let report = verify_model(&read::<ModelProfile>(&profile)?)?;
            emit(&report, false)?;
            ensure!(report.admitted, "pretrained model is not admitted");
        }
    }
    Ok(())
}
