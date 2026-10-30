//! A persistent UCI engine session for the engine console, plus a one-shot
//! handshake used by "test engine".

use anyhow::{Context, Result, bail};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

pub struct Console {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    pub name: String,
    pub path: String,
}

impl Console {
    pub fn spawn(path: &str) -> Result<Self> {
        let mut child = Command::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("spawn {path}"))?;
        let stdin = child.stdin.take().context("engine stdin")?;
        let stdout = child.stdout.take().context("engine stdout")?;
        let stderr = child.stderr.take().context("engine stderr")?;
        let (tx, rx): (Sender<String>, Receiver<String>) = channel();
        for pipe in [
            Box::new(stdout) as Box<dyn std::io::Read + Send>,
            Box::new(stderr),
        ] {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let reader = BufReader::new(pipe);
                for line in reader.lines() {
                    match line {
                        Ok(line) => {
                            if tx.send(line).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }
        let mut console = Self {
            child,
            stdin,
            lines: rx,
            name: String::new(),
            path: path.to_owned(),
        };
        console.send("uci")?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Ok(line) = console.lines.recv_timeout(Duration::from_millis(100)) {
                if let Some(name) = line.strip_prefix("id name ") {
                    console.name = name.trim().to_owned();
                }
                if line.trim() == "uciok" {
                    break;
                }
            }
        }
        console.send("isready")?;
        // Drain the handshake's readyok so the first user command starts clean.
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            match console.lines.recv_timeout(Duration::from_millis(100)) {
                Ok(line) if line.trim() == "readyok" => break,
                Ok(_) => continue,
                Err(_) => continue,
            }
        }
        Ok(console)
    }

    fn send(&mut self, command: &str) -> Result<()> {
        self.stdin
            .write_all(command.as_bytes())
            .context("write to engine")?;
        self.stdin.write_all(b"\n").context("newline")?;
        self.stdin.flush().context("flush")?;
        Ok(())
    }

    /// Send one command and collect the engine's output for up to `wait_ms`,
    /// stopping early on `bestmove`.
    pub fn command(&mut self, command: &str, wait_ms: u64) -> Result<Vec<String>> {
        self.send(command)?;
        let deadline = Instant::now() + Duration::from_millis(wait_ms.min(30_000));
        let mut out = Vec::new();
        while Instant::now() < deadline {
            match self.lines.recv_timeout(Duration::from_millis(50)) {
                Ok(line) => {
                    let done = line.starts_with("bestmove");
                    out.push(line);
                    if done {
                        break;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        Ok(out)
    }

    pub fn stop(mut self) {
        let _ = self.send("quit");
        let _ = self.child.wait();
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct EngineOptionInfo {
    pub name: String,
    pub kind: String,
    pub default: Option<String>,
    pub min: Option<String>,
    pub max: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct EngineInfo {
    pub ok: bool,
    pub name: String,
    pub author: String,
    pub options: Vec<EngineOptionInfo>,
    pub error: Option<String>,
}

/// Spawn an engine just long enough to read its UCI identity and options.
pub fn probe(path: &str) -> EngineInfo {
    let mut info = EngineInfo {
        ok: false,
        name: String::new(),
        author: String::new(),
        options: Vec::new(),
        error: None,
    };
    if path.trim().is_empty() {
        info.error = Some("no engine path".into());
        return info;
    }
    let mut child = match Command::new(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            info.error = Some(error.to_string());
            return info;
        }
    };
    let Some(mut stdin) = child.stdin.take() else {
        info.error = Some("no stdin".into());
        return info;
    };
    let Some(stdout) = child.stdout.take() else {
        info.error = Some("no stdout".into());
        return info;
    };
    if stdin.write_all(b"uci\n").is_err() {
        info.error = Some("could not send uci".into());
        return info;
    }
    let _ = stdin.flush();
    let (tx, rx): (Sender<String>, Receiver<String>) = channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => {
                if let Some(name) = line.strip_prefix("id name ") {
                    info.name = name.trim().to_owned();
                } else if let Some(author) = line.strip_prefix("id author ") {
                    info.author = author.trim().to_owned();
                } else if let Some(rest) = line.strip_prefix("option ") {
                    info.options.push(parse_option(rest));
                } else if line.trim() == "uciok" {
                    info.ok = true;
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let _ = stdin.write_all(b"quit\n");
    let _ = child.wait();
    if !info.ok && info.error.is_none() {
        info.error = Some("engine did not answer uci".into());
    }
    info
}

fn parse_option(rest: &str) -> EngineOptionInfo {
    let mut name = String::new();
    let mut kind = String::new();
    let mut default = None;
    let mut min = None;
    let mut max = None;
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    let mut index = 0;
    while index < tokens.len() {
        match tokens[index] {
            "name" => {
                index += 1;
                let start = index;
                while index < tokens.len() && tokens[index] != "type" {
                    index += 1;
                }
                name = tokens[start..index].join(" ");
                continue;
            }
            "type" => {
                index += 1;
                kind = tokens.get(index).copied().unwrap_or_default().to_owned();
                index += 1;
                continue;
            }
            "default" => {
                index += 1;
                let start = index;
                while index < tokens.len() && !matches!(tokens[index], "min" | "max" | "var") {
                    index += 1;
                }
                default = Some(tokens[start..index].join(" "));
                continue;
            }
            "min" => {
                index += 1;
                min = tokens.get(index).map(|v| v.to_string());
                index += 1;
                continue;
            }
            "max" => {
                index += 1;
                max = tokens.get(index).map(|v| v.to_string());
                index += 1;
                continue;
            }
            _ => {
                index += 1;
            }
        }
    }
    EngineOptionInfo {
        name,
        kind,
        default,
        min,
        max,
    }
}

pub fn require_path(path: &str) -> Result<()> {
    if path.trim().is_empty() {
        bail!("no engine path configured");
    }
    Ok(())
}
