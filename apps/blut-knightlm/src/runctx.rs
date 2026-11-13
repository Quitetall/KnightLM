//! Locations shared by the stages: the Python payload, the KnightLM checkout,
//! the run ledger, and the run-state marker.

use std::io::Write;
use std::path::{Path, PathBuf};

use blut::framework::error::StageError;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// Directory holding the `knightlm_blut` Python package.
pub fn payload_dir() -> PathBuf {
    std::env::var_os("KNIGHTLM_BLUT_PAYLOAD").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("python"),
        PathBuf::from,
    )
}

/// The KnightLM checkout (`model.*` lives here): `$KNIGHTLM_REPO`, else the
/// repository this crate sits in (`apps/blut-knightlm/../..`).
pub fn repo_root() -> PathBuf {
    let raw = std::env::var_os("KNIGHTLM_REPO").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
        PathBuf::from,
    );
    raw.canonicalize().unwrap_or(raw)
}

pub fn pythonpath(repo: &Path) -> String {
    format!("{}:{}", repo.display(), payload_dir().display())
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    std::io::copy(&mut f, &mut h)?;
    Ok(format!("{:x}", h.finalize()))
}

pub fn io_err(path: &Path, source: std::io::Error) -> StageError {
    StageError::Io {
        path: path.to_path_buf(),
        source,
    }
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Append one record to `<run_dir>/ledger.jsonl`. The ledger is append-only: one
/// JSON object per line, never rewritten, so a crashed run leaves a readable
/// history and a resumed run continues it.
pub fn ledger_append(
    run_dir: &Path,
    stage: &str,
    status: &str,
    detail: Value,
) -> Result<(), StageError> {
    std::fs::create_dir_all(run_dir).map_err(|e| io_err(run_dir, e))?;
    let path = run_dir.join("ledger.jsonl");
    let record = json!({
        "schema": "knightlm.run-ledger/v1",
        "unix": now_unix(),
        "stage": stage,
        "status": status,
        "detail": detail,
    });
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| io_err(&path, e))?;
    writeln!(f, "{record}").map_err(|e| io_err(&path, e))?;
    f.sync_all().map_err(|e| io_err(&path, e))
}

/// Run a Python module with the KnightLM repo and payload on `PYTHONPATH`.
/// Returns stdout; a non-zero exit becomes `BadInput(stderr)`.
pub async fn run_python(
    python: &Path,
    module: &str,
    args: &[String],
) -> Result<String, StageError> {
    let repo = repo_root();
    let out = tokio::process::Command::new(python)
        .arg("-m")
        .arg(module)
        .args(args)
        .current_dir(&repo)
        .env("PYTHONPATH", pythonpath(&repo))
        .output()
        .await
        .map_err(|e| io_err(python, e))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail: String = err
            .chars()
            .rev()
            .take(2000)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        return Err(StageError::BadInput(format!(
            "{module} exited with {}: {}",
            out.status,
            tail.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn last_json_line(stdout: &str) -> Result<Value, StageError> {
    let line = stdout
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .ok_or_else(|| StageError::BadInput("python printed nothing".into()))?;
    serde_json::from_str(line)
        .map_err(|e| StageError::BadInput(format!("python result is not JSON ({e}): {line}")))
}

/// Short revision of the blut-cookbook-standard checkout this crate is pinned
/// to (the directory name cargo uses under `~/.cargo/git/checkouts`). A test
/// keeps it equal to the `rev` in Cargo.toml.
pub const STANDARD_REV_SHORT: &str = "125482d";

/// Where the standard cookbook's `hf_trainer_runner.py` (and its sibling
/// modules) live when this binary is built from git: the cargo checkout of the
/// pinned rev. `None` when it cannot be found.
pub fn locate_hf_runner() -> Option<PathBuf> {
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cargo")))?;
    let checkouts = cargo_home.join("git/checkouts");
    for repo in std::fs::read_dir(checkouts).ok()?.flatten() {
        if !repo
            .file_name()
            .to_string_lossy()
            .starts_with("blut-cookbooks-")
        {
            continue;
        }
        let candidate = repo
            .path()
            .join(STANDARD_REV_SHORT)
            .join("src/backends/hf_trainer/python/hf_trainer_runner.py");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Point the standard trainer at its wrapper script. The engine looks for it
/// next to the binary or in a development tree, neither of which holds for a
/// binary built from a git dependency. An existing `BLUT_HF_TRAINER_RUNNER`
/// wins. Call before any thread starts.
pub fn ensure_hf_runner_env() {
    if std::env::var_os("BLUT_HF_TRAINER_RUNNER").is_some() {
        return;
    }
    if let Some(path) = locate_hf_runner() {
        // SAFETY: called from `main` before the async runtime spawns threads.
        unsafe { std::env::set_var("BLUT_HF_TRAINER_RUNNER", path) };
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn standard_rev_constant_matches_cargo_toml() {
        let toml =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
        let rev = toml
            .lines()
            .find(|l| l.starts_with("blut-backends"))
            .and_then(|l| l.split("rev = \"").nth(1))
            .and_then(|r| r.split('"').next())
            .unwrap();
        assert!(rev.starts_with(super::STANDARD_REV_SHORT), "{rev}");
    }
}
