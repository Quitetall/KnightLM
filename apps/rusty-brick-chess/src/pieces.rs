//! Custom SVG piece sets uploaded by the operator.
//!
//! A set is a directory of twelve SVGs (`K Q R B N P k q r b n p`) under
//! `data_dir/pieces/<name>`. They are served read-only and referenced from the
//! board as `<img>` (image contexts never execute SVG scripts).

use anyhow::{Context, Result, bail, ensure};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const CODES: [&str; 12] = ["K", "Q", "R", "B", "N", "P", "k", "q", "r", "b", "n", "p"];
const MAX_SVG: usize = 512 * 1024;

#[derive(Debug, Clone, serde::Serialize)]
pub struct PieceSet {
    pub name: String,
    pub pieces: usize,
}

pub fn root(data_dir: &Path) -> PathBuf {
    data_dir.join("pieces")
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

pub fn list(data_dir: &Path) -> Result<Vec<PieceSet>> {
    let dir = root(data_dir);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut sets = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !valid_name(&name) {
            continue;
        }
        let pieces = CODES
            .iter()
            .filter(|code| entry.path().join(format!("{code}.svg")).exists())
            .count();
        sets.push(PieceSet { name, pieces });
    }
    sets.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(sets)
}

pub fn save(data_dir: &Path, name: &str, files: &BTreeMap<String, String>) -> Result<usize> {
    ensure!(valid_name(name), "piece set names use a-z, 0-9, - and _");
    for code in files.keys() {
        ensure!(CODES.contains(&code.as_str()), "unknown piece code: {code}");
    }
    for code in CODES {
        let Some(svg) = files.get(code) else {
            bail!("missing piece {code}");
        };
        ensure!(svg.len() <= MAX_SVG, "piece {code} is too large");
        let trimmed = svg.trim_start();
        ensure!(
            trimmed.starts_with("<svg") || trimmed.starts_with("<?xml"),
            "piece {code} is not an SVG"
        );
    }
    let dir = root(data_dir).join(name);
    std::fs::create_dir_all(&dir)?;
    for (code, svg) in files {
        std::fs::write(dir.join(format!("{code}.svg")), svg.as_bytes())?;
    }
    Ok(CODES.len())
}

pub fn read(data_dir: &Path, name: &str, code: &str) -> Result<Vec<u8>> {
    ensure!(valid_name(name), "invalid piece set");
    ensure!(CODES.contains(&code), "invalid piece code");
    let path = root(data_dir).join(name).join(format!("{code}.svg"));
    std::fs::read(&path).with_context(|| format!("read {}", path.display()))
}

pub fn delete(data_dir: &Path, name: &str) -> Result<()> {
    ensure!(valid_name(name), "invalid piece set");
    let dir = root(data_dir).join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    Ok(())
}
