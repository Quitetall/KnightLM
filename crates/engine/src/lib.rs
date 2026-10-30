//! A Stockfish (or any UCI) client.
//!
//! Protocol parsing is delegated to `vampirc-uci`; this crate owns the process,
//! timeouts, and the typed `Analysis`/`Label` results. `STOCKFISH_PATH` is an
//! operator contract: nothing is downloaded here.

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};
use vampirc_uci::{parse_one, UciInfoAttribute, UciMessage};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Analysis {
    pub engine: String,
    pub version: String,
    pub depth: u32,
    pub best_move: String,
    pub score_cp: Option<i32>,
    pub mate: Option<i32>,
    pub pv: Vec<String>,
    pub nodes: Option<u64>,
    pub nps: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multipv: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Label {
    pub position_key: String,
    pub fen: String,
    pub best_move: String,
    pub score_cp: Option<i32>,
    pub mate: Option<i32>,
    pub pv: Vec<String>,
    pub depth: u32,
    pub engine: String,
    pub version: String,
}

/// Operator-provided UCI options. Nothing is downloaded or guessed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EngineOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threads: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash_mb: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub syzygy_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multipv: Option<u32>,
    #[serde(default)]
    pub chess960: bool,
    /// `UCI_Variant` value for Fairy-Stockfish et al. (None for standard).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    /// Arbitrary `setoption` pairs from the engine's own settings
    /// (Skill Level, UCI_Elo, Ponder, ...).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<(String, String)>,
}

impl EngineOptions {
    /// The `setoption` lines these options expand to, in a stable order.
    pub fn commands(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(threads) = self.threads.filter(|t| *t > 0) {
            out.push(format!("setoption name Threads value {threads}"));
        }
        if let Some(hash) = self.hash_mb.filter(|h| *h > 0) {
            out.push(format!("setoption name Hash value {hash}"));
        }
        if let Some(path) = self.syzygy_path.as_deref().filter(|p| !p.trim().is_empty()) {
            out.push(format!("setoption name SyzygyPath value {}", path.trim()));
        }
        if let Some(n) = self.multipv.filter(|n| *n > 0) {
            out.push(format!("setoption name MultiPV value {n}"));
        }
        if self.chess960 {
            out.push("setoption name UCI_Chess960 value true".to_owned());
        }
        if let Some(variant) = self.variant.as_deref().filter(|v| !v.is_empty()) {
            out.push(format!("setoption name UCI_Variant value {variant}"));
        }
        for (name, value) in &self.extra {
            let name = name.trim();
            let value = value.trim();
            if name.is_empty() || value.is_empty() {
                continue;
            }
            out.push(format!("setoption name {name} value {value}"));
        }
        out
    }
}

pub struct Engine {
    child: Child,
    stdin: std::process::ChildStdin,
    rx: Receiver<String>,
    name: String,
    version: String,
    timeout: Duration,
}

impl Engine {
    pub fn spawn(path: &str, timeout_secs: u64) -> Result<Self> {
        Self::spawn_with(path, timeout_secs, EngineOptions::default())
    }

    pub fn spawn_with(path: &str, timeout_secs: u64, options: EngineOptions) -> Result<Self> {
        let mut child = Command::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("spawn {path}"))?;
        let stdin = child.stdin.take().context("no stdin")?;
        let stdout = child.stdout.take().context("no stdout")?;
        let (tx, rx): (Sender<String>, Receiver<String>) = mpsc::channel();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        if tx.send(l).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        let mut eng = Self {
            child,
            stdin,
            rx,
            name: String::new(),
            version: String::new(),
            timeout: Duration::from_secs(timeout_secs),
        };
        eng.handshake()?;
        for cmd in options.commands() {
            eng.send(&cmd)?;
        }
        eng.send("isready")?;
        eng.wait_ready()?;
        Ok(eng)
    }

    fn send(&mut self, cmd: &str) -> Result<()> {
        self.stdin
            .write_all(cmd.as_bytes())
            .context("write to engine")?;
        self.stdin.write_all(b"\n").context("write newline")?;
        self.stdin.flush().context("flush")?;
        Ok(())
    }

    /// Read the next parsed UCI message, waiting until `deadline`.
    fn recv_until(&mut self, deadline: Instant) -> Result<UciMessage> {
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                bail!("engine timeout");
            }
            match self.rx.recv_timeout(remaining.min(Duration::from_secs(2))) {
                Ok(line) => {
                    // Terminal positions answer `bestmove (none)`, which the UCI
                    // parser cannot represent; fail fast instead of waiting for
                    // the timeout.
                    if line.starts_with("bestmove") && line.contains("(none)") {
                        bail!("no best move");
                    }
                    return Ok(parse_one(&line));
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => bail!("engine closed its output"),
            }
        }
    }

    fn handshake(&mut self) -> Result<()> {
        self.send("uci")?;
        let deadline = Instant::now() + self.timeout;
        loop {
            match self.recv_until(deadline)? {
                UciMessage::Id { name, author } => {
                    if let Some(n) = name {
                        self.name = n;
                    }
                    if let Some(a) = author {
                        let _ = a;
                    }
                }
                UciMessage::UciOk => break,
                _ => {}
            }
        }
        self.wait_ready()?;
        if self.version.is_empty() {
            self.version = self.name.clone();
        }
        Ok(())
    }

    fn wait_ready(&mut self) -> Result<()> {
        self.send("isready")?;
        let deadline = Instant::now() + self.timeout;
        loop {
            if matches!(self.recv_until(deadline)?, UciMessage::ReadyOk) {
                break;
            }
        }
        Ok(())
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn version(&self) -> &str {
        &self.version
    }

    #[allow(unused_assignments)]
    pub fn analyse(&mut self, fen: &str, depth: u32) -> Result<Analysis> {
        self.send("setoption name MultiPV value 1")?;
        self.send(&format!("position fen {fen}"))?;
        self.send(&format!("go depth {depth}"))?;

        let mut best_move = String::new();
        let mut score_cp = None;
        let mut mate = None;
        let mut pv = Vec::new();
        let mut cur_depth = 0u32;
        let mut nodes = None;
        let mut nps = None;
        let deadline = Instant::now() + self.timeout;
        loop {
            match self.recv_until(deadline)? {
                UciMessage::Info(attrs) => {
                    let line_depth = attrs
                        .iter()
                        .find_map(|a| match a {
                            UciInfoAttribute::Depth(d) => Some(u32::from(*d)),
                            _ => None,
                        })
                        .unwrap_or(0);
                    if line_depth >= cur_depth {
                        if line_depth > 0 {
                            cur_depth = line_depth;
                        }
                        for attr in attrs {
                            match attr {
                                UciInfoAttribute::Score { cp, mate: m, .. } => {
                                    score_cp = cp;
                                    mate = m.map(i32::from);
                                }
                                UciInfoAttribute::Pv(v) => {
                                    pv = v.iter().map(|m| m.to_string()).collect();
                                }
                                UciInfoAttribute::Nodes(n) => nodes = Some(n),
                                UciInfoAttribute::Nps(n) => nps = Some(n),
                                _ => {}
                            }
                        }
                    }
                }
                UciMessage::BestMove { best_move: bm, .. } => {
                    best_move = bm.to_string();
                    break;
                }
                _ => {}
            }
        }
        ensure!(!best_move.is_empty() && best_move != "(none)", "no best move");
        Ok(Analysis {
            engine: self.name.clone(),
            version: self.version.clone(),
            depth: cur_depth.max(depth),
            best_move,
            score_cp,
            mate,
            pv,
            nodes,
            nps,
            multipv: None,
        })
    }

    /// MultiPV analysis: `lines` best lines at `depth`, each as an `Analysis`.
    pub fn analyse_lines(&mut self, fen: &str, depth: u32, lines: u32) -> Result<Vec<Analysis>> {
        let n = lines.clamp(1, 8);
        self.send(&format!("setoption name MultiPV value {n}"))?;
        self.send(&format!("position fen {fen}"))?;
        self.send(&format!("go depth {depth}"))?;
        let mut best: BTreeMap<u16, (u32, Option<i32>, Option<i32>, Vec<String>, Option<u64>, Option<u64>)> =
            BTreeMap::new();
        let deadline = Instant::now() + self.timeout;
        loop {
            match self.recv_until(deadline)? {
                UciMessage::Info(attrs) => {
                    let idx = attrs
                        .iter()
                        .find_map(|a| match a {
                            UciInfoAttribute::MultiPv(k) => Some(*k),
                            _ => None,
                        })
                        .unwrap_or(1);
                    let d = attrs
                        .iter()
                        .find_map(|a| match a {
                            UciInfoAttribute::Depth(x) => Some(u32::from(*x)),
                            _ => None,
                        })
                        .unwrap_or(0);
                    let prev = best.get(&idx).map(|v| v.0).unwrap_or(0);
                    if d < prev {
                        continue;
                    }
                    let mut score = None;
                    let mut mate = None;
                    let mut pv = Vec::new();
                    let mut nodes = None;
                    let mut nps = None;
                    for attr in attrs {
                        match attr {
                            UciInfoAttribute::Score { cp, mate: m, .. } => {
                                score = cp;
                                mate = m.map(i32::from);
                            }
                            UciInfoAttribute::Pv(v) => {
                                pv = v.iter().map(|m| m.to_string()).collect();
                            }
                            UciInfoAttribute::Nodes(x) => nodes = Some(x),
                            UciInfoAttribute::Nps(x) => nps = Some(x),
                            _ => {}
                        }
                    }
                    best.insert(idx, (d, score, mate, pv, nodes, nps));
                }
                UciMessage::BestMove { .. } => break,
                _ => {}
            }
        }
        let _ = self.send("setoption name MultiPV value 1");
        Ok(best
            .into_iter()
            .map(|(idx, (d, score, mate, pv, nodes, nps))| Analysis {
                engine: self.name.clone(),
                version: self.version.clone(),
                depth: d.max(depth),
                best_move: pv.first().cloned().unwrap_or_default(),
                score_cp: score,
                mate,
                pv,
                nodes,
                nps,
                multipv: Some(idx),
            })
            .collect())
    }

    pub fn label(&mut self, fen: &str, depth: u32) -> Result<Label> {
        let a = self.analyse(fen, depth)?;
        let fields: Vec<&str> = fen.split_whitespace().collect();
        let key = if fields.len() >= 4 {
            format!("{} {} {} {}", fields[0], fields[1], fields[2], fields[3])
        } else {
            fen.to_owned()
        };
        Ok(Label {
            position_key: key,
            fen: fen.to_owned(),
            best_move: a.best_move,
            score_cp: a.score_cp,
            mate: a.mate,
            pv: a.pv,
            depth: a.depth,
            engine: a.engine,
            version: a.version,
        })
    }

    pub fn stop(&mut self) {
        let _ = self.send("stop");
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.send("quit");
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod option_tests {
    use super::EngineOptions;

    #[test]
    fn extra_options_are_appended_last() {
        let opts = EngineOptions {
            threads: Some(2),
            extra: vec![
                ("Skill Level".into(), "5".into()),
                ("UCI_Elo".into(), "1600".into()),
                ("empty".into(), "".into()),
            ],
            ..EngineOptions::default()
        };
        assert_eq!(
            opts.commands(),
            vec![
                "setoption name Threads value 2".to_owned(),
                "setoption name Skill Level value 5".to_owned(),
                "setoption name UCI_Elo value 1600".to_owned(),
            ]
        );
    }

    #[test]
    fn options_expand_to_uci_commands() {
        let opts = EngineOptions {
            extra: Vec::new(),
            threads: Some(4),
            hash_mb: Some(512),
            syzygy_path: Some("/data/syzygy".into()),
            multipv: Some(3),
            chess960: true,
            variant: Some("atomic".into()),
        };
        assert_eq!(
            opts.commands(),
            vec![
                "setoption name Threads value 4".to_owned(),
                "setoption name Hash value 512".to_owned(),
                "setoption name SyzygyPath value /data/syzygy".to_owned(),
                "setoption name MultiPV value 3".to_owned(),
                "setoption name UCI_Chess960 value true".to_owned(),
                "setoption name UCI_Variant value atomic".to_owned(),
            ]
        );
        assert!(EngineOptions::default().commands().is_empty());
    }
}
