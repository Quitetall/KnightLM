//! The managed KnightLM runtime: get the model, check it, run it, register it.
//!
//! One click should turn "no model" into "a model you can play against":
//!
//! 1. **install** downloads the pinned base weights and the adapter (only from the URLs in
//!    `knightlm_manifest.json`), verifies every sha256, and moves the verified files into
//!    `<RBC_DATA>/knightlm/` atomically. A partial or corrupt download is never visible as
//!    installed. It also builds the venv: from the app's private Python (downloaded on demand,
//!    pinned like everything else), or a suitable system Python; see [`crate::py_runtime`].
//! 2. **start** runs `python -m model.serve.server` on a free loopback port, waits for
//!    `/v1/models`, and registers (or updates) the `knightlm` provider row to that URL.
//! 3. **stop**, or the app exiting, ends the child. The child also watches its stdin, which
//!    the app holds: if the app dies for any reason the pipe closes and the child exits, so
//!    no server is left behind.
//!
//! `fake` mode serves `model.serve.fake_server` (no torch, no weights) so CI can run the
//! whole flow offline. It is configured by `RBC_KNIGHTLM_FAKE=1`, never by a request.
//!
//! Dev-only overrides (not for users): `RBC_KNIGHTLM_MANIFEST` (a manifest file, `file://`
//! URLs allowed), `RBC_KNIGHTLM_DEV_BASE_DIR` (copy the base files from a local directory
//! instead of downloading; they are still hash-checked against the pinned manifest).
//! `RBC_KNIGHTLM_PYTHON` (a Python to use as is, never downloaded over) is a user setting.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail, ensure};
use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::api::{ApiError, AppState};
use crate::py_runtime::{self, Candidate, Kind};

const EMBEDDED_MANIFEST: &str = include_str!("knightlm_manifest.json");
/// The only places the real runtime downloads from.
const PINNED_BASE_PREFIX: &str = "https://huggingface.co/HuggingFaceTB/SmolVLM-500M-Instruct/resolve/a7da5b986cb59b408707209984f360a5f4ad7e47/";
const PINNED_ADAPTER_URL: &str = "https://github.com/Quitetall/KnightLM/releases/download/model-run1-short-adapter-v1/knightlm-run1-short-adapter.tar.gz";
/// The only place the private Python is downloaded from.
const PINNED_PYTHON_PREFIX: &str =
    "https://github.com/astral-sh/python-build-standalone/releases/download/20261123/";
const MAX_ADAPTER_FILE: u64 = 64 * 1024 * 1024;
const PROVIDER_LABEL: &str = "KnightLM server";

// --- the manifest ------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct FileSpec {
    pub name: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BaseSet {
    pub repo: String,
    pub revision: String,
    pub license: String,
    pub url_prefix: String,
    pub files: Vec<FileSpec>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdapterSet {
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub dir: String,
    pub files: Vec<FileSpec>,
}

/// One pinned python-build-standalone `install_only` tarball for an OS and CPU
/// (`target` is `linux-x86_64`, `windows-x86_64`, `macos-aarch64`, `macos-x86_64`).
#[derive(Debug, Clone, Deserialize)]
pub struct PythonBuild {
    pub target: String,
    pub version: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    pub base: BaseSet,
    pub adapter: AdapterSet,
    #[serde(default)]
    pub python: Vec<PythonBuild>,
}

impl Manifest {
    /// The manifest compiled into this build.
    pub fn pinned() -> Manifest {
        serde_json::from_str(EMBEDDED_MANIFEST).expect("embedded KnightLM manifest parses")
    }

    /// Whether every URL is one of the pinned sources. The real runtime refuses anything else.
    pub fn check_pinned(&self) -> Result<()> {
        ensure!(self.schema == 1, "unsupported manifest schema");
        ensure!(
            self.base.url_prefix == PINNED_BASE_PREFIX,
            "the base weights are not at the pinned Hugging Face revision"
        );
        ensure!(
            self.adapter.url == PINNED_ADAPTER_URL,
            "the adapter is not at the pinned release URL"
        );
        for build in &self.python {
            ensure!(
                build.url.starts_with(PINNED_PYTHON_PREFIX)
                    && build.url.ends_with("-install_only.tar.gz")
                    && !build.url[PINNED_PYTHON_PREFIX.len()..].contains('/'),
                "the Python download for {} is not at the pinned release",
                build.target
            );
        }
        self.check_shape()
    }

    /// The private Python for this OS and CPU, if one is pinned.
    pub fn python_for(&self, target: Option<&str>) -> Option<&PythonBuild> {
        let target = target?;
        self.python.iter().find(|build| build.target == target)
    }

    /// Names are plain file names and hashes are 64 hex digits.
    pub fn check_shape(&self) -> Result<()> {
        let hex = |s: &str| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit());
        for file in self.base.files.iter().chain(&self.adapter.files) {
            ensure!(plain_name(&file.name), "unsafe file name {:?}", file.name);
            ensure!(hex(&file.sha256), "bad sha256 for {}", file.name);
        }
        ensure!(hex(&self.adapter.sha256), "bad adapter sha256");
        for build in &self.python {
            ensure!(hex(&build.sha256), "bad sha256 for the {} Python", build.target);
            ensure!(build.size > 0, "bad size for the {} Python", build.target);
            ensure!(plain_name(&build.target), "unsafe Python target");
        }
        ensure!(plain_name(&self.adapter.dir), "unsafe adapter directory");
        Ok(())
    }

    fn total_bytes(&self) -> u64 {
        self.base.files.iter().map(|f| f.size).sum::<u64>() + self.adapter.size
    }
}

fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', '\0', ':'])
        && !name.starts_with('.')
}

// --- hashing and downloading ---------------------------------------------------------------

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Check a file against its pinned size and hash.
pub fn verify_file(path: &Path, spec_size: u64, spec_sha: &str) -> Result<()> {
    let size = std::fs::metadata(path)
        .with_context(|| format!("missing {}", path.display()))?
        .len();
    ensure!(
        size == spec_size,
        "{} has {size} bytes, expected {spec_size}",
        path.display()
    );
    let digest = sha256_file(path)?;
    ensure!(
        digest == spec_sha,
        "{} does not match its pinned sha256",
        path.display()
    );
    Ok(())
}

/// Fetch `url` into `part`, resuming at the bytes already there when the server allows it.
/// `progress` is told the number of bytes of this file now on disk.
fn fetch(url: &str, part: &Path, expected: u64, progress: &mut dyn FnMut(u64)) -> Result<()> {
    if let Some(path) = url.strip_prefix("file://") {
        // Dev and tests only (`check_pinned` refuses it for the real runtime).
        let mut source = std::fs::File::open(path).with_context(|| format!("open {path}"))?;
        let mut out = std::fs::File::create(part)?;
        copy_progress(&mut source, &mut out, 0, progress)?;
        return Ok(());
    }
    let have = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);
    let have = if have > expected { 0 } else { have };
    let agent = ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_recv_response(Some(Duration::from_secs(60)))
            .build(),
    );
    let mut request = agent.get(url);
    if have > 0 {
        request = request.header("Range", format!("bytes={have}-"));
    }
    let mut response = request
        .call()
        .map_err(|e| anyhow!("download failed: {e}"))?;
    let status = response.status().as_u16();
    let (mut out, start) = match status {
        206 => (std::fs::OpenOptions::new().append(true).open(part)?, have),
        200 => (std::fs::File::create(part)?, 0),
        416 if have == expected => return Ok(()),
        other => bail!("download failed: the server answered HTTP {other}"),
    };
    let mut reader = response.body_mut().as_reader();
    copy_progress(&mut reader, &mut out, start, progress)?;
    Ok(())
}

fn copy_progress(
    from: &mut dyn Read,
    to: &mut dyn Write,
    start: u64,
    progress: &mut dyn FnMut(u64),
) -> Result<()> {
    let mut buffer = vec![0u8; 256 * 1024];
    let mut done = start;
    loop {
        let n = from.read(&mut buffer).context("download interrupted")?;
        if n == 0 {
            break;
        }
        to.write_all(&buffer[..n])?;
        done += n as u64;
        progress(done);
    }
    to.flush()?;
    Ok(())
}

// --- the adapter tarball ---------------------------------------------------------------------

/// Unpack the adapter tarball into `dest` (which must be empty or new). Every entry must be a
/// plain file or directory inside `<dir>/`, with a name the manifest lists; anything else (a
/// `..`, an absolute path, a link, an unlisted file, an oversized file) refuses the whole
/// archive. Files are written by hand, never with `unpack`, so no path from the archive is
/// ever joined onto the disk without being checked first.
pub fn extract_adapter(tgz: &Path, dest: &Path, set: &AdapterSet) -> Result<()> {
    let file = std::fs::File::open(tgz)?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
    std::fs::create_dir_all(dest)?;
    let mut seen = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let kind = entry.header().entry_type();
        let raw = entry.path()?.into_owned();
        let mut parts = Vec::new();
        for component in raw.components() {
            match component {
                std::path::Component::Normal(part) => parts.push(
                    part.to_str()
                        .context("non-UTF-8 name in the adapter archive")?
                        .to_owned(),
                ),
                std::path::Component::CurDir => {}
                _ => bail!("the adapter archive has an unsafe path: {}", raw.display()),
            }
        }
        if parts.is_empty() {
            continue;
        }
        ensure!(
            parts[0] == set.dir,
            "the adapter archive has an entry outside {}/: {}",
            set.dir,
            raw.display()
        );
        if kind.is_dir() {
            ensure!(
                parts.len() == 1,
                "unexpected directory in the adapter archive"
            );
            continue;
        }
        ensure!(
            kind.is_file() && parts.len() == 2,
            "the adapter archive has an entry that is not a plain file: {}",
            raw.display()
        );
        let name = &parts[1];
        ensure!(
            set.files.iter().any(|f| &f.name == name),
            "the adapter archive has an unlisted file: {name}"
        );
        ensure!(
            entry.size() <= MAX_ADAPTER_FILE,
            "{name} is larger than an adapter file may be"
        );
        let mut out = std::fs::File::create(dest.join(name))?;
        let copied = std::io::copy(&mut (&mut entry).take(MAX_ADAPTER_FILE + 1), &mut out)?;
        ensure!(
            copied <= MAX_ADAPTER_FILE,
            "{name} is larger than an adapter file may be"
        );
        seen.push(name.clone());
    }
    for file in &set.files {
        ensure!(
            seen.contains(&file.name),
            "the adapter archive is missing {}",
            file.name
        );
        verify_file(&dest.join(&file.name), file.size, &file.sha256)?;
    }
    Ok(())
}

// --- configuration, status ---------------------------------------------------------------

/// How a runtime is set up. `from_env` is what the app uses; tests build their own.
#[derive(Debug, Clone)]
pub struct Config {
    pub manifest: Manifest,
    /// The manifest is not the pinned one (dev or test): URLs are not restricted.
    pub unpinned: bool,
    /// Serve `model.serve.fake_server`: no weights, no torch.
    pub fake: bool,
    /// Copy base files from here instead of downloading them (dev only).
    pub dev_base_dir: Option<PathBuf>,
    /// A python to use as is (`RBC_KNIGHTLM_PYTHON`).
    pub python: Option<PathBuf>,
    /// Where `model/` (the serving code) lives. Resolved when starting if absent.
    pub code_dir: Option<PathBuf>,
}

impl Config {
    pub fn from_env() -> Result<Config> {
        let (manifest, unpinned) = match std::env::var_os("RBC_KNIGHTLM_MANIFEST") {
            Some(path) => (
                serde_json::from_slice(
                    &std::fs::read(&path).context("read RBC_KNIGHTLM_MANIFEST")?,
                )
                .context("parse RBC_KNIGHTLM_MANIFEST")?,
                true,
            ),
            None => (Manifest::pinned(), false),
        };
        manifest.check_shape()?;
        if !unpinned {
            manifest.check_pinned()?;
        }
        Ok(Config {
            manifest,
            unpinned,
            fake: std::env::var("RBC_KNIGHTLM_FAKE").is_ok_and(|v| v == "1"),
            dev_base_dir: std::env::var_os("RBC_KNIGHTLM_DEV_BASE_DIR").map(PathBuf::from),
            python: std::env::var_os("RBC_KNIGHTLM_PYTHON").map(PathBuf::from),
            code_dir: std::env::var_os("RBC_MODEL_CODE").map(PathBuf::from),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Phase {
    NotInstalled,
    Downloading {
        bytes: u64,
        total: u64,
        file: String,
    },
    Verifying,
    SettingUp {
        step: String,
    },
    Ready,
    Starting,
    Running {
        url: String,
    },
    Error {
        code: String,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct PythonInfo {
    pub found: bool,
    pub path: Option<String>,
    /// Whether the serving dependencies import (torch etc., or fastapi in fake mode).
    pub ready: bool,
    pub cuda: bool,
}

struct Inner {
    phase: Phase,
    child: Option<RunningChild>,
    installing: bool,
    python: Option<(Instant, PythonInfo)>,
}

struct RunningChild {
    child: Child,
    /// Held open: dropping it closes the child's stdin, which makes the child exit.
    _stdin: Option<std::process::ChildStdin>,
    url: String,
}

pub struct Runtime {
    data_dir: PathBuf,
    config: Config,
    inner: Mutex<Inner>,
}

type Registry = Mutex<HashMap<PathBuf, Arc<Runtime>>>;

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}

/// The runtime for an app data directory. Created from the environment on first use.
pub fn runtime_for(data_dir: &Path) -> Result<Arc<Runtime>> {
    let mut map = registry().lock().expect("runtime registry");
    if let Some(existing) = map.get(data_dir) {
        return Ok(existing.clone());
    }
    let runtime = Arc::new(Runtime::new(data_dir, Config::from_env()?));
    map.insert(data_dir.to_path_buf(), runtime.clone());
    Ok(runtime)
}

/// Register a runtime with an explicit configuration (tests).
#[doc(hidden)]
pub fn install_runtime(data_dir: &Path, config: Config) -> Arc<Runtime> {
    let runtime = Arc::new(Runtime::new(data_dir, config));
    registry()
        .lock()
        .expect("runtime registry")
        .insert(data_dir.to_path_buf(), runtime.clone());
    runtime
}

/// Stop every managed server (the app is exiting).
pub fn shutdown_all() {
    let runtimes: Vec<_> = registry()
        .lock()
        .expect("runtime registry")
        .values()
        .cloned()
        .collect();
    for runtime in runtimes {
        runtime.stop();
    }
}

fn err(code: &str, message: impl Into<String>) -> Phase {
    Phase::Error {
        code: code.to_string(),
        message: message.into(),
    }
}

/// An error with a stable code, carried through `anyhow`.
#[derive(Debug)]
struct Coded(String, String);
impl std::fmt::Display for Coded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.1)
    }
}
impl std::error::Error for Coded {}
fn coded(code: &str, message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(Coded(code.to_string(), message.into()))
}

impl Runtime {
    fn new(data_dir: &Path, config: Config) -> Runtime {
        Runtime {
            data_dir: data_dir.to_path_buf(),
            config,
            inner: Mutex::new(Inner {
                phase: Phase::NotInstalled,
                child: None,
                installing: false,
                python: None,
            }),
        }
    }

    fn root(&self) -> PathBuf {
        self.data_dir.join("knightlm")
    }
    fn base_dir(&self) -> PathBuf {
        self.root().join("base")
    }
    fn adapter_dir(&self) -> PathBuf {
        self.root()
            .join("adapter")
            .join(&self.config.manifest.adapter.dir)
    }
    fn marker(&self) -> PathBuf {
        self.root().join("installed.json")
    }
    fn venv_dir(&self) -> PathBuf {
        self.root().join("venv")
    }
    /// The private Python: an unpacked python-build-standalone tree.
    fn private_python_dir(&self) -> PathBuf {
        self.root().join("python")
    }

    /// Whether the verified files are in place. The marker is written last, after every hash
    /// has matched and both directories are renamed in; a partial download has no marker.
    pub fn files_installed(&self) -> bool {
        if self.config.fake {
            return self.marker().is_file();
        }
        self.marker().is_file()
            && self.config.manifest.base.files.iter().all(|f| {
                std::fs::metadata(self.base_dir().join(&f.name)).is_ok_and(|m| m.len() == f.size)
            })
            && self.config.manifest.adapter.files.iter().all(|f| {
                std::fs::metadata(self.adapter_dir().join(&f.name)).is_ok_and(|m| m.len() == f.size)
            })
    }

    fn child_alive(inner: &mut Inner) -> bool {
        match inner.child.as_mut() {
            Some(running) => match running.child.try_wait() {
                Ok(None) => true,
                _ => {
                    inner.child = None;
                    false
                }
            },
            None => false,
        }
    }

    /// The state, derived from the live process and the disk when nothing is in flight.
    pub fn phase(&self) -> Phase {
        let mut inner = self.inner.lock().expect("runtime lock");
        let alive = Self::child_alive(&mut inner);
        if !alive && matches!(inner.phase, Phase::Running { .. }) {
            inner.phase = err(
                "server_exited",
                "The model server stopped unexpectedly. Press Play to start it again.",
            );
        }
        if inner.installing || matches!(inner.phase, Phase::Starting | Phase::Error { .. }) {
            return inner.phase.clone();
        }
        if alive {
            let url = inner
                .child
                .as_ref()
                .map(|c| c.url.clone())
                .unwrap_or_default();
            return Phase::Running { url };
        }
        if self.files_installed() {
            Phase::Ready
        } else {
            Phase::NotInstalled
        }
    }

    fn set_phase(&self, phase: Phase) {
        self.inner.lock().expect("runtime lock").phase = phase;
    }

    // --- python ---

    fn windows() -> bool {
        cfg!(windows)
    }

    /// The private Python's interpreter, whether or not it is there yet.
    fn private_python_exe(&self) -> PathBuf {
        py_runtime::python_exe_in(&self.private_python_dir(), Self::windows())
    }

    /// Every Python worth asking, best first: the override, our venv, the private Python, then
    /// the system's (Windows: the `py` launcher's answers before bare names).
    fn candidates(&self) -> Vec<Candidate> {
        let mut list = Vec::new();
        if let Some(python) = &self.config.python {
            list.push(Candidate {
                path: python.clone(),
                kind: Kind::Override,
            });
        }
        let venv = py_runtime::venv_python(&self.venv_dir(), Self::windows());
        if venv.is_file() {
            list.push(Candidate {
                path: venv,
                kind: Kind::Venv,
            });
        }
        let private = self.private_python_exe();
        if private.is_file() {
            list.push(Candidate {
                path: private,
                kind: Kind::Private,
            });
        }
        let system = py_runtime::windows_launcher_paths()
            .into_iter()
            .chain(py_runtime::system_names());
        list.extend(system.map(|path| Candidate {
            path,
            kind: Kind::System,
        }));
        list
    }

    fn imports(&self) -> &'static str {
        if self.config.fake {
            "import fastapi, uvicorn"
        } else {
            "import torch, transformers, peft, PIL, numpy, fastapi, uvicorn"
        }
    }

    /// Probe the Python the runtime would use, cached for a short while.
    pub fn python_info(&self, fresh: bool) -> PythonInfo {
        if !fresh {
            let inner = self.inner.lock().expect("runtime lock");
            if let Some((when, info)) = &inner.python
                && when.elapsed() < Duration::from_secs(60)
            {
                return info.clone();
            }
        }
        let mut found: Option<PathBuf> = None;
        let mut info = PythonInfo {
            found: false,
            path: None,
            ready: false,
            cuda: false,
        };
        for candidate in self.candidates() {
            let Candidate {
                path: candidate,
                kind,
            } = candidate;
            // It must say its own version, and be one we can run the model on. A Windows Store
            // stub, a Python 3.9 and a Python 3.14 are all skipped here.
            let Some(version) = py_runtime::probe_version(&candidate) else {
                continue;
            };
            if !py_runtime::suitable(kind, version) {
                continue;
            }
            if kind != Kind::Venv {
                found.get_or_insert_with(|| candidate.clone());
            }
            let ready = Command::new(&candidate)
                .args(["-c", self.imports()])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            if ready {
                let cuda = !self.config.fake
                    && Command::new(&candidate)
                        .args([
                            "-c",
                            "import sys, torch; sys.exit(0 if torch.cuda.is_available() else 1)",
                        ])
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status()
                        .is_ok_and(|s| s.success());
                info = PythonInfo {
                    found: true,
                    path: Some(candidate.display().to_string()),
                    ready: true,
                    cuda,
                };
                break;
            }
        }
        if !info.found
            && let Some(path) = found
        {
            info.found = true;
            info.path = Some(path.display().to_string());
        }
        self.inner.lock().expect("runtime lock").python = Some((Instant::now(), info.clone()));
        info
    }

    fn requirements_file(&self) -> Result<PathBuf> {
        let code = self.code_dir()?;
        let file = code.join("model/serve/requirements.txt");
        ensure!(file.is_file(), "the model code has no requirements file");
        Ok(file)
    }

    /// The one command a person can run themselves when automatic setup cannot.
    fn manual_command(&self) -> String {
        let venv = self.venv_dir();
        let requirements = self
            .requirements_file()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "model/serve/requirements.txt".into());
        let (python, pip) = if cfg!(windows) {
            ("py -3.12", format!("{}\\Scripts\\pip", venv.display()))
        } else {
            ("python3", format!("{}/bin/pip", venv.display()))
        };
        format!(
            "{python} -m venv \"{v}\" && \"{pip}\" install --extra-index-url https://download.pytorch.org/whl/cpu -r \"{r}\"",
            v = venv.display(),
            r = requirements
        )
    }

    /// The Python to build the venv from: override, private, then system, never our own venv.
    fn base_python(&self) -> Option<Candidate> {
        let list: Vec<Candidate> = self
            .candidates()
            .into_iter()
            .filter(|c| c.kind != Kind::Venv)
            .collect();
        py_runtime::select(&list, &py_runtime::probe_version).map(|(candidate, _)| candidate)
    }

    /// Bytes the private Python would add to this install (0 when one is not needed).
    fn python_download_bytes(&self, fresh: bool) -> u64 {
        if self.config.fake || self.python_info(fresh).found {
            return 0;
        }
        self.config
            .manifest
            .python_for(py_runtime::host_target())
            .map_or(0, |build| build.size)
    }

    /// Download, verify and unpack the private Python (a no-op when it is already there and
    /// works). `done_before` and `total` position the byte progress within the whole install.
    pub fn ensure_private_python(&self, done_before: u64, total: u64) -> Result<PathBuf> {
        let exe = self.private_python_exe();
        if let Some(version) = py_runtime::probe_version(&exe)
            && py_runtime::suitable(Kind::Private, version)
        {
            return Ok(exe);
        }
        let build = self
            .config
            .manifest
            .python_for(py_runtime::host_target())
            .ok_or_else(|| {
                coded(
                    "python_missing",
                    "Python 3.10 to 3.13 was not found, and this computer has no built-in download of it. Install Python from python.org (or set RBC_KNIGHTLM_PYTHON), then press Get the model again.",
                )
            })?;
        let parts = self.root().join("partial");
        std::fs::create_dir_all(&parts)?;
        let tgz = parts.join("python.tar.gz");
        self.download_verified(
            &build.url,
            &parts.join("python.tar.gz.part"),
            &tgz,
            build.size,
            &build.sha256,
            done_before,
            total,
            "python",
        )?;
        self.set_phase(Phase::SettingUp {
            step: "Unpacking Python".into(),
        });
        let staging = self.root().join("python.staging");
        let _ = std::fs::remove_dir_all(&staging);
        py_runtime::extract_python(&tgz, &staging).map_err(|e| {
            coded(
                "python_setup_failed",
                format!("The downloaded Python could not be unpacked: {e:#}"),
            )
        })?;
        let live = self.private_python_dir();
        let _ = std::fs::remove_dir_all(&live);
        std::fs::rename(staging.join("python"), &live)?;
        let _ = std::fs::remove_dir_all(&staging);
        let _ = std::fs::remove_file(&tgz);
        match py_runtime::probe_version(&exe) {
            Some(version) if py_runtime::suitable(Kind::Private, version) => Ok(exe),
            _ => Err(coded(
                "python_setup_failed",
                "The downloaded Python does not run on this computer. Install Python 3.10 to 3.13 from python.org, then press Get the model again.",
            )),
        }
    }

    fn make_venv(&self, base: &Path) -> Result<()> {
        let venv = self.venv_dir();
        let _ = std::fs::remove_dir_all(&venv);
        let out = Command::new(base)
            .args(["-m", "venv"])
            .arg(&venv)
            .stdin(Stdio::null())
            .output()
            .context("could not run Python")?;
        ensure!(
            out.status.success(),
            "venv failed: {}",
            tail_text(&out.stderr, 3)
        );
        Ok(())
    }

    /// `pip install -r requirements.txt` into the venv, reporting the package being fetched.
    fn run_pip(&self, python: &Path, requirements: &Path) -> Result<()> {
        let cache = self.root().join("pip-cache");
        let mut child = Command::new(python)
            .args([
                "-m",
                "pip",
                "install",
                "--disable-pip-version-check",
                "--no-input",
                "--progress-bar",
                "off",
                "--extra-index-url",
                "https://download.pytorch.org/whl/cpu",
                "-r",
            ])
            .arg(requirements)
            .env("PIP_CACHE_DIR", &cache)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("could not run pip")?;
        let stderr = child.stderr.take();
        let errors = std::thread::spawn(move || {
            let mut text = Vec::new();
            if let Some(mut stderr) = stderr {
                let _ = stderr.read_to_end(&mut text);
            }
            text
        });
        if let Some(stdout) = child.stdout.take() {
            for line in std::io::BufRead::lines(std::io::BufReader::new(stdout)).map_while(Result::ok) {
                let line = line.trim();
                if let Some(what) = line
                    .strip_prefix("Collecting ")
                    .or_else(|| line.strip_prefix("Downloading "))
                {
                    let what: String = what.split_whitespace().next().unwrap_or("").chars().take(60).collect();
                    self.set_phase(Phase::SettingUp {
                        step: format!("Installing the Python packages (about 1 GB, a few minutes): {what}"),
                    });
                }
            }
        }
        let status = child.wait()?;
        let stderr = errors.join().unwrap_or_default();
        let _ = std::fs::remove_dir_all(&cache);
        ensure!(status.success(), "pip failed: {}", tail_text(&stderr, 4));
        Ok(())
    }

    /// Make sure a Python with the serving dependencies exists, building a venv if needed.
    /// `total` is the whole install's byte count (the private Python is its first part).
    fn ensure_python(&self, total: u64) -> Result<PythonInfo> {
        let info = self.python_info(true);
        if info.ready {
            return Ok(info);
        }
        if self.config.fake {
            return Err(coded(
                "python_missing",
                "The fake model server needs Python with fastapi and uvicorn.",
            ));
        }
        let requirements = self.requirements_file()?;
        let mut base = match self.base_python() {
            Some(base) => base,
            None => Candidate {
                path: self.ensure_private_python(0, total)?,
                kind: Kind::Private,
            },
        };
        self.set_phase(Phase::SettingUp {
            step: "Creating the Python environment".into(),
        });
        let mut made = self.make_venv(&base.path);
        if made.is_err()
            && base.kind == Kind::System
            && self
                .config
                .manifest
                .python_for(py_runtime::host_target())
                .is_some()
        {
            // A system Python that cannot make a venv (Debian without python3-venv): use ours.
            base = Candidate {
                path: self.ensure_private_python(0, total)?,
                kind: Kind::Private,
            };
            self.set_phase(Phase::SettingUp {
                step: "Creating the Python environment".into(),
            });
            made = self.make_venv(&base.path);
        }
        if let Err(error) = made {
            return Err(coded(
                "python_setup_failed",
                format!(
                    "Python could not create its environment ({error:#}). Run this once, then try again: {}",
                    self.manual_command()
                ),
            ));
        }
        let venv_python = py_runtime::venv_python(&self.venv_dir(), Self::windows());
        ensure!(
            venv_python.is_file(),
            "the new Python environment has no python"
        );
        self.set_phase(Phase::SettingUp {
            step: "Installing the Python packages (about 1 GB, a few minutes)".into(),
        });
        if let Err(error) = self.run_pip(&venv_python, &requirements) {
            return Err(coded(
                "python_setup_failed",
                format!(
                    "Installing the Python packages failed (is the network up? {error:#}). Run this once, then try again: {}",
                    self.manual_command()
                ),
            ));
        }
        let info = self.python_info(true);
        ensure!(info.ready, "the Python packages did not install correctly");
        Ok(info)
    }

    // --- code ---

    /// Where the serving code (`model/`, `crates/contract/schemas`) is.
    pub fn code_dir(&self) -> Result<PathBuf> {
        let has = |dir: &Path| dir.join("model/serve/server.py").is_file();
        if let Some(dir) = &self.config.code_dir {
            ensure!(
                has(dir),
                "RBC_MODEL_CODE does not hold model/serve/server.py"
            );
            return Ok(dir.clone());
        }
        let mut options = Vec::new();
        if let Ok(exe) = std::env::current_exe()
            && let Some(dir) = exe.parent()
        {
            options.push(dir.join("../share/rustybrickchess/model-runtime"));
            options.push(dir.join("model-runtime"));
            options.push(dir.join("../../../.."));
        }
        options.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")));
        options.into_iter().find(|dir| has(dir)).ok_or_else(|| {
            coded(
                "model_code_missing",
                "The model server code is not installed with this app.",
            )
        })
    }

    // --- install ---

    /// Start installing in the background. Returns at once; poll [`Runtime::phase`].
    pub fn install(self: &Arc<Self>) -> Result<()> {
        {
            let mut inner = self.inner.lock().expect("runtime lock");
            ensure!(!inner.installing, "the model is already being installed");
            ensure!(
                inner.child.is_none(),
                "stop the model before reinstalling it"
            );
            inner.installing = true;
            inner.phase = Phase::Downloading {
                bytes: 0,
                total: self.config.manifest.total_bytes(),
                file: String::new(),
            };
        }
        let me = self.clone();
        std::thread::spawn(move || {
            let result = me.run_install();
            let mut inner = me.inner.lock().expect("runtime lock");
            inner.installing = false;
            inner.phase = match result {
                Ok(()) => Phase::Ready,
                Err(error) => match error.downcast_ref::<Coded>() {
                    Some(Coded(code, message)) => err(code, message.clone()),
                    None => err("install_failed", format!("{error:#}")),
                },
            };
            if matches!(inner.phase, Phase::Ready) {
                // Derived from disk from now on.
                inner.phase = Phase::NotInstalled;
                inner.python = None;
            }
        });
        Ok(())
    }

    /// Install and wait (tests, and the thread above).
    pub fn run_install(&self) -> Result<()> {
        let manifest = &self.config.manifest;
        if !self.config.unpinned {
            manifest.check_pinned()?;
        }
        std::fs::create_dir_all(self.root())?;
        let _ = std::fs::remove_file(self.marker());
        // Python first, before the big weights: the private Python download (about 50 MB) and
        // the pip install are the likelier failures, and they are cheap to retry. A system
        // Python that is new enough is used as is; otherwise the pinned standalone one is fetched.
        let python_bytes = self.python_download_bytes(true);
        let total = manifest.total_bytes() + python_bytes;
        if !self.config.fake {
            self.set_phase(Phase::Downloading {
                bytes: 0,
                total,
                file: "python".into(),
            });
            self.ensure_python(total)?;
        }
        // Staging: nothing here is ever seen as installed.
        let staging = self.root().join("staging");
        let parts = self.root().join("partial");
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(staging.join("base"))?;
        std::fs::create_dir_all(&parts)?;
        let mut finished = python_bytes;
        let set_progress = |this: &Self, bytes: u64, file: &str| {
            this.set_phase(Phase::Downloading {
                bytes,
                total,
                file: file.to_string(),
            });
        };

        for spec in &manifest.base.files {
            set_progress(self, finished, &spec.name);
            let target = staging.join("base").join(&spec.name);
            let part = parts.join(format!("{}.part", spec.name));
            let url = format!("{}{}", manifest.base.url_prefix, spec.name);
            let source = match &self.config.dev_base_dir {
                Some(dir) => format!("file://{}", dir.join(&spec.name).display()),
                None => url,
            };
            self.download_verified(
                &source,
                &part,
                &target,
                spec.size,
                &spec.sha256,
                finished,
                total,
                &spec.name,
            )?;
            finished += spec.size;
        }

        // The adapter tarball.
        set_progress(self, finished, "adapter");
        let tgz_part = parts.join("adapter.tar.gz.part");
        let tgz = parts.join("adapter.tar.gz");
        self.download_verified(
            &manifest.adapter.url,
            &tgz_part,
            &tgz,
            manifest.adapter.size,
            &manifest.adapter.sha256,
            finished,
            total,
            "adapter",
        )?;
        self.set_phase(Phase::Verifying);
        let adapter_stage = staging.join("adapter").join(&manifest.adapter.dir);
        extract_adapter(&tgz, &adapter_stage, &manifest.adapter).map_err(|e| {
            coded(
                "adapter_invalid",
                format!("The adapter archive was refused: {e:#}"),
            )
        })?;

        // Atomic move into place, marker last.
        self.set_phase(Phase::Verifying);
        for name in ["base", "adapter"] {
            let live = self.root().join(name);
            if live.exists() {
                std::fs::remove_dir_all(&live)?;
            }
            std::fs::rename(staging.join(name), &live)?;
        }
        let _ = std::fs::remove_dir_all(&staging);
        let _ = std::fs::remove_dir_all(&parts);
        std::fs::write(
            self.marker(),
            json!({"revision": manifest.base.revision, "adapter_sha256": manifest.adapter.sha256, "fake": self.config.fake}).to_string(),
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn download_verified(
        &self,
        url: &str,
        part: &Path,
        target: &Path,
        size: u64,
        sha: &str,
        done_before: u64,
        total: u64,
        label: &str,
    ) -> Result<()> {
        let fail = |e: anyhow::Error| {
            coded(
                "download_failed",
                format!(
                    "Downloading {label} failed: {e:#}. Check your connection and press Get the model again; the download resumes."
                ),
            )
        };
        // A complete part from an earlier run still has to pass the hash below.
        let mut progress = |bytes: u64| {
            self.set_phase(Phase::Downloading {
                bytes: done_before + bytes,
                total,
                file: label.to_string(),
            });
        };
        if std::fs::metadata(part).map(|m| m.len()).unwrap_or(0) != size {
            fetch(url, part, size, &mut progress).map_err(fail)?;
        }
        self.set_phase(Phase::Verifying);
        if let Err(error) = verify_file(part, size, sha) {
            let _ = std::fs::remove_file(part);
            return Err(coded(
                "hash_mismatch",
                format!(
                    "{label} did not match its pinned checksum and was discarded ({error:#}). Press Get the model again."
                ),
            ));
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(part, target)?;
        Ok(())
    }

    // --- start / stop ---

    /// Start the server and register it as the `knightlm` provider. Returns its base URL.
    pub fn start(&self, db: &rbc_db::Db) -> Result<String> {
        {
            let mut inner = self.inner.lock().expect("runtime lock");
            if Self::child_alive(&mut inner)
                && let Some(running) = inner.child.as_ref()
            {
                let url = running.url.clone();
                drop(inner);
                self.register(db, &url)?;
                return Ok(url);
            }
            ensure!(!inner.installing, "the model is still being installed");
            inner.phase = Phase::Starting;
        }
        let result = self.spawn_and_wait();
        match result {
            Ok((running, url)) => {
                {
                    let mut inner = self.inner.lock().expect("runtime lock");
                    inner.child = Some(running);
                    inner.phase = Phase::Running { url: url.clone() };
                }
                self.register(db, &url)?;
                Ok(url)
            }
            Err(error) => {
                self.set_phase(match error.downcast_ref::<Coded>() {
                    Some(Coded(code, message)) => err(code, message.clone()),
                    None => err("start_failed", format!("{error:#}")),
                });
                Err(error)
            }
        }
    }

    fn spawn_and_wait(&self) -> Result<(RunningChild, String)> {
        if !self.files_installed() {
            return Err(coded(
                "not_installed",
                "The model is not installed yet. Press Get the model first.",
            ));
        }
        let python = self.python_info(true);
        let Some(path) = python.path.clone().filter(|_| python.ready) else {
            return Err(coded(
                "python_missing",
                format!(
                    "Python with the model's packages was not found. Run this once, then try again: {}",
                    self.manual_command()
                ),
            ));
        };
        let code = self.code_dir()?;
        let port = std::net::TcpListener::bind("127.0.0.1:0")?
            .local_addr()?
            .port();
        let wrapper = "import os,sys,threading,runpy\n\
def _w():\n    try:\n        sys.stdin.buffer.read()\n    except Exception:\n        pass\n    os._exit(0)\n\
threading.Thread(target=_w,daemon=True).start()\n\
m=sys.argv[1]; sys.argv=[m]+sys.argv[2:]\n\
runpy.run_module(m,run_name='__main__',alter_sys=True)\n";
        let mut command = Command::new(&path);
        command.arg("-c").arg(wrapper);
        if self.config.fake {
            command.args(["model.serve.fake_server", "--port", &port.to_string()]);
        } else {
            command
                .args(["model.serve.server", "--base-dir"])
                .arg(self.base_dir())
                .arg("--adapter-dir")
                .arg(self.adapter_dir())
                .args(["--port", &port.to_string()])
                .args(["--device", if python.cuda { "cuda" } else { "cpu" }]);
        }
        command
            .current_dir(&code)
            .env("PYTHONPATH", &code)
            .env("PYTHONUNBUFFERED", "1")
            .env("HF_HUB_OFFLINE", "1")
            .env("TRANSFORMERS_OFFLINE", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = command.spawn().context("could not start Python")?;
        let stdin = child.stdin.take();
        let mut running = RunningChild {
            child,
            _stdin: stdin,
            url: format!("http://127.0.0.1:{port}/v1"),
        };
        let limit = if self.config.fake { 60 } else { 300 };
        let deadline = Instant::now() + Duration::from_secs(limit);
        let probe = format!("{}/models", running.url);
        loop {
            if let Ok(Some(status)) = running.child.try_wait() {
                return Err(coded(
                    "server_exited",
                    format!(
                        "The model server exited while starting ({status}). Try Get the model again to repair the install."
                    ),
                ));
            }
            if http_ok(&probe) {
                let url = running.url.clone();
                return Ok((running, url));
            }
            if Instant::now() > deadline {
                let _ = running.child.kill();
                let _ = running.child.wait();
                return Err(coded(
                    "start_timeout",
                    "The model server did not become ready in time.",
                ));
            }
            std::thread::sleep(Duration::from_millis(300));
        }
    }

    /// Point the `knightlm` provider row at the running server, adding the row if absent.
    fn register(&self, db: &rbc_db::Db, url: &str) -> Result<()> {
        let rows = db.list_providers()?;
        let row = rows
            .iter()
            .find(|row| row.kind == crate::providers::KNIGHTLM_KIND && row.label == PROVIDER_LABEL)
            .or_else(|| {
                rows.iter()
                    .find(|row| row.kind == crate::providers::KNIGHTLM_KIND)
            });
        match row {
            Some(row) if row.base_url == url => {}
            Some(row) => {
                db.update_provider(&row.id, None, Some(url), None)?;
            }
            None => {
                db.add_provider(PROVIDER_LABEL, crate::providers::KNIGHTLM_KIND, url, None)?;
            }
        }
        Ok(())
    }

    /// The id of the registered `knightlm` provider row, if any.
    pub fn provider_id(&self, db: &rbc_db::Db) -> Option<String> {
        db.list_providers()
            .ok()?
            .into_iter()
            .find(|row| row.kind == crate::providers::KNIGHTLM_KIND)
            .map(|row| row.id)
    }

    pub fn stop(&self) {
        let mut inner = self.inner.lock().expect("runtime lock");
        if let Some(mut running) = inner.child.take() {
            drop(running._stdin.take());
            let _ = running.child.kill();
            let _ = running.child.wait();
        }
        if matches!(inner.phase, Phase::Running { .. } | Phase::Starting) {
            inner.phase = Phase::NotInstalled;
        }
    }

    /// The process id of the running server (tests).
    #[doc(hidden)]
    pub fn child_pid(&self) -> Option<u32> {
        self.inner
            .lock()
            .expect("runtime lock")
            .child
            .as_ref()
            .map(|c| c.child.id())
    }

    /// Let go of the server the way a dying app would: close its stdin and do nothing else (tests).
    #[doc(hidden)]
    pub fn abandon(&self) {
        let mut inner = self.inner.lock().expect("runtime lock");
        if let Some(running) = inner.child.as_mut() {
            drop(running._stdin.take());
        }
    }

    pub fn status_json(&self, db: &rbc_db::Db) -> Value {
        let phase = self.phase();
        let python = self.python_info(false);
        let note = if self.config.fake {
            "Test mode: a stand-in server that plays the first legal move. Not the model."
        } else if python.cuda {
            "A GPU was found; the model runs on it."
        } else {
            "No GPU was found. The model runs on the CPU, so each reply can take tens of seconds."
        };
        json!({
            "phase": phase,
            "mode": if self.config.fake { "fake" } else { "real" },
            "installed": self.files_installed(),
            "download_bytes": self.config.manifest.total_bytes(),
            // What a private Python would add (0 when one is already usable), and whether this
            // computer has a pinned one to download at all.
            "python_download_bytes": self.python_download_bytes(false),
            "python_downloadable": self.config.manifest.python_for(py_runtime::host_target()).is_some(),
            "python": python,
            "device_note": note,
            "dev_source": self.config.dev_base_dir.is_some(),
            "base": {"repo": self.config.manifest.base.repo, "revision": self.config.manifest.base.revision, "license": self.config.manifest.base.license},
            "provider_id": self.provider_id(db),
        })
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        if let Ok(mut inner) = self.inner.lock()
            && let Some(mut running) = inner.child.take()
        {
            let _ = running.child.kill();
            let _ = running.child.wait();
        }
    }
}

/// The last `lines` non-empty lines of some output, for an error a person can read.
fn tail_text(bytes: &[u8], lines: usize) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut last: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = last.len().saturating_sub(lines);
    last.drain(..start);
    let joined = last.join(" / ");
    joined.chars().take(400).collect()
}

fn http_ok(url: &str) -> bool {
    ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .timeout_global(Some(Duration::from_secs(2)))
            .build(),
    )
    .get(url)
    .call()
    .is_ok()
}

// --- HTTP ---------------------------------------------------------------------------------------

fn runtime(state: &AppState) -> Result<Arc<Runtime>, ApiError> {
    runtime_for(&state.data_dir).map_err(|e| ApiError::BadRequest(format!("{e:#}")))
}

/// `GET /api/knightlm/runtime`
pub async fn get_status(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let runtime = runtime(&state)?;
    let db = state.db.clone();
    let status = tokio::task::spawn_blocking(move || runtime.status_json(&db))
        .await
        .map_err(|e| ApiError::Internal(anyhow!(e)))?;
    Ok(Json(status))
}

/// `POST /api/knightlm/runtime/install` (returns at once; poll the status)
pub async fn post_install(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let runtime = runtime(&state)?;
    let started = runtime.clone();
    tokio::task::spawn_blocking(move || started.install())
        .await
        .map_err(|e| ApiError::Internal(anyhow!(e)))?
        .map_err(|e| ApiError::BadRequest(format!("{e:#}")))?;
    let db = state.db.clone();
    Ok(Json(runtime.status_json(&db)))
}

/// `POST /api/knightlm/runtime/start` (waits until the server answers, then registers it)
pub async fn post_start(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let runtime = runtime(&state)?;
    let db = state.db.clone();
    let started = runtime.clone();
    let db2 = db.clone();
    tokio::task::spawn_blocking(move || started.start(&db2))
        .await
        .map_err(|e| ApiError::Internal(anyhow!(e)))?
        .map_err(|e| {
            ApiError::BadRequest(match e.downcast_ref::<Coded>() {
                Some(coded) => coded.1.clone(),
                None => format!("{e:#}"),
            })
        })?;
    Ok(Json(runtime.status_json(&db)))
}

/// `POST /api/knightlm/runtime/stop`
pub async fn post_stop(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let runtime = runtime(&state)?;
    let db = state.db.clone();
    let stopper = runtime.clone();
    tokio::task::spawn_blocking(move || stopper.stop())
        .await
        .map_err(|e| ApiError::Internal(anyhow!(e)))?;
    Ok(Json(runtime.status_json(&db)))
}
