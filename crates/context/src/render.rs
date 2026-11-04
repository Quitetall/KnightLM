//! Fixed renderers. Every renderer is a pure function of its arguments.

use anyhow::{Context, Result};
use serde_json::{Value, json};
use shakmaty::{File, Rank, Square};

use crate::annotations::{Annotation, Annotations};
use crate::evidence::{BoardSummary, DatabaseEvidence, EngineEvidence, OpeningsEvidence};
use crate::input::{ContextInput, Turn};
use crate::position::PreparedPosition;
use crate::spec::{ContextSpec, PROMPT_TEMPLATE_VERSION, SectionId};

/// The two pieces every section renderer produces.
pub(crate) struct Rendered {
    pub(crate) content: String,
    pub(crate) data: Value,
}

fn rendered(id: SectionId, body: String, data: Value) -> Rendered {
    Rendered {
        content: crate::canonical::normalize_text(&format!("[{}]\n{}", id.as_str(), body)),
        data,
    }
}

pub(crate) fn system(
    spec: &ContextSpec,
    input: &ContextInput,
    position: &PreparedPosition,
) -> Result<Rendered> {
    let output = match input.mode {
        crate::input::Mode::Play => "one-legal-san-move",
        crate::input::Mode::Chat | crate::input::Mode::Watch => "json-object-v1",
    };
    let spec_text = format!("{}@{}", spec.id, spec.version);
    let mut body = String::new();
    body.push_str("role: rusty-brick-chess-assistant\n");
    body.push_str(&format!("template: {PROMPT_TEMPLATE_VERSION}\n"));
    body.push_str(&format!("spec: {spec_text}\n"));
    body.push_str(&format!("mode: {}\n", input.mode.as_str()));
    if let Some(side) = input.side {
        body.push_str(&format!("seat: {}\n", side.as_str()));
    }
    body.push_str(&format!("ruleset: {}\n", position.ruleset()));
    body.push_str(&format!("output: {output}"));
    let data = json!({
        "role": "rusty-brick-chess-assistant",
        "template": PROMPT_TEMPLATE_VERSION,
        "spec": spec_text,
        "mode": input.mode.as_str(),
        "seat": input.side.map(|side| side.as_str()),
        "ruleset": position.ruleset(),
        "output": output,
    });
    Ok(rendered(SectionId::System, body, data))
}

pub(crate) fn position(position: &PreparedPosition) -> Result<Rendered> {
    let facts = position.state.facts();
    let fields: Vec<&str> = position.fen.split_whitespace().collect();
    let castling = fields.get(2).copied().unwrap_or("-");
    let en_passant = fields.get(3).copied().unwrap_or("-");
    let halfmove = fields
        .get(4)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    let fullmove = fields
        .get(5)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1);
    let body = format!(
        concat!(
            "fen: {}\n",
            "side_to_move: {}\n",
            "variant: {}\n",
            "castling: {}\n",
            "en_passant: {}\n",
            "halfmove_clock: {}\n",
            "fullmove_number: {}\n",
            "in_check: {}\n",
            "checkmate: {}\n",
            "stalemate: {}\n",
            "repetition_count: {}\n",
            "threefold_claim_now: {}\n",
            "fifty_move_claim_now: {}\n",
            "fivefold_automatic: {}\n",
            "seventy_five_move_automatic: {}\n",
            "dead_position: {}\n",
            "terminal_reason: {}",
        ),
        position.fen,
        position.side_to_move.as_str(),
        position.variant,
        castling,
        en_passant,
        halfmove,
        fullmove,
        boolean(facts.in_check),
        boolean(facts.checkmate),
        boolean(facts.stalemate),
        optional_u32(facts.repetition_count),
        optional_bool(facts.threefold_claim_now),
        boolean(facts.fifty_move_claim_now),
        optional_bool(facts.fivefold_automatic),
        boolean(facts.seventy_five_move_automatic),
        optional_bool(facts.dead_position),
        facts.terminal_reason.as_deref().unwrap_or("null"),
    );
    let facts_json = serde_json::to_value(&facts).context("facts are not representable")?;
    let data = json!({
        "fen": position.fen,
        "side_to_move": position.side_to_move.as_str(),
        "variant": position.variant,
        "castling": castling,
        "en_passant": en_passant,
        "halfmove_clock": halfmove,
        "fullmove_number": fullmove,
        "history_complete": position.history_complete,
        "facts": facts_json,
    });
    Ok(rendered(SectionId::Position, body, data))
}

pub(crate) fn ascii(position: &PreparedPosition) -> Result<Rendered> {
    let board = position.state.board();
    let mut rows = Vec::with_capacity(9);
    for rank in (0..8).rev() {
        let mut row = String::new();
        for file in 0..8 {
            if file > 0 {
                row.push(' ');
            }
            let square = Square::from_coords(File::new(file), Rank::new(rank));
            let piece = board
                .piece_at(square)
                .map(|piece| piece.char())
                .unwrap_or('.');
            row.push(piece);
        }
        rows.push(format!("{} {row}", rank + 1));
    }
    rows.push("  a b c d e f g h".to_owned());
    let board_text = rows.join("\n");
    let data = json!({ "board": board_text });
    Ok(rendered(SectionId::Ascii, board_text, data))
}

pub(crate) fn legal_moves(pairs: &[(String, String)]) -> Result<Rendered> {
    let body = pairs
        .iter()
        .map(|(san, uci)| format!("{san} {uci}"))
        .collect::<Vec<_>>()
        .join("\n");
    let data: Vec<Value> = pairs
        .iter()
        .map(|(san, uci)| json!({ "san": san, "uci": uci }))
        .collect();
    Ok(rendered(SectionId::LegalMoves, body, Value::Array(data)))
}

pub(crate) fn history(position: &PreparedPosition) -> Result<Rendered> {
    let mut lines = Vec::new();
    for (index, chunk) in position.history.chunks(2).enumerate() {
        let number = index + 1;
        if chunk.len() == 2 {
            lines.push(format!("{number}. {} {}", chunk[0], chunk[1]));
        } else {
            lines.push(format!("{number}. {}", chunk[0]));
        }
    }
    let body = if lines.is_empty() {
        "(no moves)".to_owned()
    } else {
        lines.join("\n")
    };
    let data = json!({
        "plies": position.plies,
        "history_complete": position.history_complete,
        "moves": position.history,
    });
    Ok(rendered(SectionId::History, body, data))
}

pub(crate) fn annotations(annotations: &Annotations) -> Result<Rendered> {
    let items = annotations.normalized();
    let body = items
        .iter()
        .map(annotation_line)
        .collect::<Vec<_>>()
        .join("\n");
    let data = serde_json::to_value(&items).context("annotations are not representable")?;
    Ok(rendered(SectionId::Annotations, body, data))
}

fn annotation_line(item: &Annotation) -> String {
    let san = item.san.as_deref().unwrap_or("-");
    let nag = item
        .nag
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_owned());
    let comment = match &item.comment {
        Some(text) => serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_owned()),
        None => "-".to_owned(),
    };
    let arrows = if item.arrows.is_empty() {
        "-".to_owned()
    } else {
        item.arrows
            .iter()
            .map(|arrow| format!("{}->{}", arrow.from, arrow.to))
            .collect::<Vec<_>>()
            .join(",")
    };
    let circles = if item.circles.is_empty() {
        "-".to_owned()
    } else {
        item.circles.join(",")
    };
    format!(
        "ply {} san {san} nag {nag} comment {comment} arrows {arrows} circles {circles}",
        item.ply
    )
}

pub(crate) fn engine(evidence: &EngineEvidence) -> Result<Rendered> {
    let evidence = evidence.normalized();
    let mut lines = evidence.lines.clone();
    lines.sort_by(|a, b| {
        a.rank
            .cmp(&b.rank)
            .then_with(|| a.pv.cmp(&b.pv))
            .then_with(|| a.score_cp.cmp(&b.score_cp))
            .then_with(|| a.mate.cmp(&b.mate))
    });
    let mut body = String::new();
    body.push_str(&format!("name: {}\n", evidence.name));
    body.push_str(&format!("binary_sha256: {}\n", evidence.binary_sha256));
    body.push_str(&format!("depth: {}\n", evidence.depth));
    body.push_str(&format!("threads: {}", evidence.threads));
    for line in &lines {
        let score = if let Some(mate) = line.mate {
            format!("mate {mate}")
        } else if let Some(cp) = line.score_cp {
            format!("cp {cp}")
        } else {
            "score -".to_owned()
        };
        let pv = if line.pv.is_empty() {
            "-".to_owned()
        } else {
            line.pv.join(" ")
        };
        body.push_str(&format!("\n{} {score} pv {pv}", line.rank));
    }
    let data = serde_json::to_value(evidence).context("engine evidence is not representable")?;
    Ok(rendered(SectionId::Engine, body, data))
}

pub(crate) fn openings(evidence: &OpeningsEvidence) -> Result<Rendered> {
    let evidence = evidence.normalized();
    let mut rows = evidence.rows.clone();
    rows.sort_by(|a, b| {
        a.move_san
            .cmp(&b.move_san)
            .then_with(|| b.games.cmp(&a.games))
            .then_with(|| a.white_wins.cmp(&b.white_wins))
    });
    let mut body = String::new();
    body.push_str(&format!("source: {}\n", evidence.source));
    body.push_str(&format!("query: {}", evidence.query));
    for row in &rows {
        body.push_str(&format!(
            "\n{} games={} white={} draws={} black={} win_pct={}",
            row.move_san,
            row.games,
            row.white_wins,
            row.draws,
            row.black_wins,
            percentage(row.win_pct),
        ));
    }
    let data = serde_json::to_value(evidence).context("opening evidence is not representable")?;
    Ok(rendered(SectionId::Openings, body, data))
}

pub(crate) fn database(evidence: &DatabaseEvidence) -> Result<Rendered> {
    let evidence = evidence.normalized();
    let mut sample = evidence.sample.clone();
    sample.sort_by(|a, b| {
        a.id.cmp(&b.id)
            .then_with(|| a.white.cmp(&b.white))
            .then_with(|| a.black.cmp(&b.black))
    });
    let mut body = String::new();
    body.push_str(&format!("query: {}\n", evidence.query));
    body.push_str(&format!("total_games: {}\n", evidence.total_games));
    body.push_str(&format!("white_wins: {}\n", evidence.white_wins));
    body.push_str(&format!("draws: {}\n", evidence.draws));
    body.push_str(&format!("black_wins: {}", evidence.black_wins));
    for game in &sample {
        body.push_str(&format!(
            "\n{} | {} | {} | {} | {}",
            game.id,
            game.white,
            game.black,
            game.result,
            game.date.as_deref().unwrap_or("-"),
        ));
    }
    let data = serde_json::to_value(evidence).context("database evidence is not representable")?;
    Ok(rendered(SectionId::Database, body, data))
}

pub(crate) fn notes(notes: &str) -> Result<Rendered> {
    let text = crate::canonical::normalize_text(notes);
    let data = json!({ "text": text });
    Ok(rendered(SectionId::Notes, text, data))
}

pub(crate) fn boards(id: SectionId, summaries: &[BoardSummary]) -> Result<Rendered> {
    let mut rows: Vec<&BoardSummary> = summaries.iter().collect();
    rows.sort_by(|a, b| {
        a.id.cmp(&b.id)
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.fen.cmp(&b.fen))
            .then_with(|| a.ply.cmp(&b.ply))
            .then_with(|| a.note.cmp(&b.note))
    });
    let body = rows
        .iter()
        .map(|summary| {
            let mut line = format!("- {} | {} | {}", summary.id, summary.title, summary.fen);
            if let Some(ply) = summary.ply {
                line.push_str(&format!(" | ply {ply}"));
            }
            if let Some(note) = &summary.note {
                line.push_str(&format!(" | {note}"));
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n");
    let data = serde_json::to_value(&rows).context("board summaries are not representable")?;
    Ok(rendered(id, body, data))
}

pub(crate) fn turns(turns: &[Turn]) -> Result<Rendered> {
    let body = turns
        .iter()
        .map(|turn| {
            format!(
                "{}: {}",
                turn.role.as_str(),
                crate::canonical::normalize_text(&turn.content)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let data = serde_json::to_value(turns).context("turns are not representable")?;
    Ok(rendered(SectionId::Turns, body, data))
}

/// Concatenate included sections in their supplied (canonical) order.
pub(crate) fn prompt(sections: &[crate::section::Section]) -> String {
    let text = sections
        .iter()
        .filter(|section| section.included)
        .map(|section| section.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    crate::canonical::normalize_text(&text)
}

fn boolean(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn optional_bool(value: Option<bool>) -> &'static str {
    value.map(boolean).unwrap_or("null")
}

fn optional_u32(value: Option<u32>) -> String {
    value
        .map(|number| number.to_string())
        .unwrap_or_else(|| "null".to_owned())
}

fn percentage(value: Option<f64>) -> String {
    value
        .map(|number| format!("{number:.2}"))
        .unwrap_or_else(|| "-".to_owned())
}
