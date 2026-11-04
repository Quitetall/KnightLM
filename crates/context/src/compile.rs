//! The compiler: pure, deterministic and fail-closed.

use anyhow::{Result, bail};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

use crate::annotations::Annotations;
use crate::canonical::{estimate_tokens, sha256_hex};
use crate::input::{ContextInput, Mode, Turn};
use crate::output::CompiledContext;
use crate::position::{self, PreparedPosition};
use crate::render::{self, Rendered};
use crate::section::Section;
use crate::spec::{ContextSpec, SectionId, TOKENIZER_ESTIMATE_V1};

struct Candidate {
    id: SectionId,
    priority: u8,
    required: bool,
    rendered: Rendered,
    tokens: u32,
}

impl Candidate {
    fn new(id: SectionId, required: bool, rendered: Rendered) -> Self {
        let tokens = estimate_tokens(&rendered.content);
        Self {
            id,
            priority: id.priority(),
            required,
            rendered,
            tokens,
        }
    }
}

/// Compile a context packet.
///
/// The result is a pure function of `spec` and `input`: no clock, randomness,
/// environment, network, database or engine is consulted. Required sections
/// that cannot fit the budget are an error, naming them.
pub fn compile(spec: &ContextSpec, input: &ContextInput) -> Result<CompiledContext> {
    if spec.tokenizer != TOKENIZER_ESTIMATE_V1 {
        bail!(
            "unsupported tokenizer `{}`; supported: {}",
            spec.tokenizer,
            TOKENIZER_ESTIMATE_V1
        );
    }
    if spec.token_budget == 0 {
        bail!("token_budget must be greater than zero");
    }

    let position = position::prepare(input)?;
    let evidence = input.evidence.normalized();
    let enabled = spec.enabled_sections();

    let mut required_ids: Vec<SectionId> = SectionId::ALL
        .into_iter()
        .filter(|id| id.is_required())
        .collect();
    if input.mode == Mode::Play && spec.legal_moves {
        required_ids.push(SectionId::LegalMoves);
    }
    let missing: Vec<&str> = required_ids
        .iter()
        .filter(|id| !enabled.contains(id))
        .map(|id| id.as_str())
        .collect();
    if !missing.is_empty() {
        bail!(
            "context spec is missing required section(s): {}",
            missing.join(", ")
        );
    }

    let windowed_turns = window_turns(&input.turns, spec.turn_window);
    let turns_window_truncated = windowed_turns.len() < input.turns.len();

    let mut candidates = Vec::new();
    for id in &enabled {
        match id {
            SectionId::System => candidates.push(Candidate::new(
                *id,
                true,
                render::system(spec, input, &position)?,
            )),
            SectionId::Position => {
                candidates.push(Candidate::new(*id, true, render::position(&position)?))
            }
            SectionId::Ascii => {
                candidates.push(Candidate::new(*id, true, render::ascii(&position)?))
            }
            SectionId::LegalMoves => {
                let required = required_ids.contains(id);
                candidates.push(Candidate::new(
                    *id,
                    required,
                    render::legal_moves(&position.legal_move_pairs())?,
                ));
            }
            SectionId::History => {
                if !position.history.is_empty() {
                    candidates.push(Candidate::new(*id, false, render::history(&position)?));
                }
            }
            SectionId::Annotations => {
                let merged = merged_annotations(input, &evidence);
                if !merged.is_empty() {
                    candidates.push(Candidate::new(*id, false, render::annotations(&merged)?));
                }
            }
            SectionId::Engine => {
                if let Some(engine) = &evidence.engine {
                    candidates.push(Candidate::new(*id, false, render::engine(engine)?));
                }
            }
            SectionId::Openings => {
                if let Some(openings) = &evidence.openings {
                    candidates.push(Candidate::new(*id, false, render::openings(openings)?));
                }
            }
            SectionId::Database => {
                if let Some(database) = &evidence.database {
                    candidates.push(Candidate::new(*id, false, render::database(database)?));
                }
            }
            SectionId::Notes => {
                if let Some(notes) = &evidence.notes {
                    if !notes.trim().is_empty() {
                        candidates.push(Candidate::new(*id, false, render::notes(notes)?));
                    }
                }
            }
            SectionId::Siblings => {
                if !evidence.siblings.is_empty() {
                    candidates.push(Candidate::new(
                        *id,
                        false,
                        render::boards(*id, &evidence.siblings)?,
                    ));
                }
            }
            SectionId::Compare => {
                if !evidence.compare.is_empty() {
                    candidates.push(Candidate::new(
                        *id,
                        false,
                        render::boards(*id, &evidence.compare)?,
                    ));
                }
            }
            SectionId::Turns => {
                if !windowed_turns.is_empty() {
                    candidates.push(Candidate::new(*id, false, render::turns(&windowed_turns)?));
                }
            }
        }
    }

    candidates.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.id.cmp(&b.id)));

    let required_total: u32 = candidates
        .iter()
        .filter(|candidate| candidate.required)
        .map(|candidate| candidate.tokens)
        .sum();
    if required_total > spec.token_budget {
        let names = candidates
            .iter()
            .filter(|candidate| candidate.required)
            .map(|candidate| format!("{} ({} tokens)", candidate.id.as_str(), candidate.tokens))
            .collect::<Vec<_>>()
            .join(", ");
        bail!(
            "required sections exceed token budget {}: {}",
            spec.token_budget,
            names
        );
    }

    let mut remaining = spec.token_budget - required_total;
    let mut sections = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        if candidate.required {
            sections.push(section_from(&candidate, true, false));
            continue;
        }
        if candidate.id == SectionId::Turns {
            let section = truncate_turns(
                &candidate,
                &windowed_turns,
                turns_window_truncated,
                remaining,
            )?;
            if section.included {
                remaining -= section.tokens;
            }
            sections.push(section);
            continue;
        }
        if candidate.id == SectionId::Notes {
            let section = truncate_notes(&candidate, remaining)?;
            if section.included {
                remaining -= section.tokens;
            }
            sections.push(section);
            continue;
        }
        if candidate.tokens <= remaining {
            remaining -= candidate.tokens;
            sections.push(section_from(&candidate, true, false));
        } else {
            sections.push(section_from(&candidate, false, false));
        }
    }

    let prompt = render::prompt(&sections);
    let tokens: u32 = sections
        .iter()
        .filter(|section| section.included)
        .map(|section| section.tokens)
        .sum();
    let data = compiled_data(&sections, &enabled, &evidence)?;

    let mut compiled = CompiledContext::new(
        spec.id.clone(),
        spec.version,
        spec.tokenizer.clone(),
        sections,
        prompt,
        data,
        tokens,
    );
    compiled.sha256 = sha256_hex(&compiled.canonical_bytes()?);
    Ok(compiled)
}

fn merged_annotations(input: &ContextInput, evidence: &crate::evidence::Evidence) -> Annotations {
    let mut merged = input.annotations.clone();
    if let Some(annotations) = &evidence.annotations {
        merged = merged.merged(annotations);
    }
    merged
}

fn window_turns(turns: &[Turn], window: usize) -> Vec<Turn> {
    if window == 0 || turns.len() <= window {
        return if window == 0 {
            Vec::new()
        } else {
            turns.to_vec()
        };
    }
    turns[turns.len() - window..].to_vec()
}

fn section_from(candidate: &Candidate, included: bool, truncated: bool) -> Section {
    section_parts(
        candidate,
        candidate.rendered.content.clone(),
        candidate.rendered.data.clone(),
        included,
        truncated,
    )
}

fn section_parts(
    candidate: &Candidate,
    content: String,
    data: Value,
    included: bool,
    truncated: bool,
) -> Section {
    Section {
        id: candidate.id.as_str().to_owned(),
        version: 1,
        priority: candidate.priority,
        required: candidate.required,
        tokens: estimate_tokens(&content),
        content,
        data: Some(data),
        included,
        truncated,
    }
}

/// Keep the newest complete turns that fit. Never split a turn.
fn truncate_turns(
    candidate: &Candidate,
    turns: &[Turn],
    window_truncated: bool,
    remaining: u32,
) -> Result<Section> {
    if candidate.tokens <= remaining {
        return Ok(section_from(candidate, true, window_truncated));
    }
    for keep in (1..=turns.len()).rev() {
        let suffix = &turns[turns.len() - keep..];
        let rendered = render::turns(suffix)?;
        let tokens = estimate_tokens(&rendered.content);
        if tokens <= remaining {
            return Ok(section_parts(
                candidate,
                rendered.content,
                rendered.data,
                true,
                true,
            ));
        }
    }
    Ok(section_from(candidate, false, true))
}

/// Truncate long notes at paragraph boundaries with an explicit marker.
fn truncate_notes(candidate: &Candidate, remaining: u32) -> Result<Section> {
    if candidate.tokens <= remaining {
        return Ok(section_from(candidate, true, false));
    }
    let text = candidate
        .rendered
        .content
        .strip_prefix("[notes]\n")
        .unwrap_or(&candidate.rendered.content);
    let paragraphs: Vec<&str> = text
        .split("\n\n")
        .filter(|paragraph| !paragraph.is_empty())
        .collect();
    let mut best: Option<Rendered> = None;
    for index in 1..=paragraphs.len() {
        let mut partial = paragraphs[..index].join("\n\n");
        if index < paragraphs.len() {
            partial.push_str("\n\n[truncated]");
        }
        let rendered = render::notes(&partial)?;
        if estimate_tokens(&rendered.content) <= remaining {
            best = Some(rendered);
        } else {
            break;
        }
    }
    match best {
        Some(rendered) => Ok(section_parts(
            candidate,
            rendered.content,
            rendered.data,
            true,
            true,
        )),
        None => Ok(section_from(candidate, false, true)),
    }
}

fn compiled_data(
    sections: &[Section],
    enabled: &[SectionId],
    evidence: &crate::evidence::Evidence,
) -> Result<Value> {
    let mut map: BTreeMap<String, Value> = BTreeMap::new();
    for section in sections {
        if section.included {
            if let Some(data) = &section.data {
                map.insert(section.id.clone(), data.clone());
            }
        }
    }
    let fingerprints = evidence.fingerprints()?;
    let fingerprints: Map<String, Value> = fingerprints
        .into_iter()
        .filter(|(key, _)| enabled.iter().any(|id| id.as_str() == *key))
        .map(|(key, value)| (key.to_owned(), Value::String(value)))
        .collect();
    if !fingerprints.is_empty() {
        map.insert(
            "evidence_fingerprints".to_owned(),
            Value::Object(fingerprints),
        );
    }
    Ok(Value::Object(map.into_iter().collect()))
}

/// Expose a position-only helper for callers that need the canonical FEN
/// without compiling a whole packet.
pub fn canonical_fen(input: &ContextInput) -> Result<String> {
    Ok(position::prepare(input)?.fen)
}

/// Render the canonical ASCII board for an input, without compiling a packet.
pub fn canonical_ascii(input: &ContextInput) -> Result<String> {
    let position: PreparedPosition = position::prepare(input)?;
    Ok(render::ascii(&position)?.content)
}
