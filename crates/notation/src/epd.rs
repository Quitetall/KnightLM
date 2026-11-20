//! Extended Position Description: four FEN fields followed by `opcode
//! operand...;` operations (e.g. `bm Qg6; id "WAC.001";`).
//!
//! shakmaty's `Epd` models only the four position fields, so the operation
//! grammar lives here. Parsing is strict: every operation must end with `;`,
//! opcodes are 1-15 characters starting with a letter, an opcode may appear
//! once, quotes must balance, and a fifth numeric token (a six-field FEN) is
//! refused rather than guessed at.

use anyhow::{Context, Result, bail};
use shakmaty::{CastlingMode, Chess, Position};

use crate::fen::{CastlingStyle, check_core_fields, write_fen4};
use crate::san::{parse_san_mode, to_san};
use knightlm_core::state::parse_chess;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    pub opcode: String,
    /// Operands verbatim; string operands keep their surrounding quotes.
    pub operands: Vec<String>,
}

impl Operation {
    pub fn new<S: Into<String>>(
        opcode: &str,
        operands: impl IntoIterator<Item = S>,
    ) -> Result<Self> {
        let op = Operation {
            opcode: opcode.to_owned(),
            operands: operands.into_iter().map(Into::into).collect(),
        };
        op.validate()?;
        Ok(op)
    }

    fn validate(&self) -> Result<()> {
        let o = &self.opcode;
        let valid = !o.is_empty()
            && o.len() <= 15
            && o.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && o.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid {
            bail!(
                "EPD opcode {o:?} must be 1-15 characters, start with a letter, and use [A-Za-z0-9_]"
            );
        }
        for operand in &self.operands {
            if let Some(inner) = operand.strip_prefix('"') {
                let Some(inner) = inner.strip_suffix('"') else {
                    bail!("EPD operand {operand:?} has an unterminated string");
                };
                if inner.contains('"') {
                    bail!("EPD string operand {operand:?} contains a quote");
                }
            } else if operand.is_empty()
                || operand.contains(|c: char| c.is_whitespace() || c == ';' || c == '"')
            {
                bail!("EPD operand {operand:?} is not a bare token or a quoted string");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Epd {
    pub position: Chess,
    pub mode: CastlingMode,
    pub operations: Vec<Operation>,
}

fn split_operations(rest: &str) -> Result<Vec<String>> {
    let mut ops = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    for c in rest.chars() {
        match c {
            '"' => {
                in_quote = !in_quote;
                cur.push(c);
            }
            ';' if !in_quote => {
                ops.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    if in_quote {
        bail!("EPD has an unterminated quoted string");
    }
    if !cur.trim().is_empty() {
        bail!(
            "EPD operation {:?} is missing its terminating ';'",
            cur.trim()
        );
    }
    Ok(ops)
}

fn tokenize_operands(s: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '"' {
            let mut tok = String::from('"');
            chars.next();
            loop {
                match chars.next() {
                    Some('"') => {
                        tok.push('"');
                        break;
                    }
                    Some(ch) => tok.push(ch),
                    None => bail!("EPD has an unterminated quoted string"),
                }
            }
            if chars.peek().is_some_and(|ch| !ch.is_whitespace()) {
                bail!("EPD string operand {tok} runs into the next token");
            }
            out.push(tok);
        } else {
            let mut tok = String::new();
            while let Some(&ch) = chars.peek() {
                if ch.is_whitespace() {
                    break;
                }
                if ch == '"' {
                    bail!("EPD operand {tok:?} contains a stray quote");
                }
                tok.push(ch);
                chars.next();
            }
            out.push(tok);
        }
    }
    Ok(out)
}

/// Parse one EPD record (a single line).
pub fn parse_epd(line: &str) -> Result<Epd> {
    let line = line.trim();
    if line.contains('\n') {
        bail!("EPD is a single line; use parse_epd_suite for several records");
    }
    // Take the first four whitespace-delimited fields; the rest is operations.
    let mut rest = line;
    let mut fields = Vec::new();
    for _ in 0..4 {
        rest = rest.trim_start();
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        if end == 0 {
            bail!("EPD needs 4 position fields, found {}", fields.len());
        }
        fields.push(&rest[..end]);
        rest = &rest[end..];
    }
    if fields.iter().any(|f| f.contains('_')) {
        bail!("EPD fields must be separated by spaces, not underscores");
    }
    check_core_fields(&fields)?;
    let (position, mode) = parse_chess(&fields.join(" "))?;

    if rest.trim_start().starts_with(|c: char| c.is_ascii_digit()) {
        bail!(
            "EPD has numeric fields after the position; that looks like a six-field FEN (FEN clock fields), not EPD"
        );
    }
    let mut operations = Vec::new();
    for raw in split_operations(rest)? {
        let raw = raw.trim();
        if raw.is_empty() {
            bail!("EPD has an empty operation");
        }
        let end = raw.find(char::is_whitespace).unwrap_or(raw.len());
        let opcode = &raw[..end];
        if opcode.starts_with(|c: char| c.is_ascii_digit()) {
            bail!("EPD operation {opcode:?} looks like FEN clock fields; this is not EPD");
        }
        let operands = tokenize_operands(&raw[end..])?;
        let op = Operation::new(opcode, operands)?;
        if operations.iter().any(|o: &Operation| o.opcode == op.opcode) {
            bail!("EPD opcode {:?} appears more than once", op.opcode);
        }
        operations.push(op);
    }
    Ok(Epd {
        position,
        mode,
        operations,
    })
}

/// Parse a suite: one record per non-blank line. Errors name the line.
pub fn parse_epd_suite(text: &str) -> Result<Vec<Epd>> {
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(i, l)| parse_epd(l).with_context(|| format!("EPD line {}", i + 1)))
        .collect()
}

impl Epd {
    pub fn from_position(position: Chess, mode: CastlingMode) -> Self {
        Epd {
            position,
            mode,
            operations: Vec::new(),
        }
    }

    pub fn op(&self, opcode: &str) -> Option<&[String]> {
        self.operations
            .iter()
            .find(|o| o.opcode == opcode)
            .map(|o| o.operands.as_slice())
    }

    /// Add or replace an operation, keeping its position if it exists.
    pub fn set_op(&mut self, op: Operation) {
        match self.operations.iter_mut().find(|o| o.opcode == op.opcode) {
            Some(slot) => *slot = op,
            None => self.operations.push(op),
        }
    }

    /// The `id` operand with its quotes removed.
    pub fn id(&self) -> Option<String> {
        let v = self.op("id")?;
        let first = v.first()?;
        Some(first.trim_matches('"').to_owned())
    }

    fn moves_for(&self, opcode: &str) -> Result<Vec<String>> {
        self.op(opcode)
            .unwrap_or_default()
            .iter()
            .map(|san| parse_san_mode(&self.position, san, self.mode))
            .collect::<Result<Vec<_>>>()
            .with_context(|| format!("EPD {opcode} operand"))
    }

    /// `bm` (best moves) as UCI, validated legal in the position.
    pub fn best_moves(&self) -> Result<Vec<String>> {
        self.moves_for("bm")
    }

    /// `am` (avoid moves) as UCI, validated legal in the position.
    pub fn avoid_moves(&self) -> Result<Vec<String>> {
        self.moves_for("am")
    }

    /// Set `bm` from UCI moves, rendered as SAN.
    pub fn set_best_moves(&mut self, uci: &[&str]) -> Result<()> {
        let sans = uci
            .iter()
            .map(|m| to_san(&self.position, m))
            .collect::<Result<Vec<_>>>()?;
        self.set_op(Operation::new("bm", sans)?);
        Ok(())
    }
}

/// Write one EPD record. Re-validates every operation so a hand-built value
/// cannot emit text the parser would refuse.
pub fn write_epd(epd: &Epd, style: CastlingStyle) -> Result<String> {
    let mut out = write_fen4(&epd.position, style);
    let mut seen: Vec<&str> = Vec::new();
    for op in &epd.operations {
        op.validate()?;
        if seen.contains(&op.opcode.as_str()) {
            bail!("EPD opcode {:?} appears more than once", op.opcode);
        }
        seen.push(&op.opcode);
        out.push(' ');
        out.push_str(&op.opcode);
        for operand in &op.operands {
            out.push(' ');
            out.push_str(operand);
        }
        out.push(';');
    }
    Ok(out)
}

/// Export any position as a bare four-field EPD (no operations).
pub fn export_epd<P: Position>(pos: &P, style: CastlingStyle) -> String {
    write_fen4(pos, style)
}
