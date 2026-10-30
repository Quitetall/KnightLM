use anyhow::{Context, Result, bail};
use knightlm_core::state::{GameState, PacketOptions};
use knightlm_engine::Engine;
use std::sync::Mutex;
use ureq::Agent;

use crate::uci::Player;

/// Uniformly random among compiler-verified legal moves. Deterministic given a
/// seed and a position; the run-0 baseline.
pub struct RandomLegal {
    pub seed: u64,
}

impl Player for RandomLegal {
    fn select(&self, state: &GameState) -> Result<String> {
        let moves = state.legal_moves();
        if moves.is_empty() {
            bail!("no legal moves");
        }
        let mut x = self.seed
            ^ state
                .fen()
                .bytes()
                .fold(0u64, |a, b| a.wrapping_mul(31).wrapping_add(b as u64));
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        Ok(moves[(x as usize) % moves.len()].clone())
    }
    fn name(&self) -> &str {
        "RandomLegal"
    }
}

/// An external UCI engine (e.g. Stockfish) driving our state through the same
/// `Player` seam. Used as an opponent and as a label/scoring engine.
pub struct EnginePlayer {
    engine: Mutex<Engine>,
    depth: u32,
}

impl EnginePlayer {
    pub fn spawn(path: &str, depth: u32, timeout_secs: u64) -> Result<Self> {
        let engine = Engine::spawn(path, timeout_secs).context("spawn engine")?;
        Ok(Self {
            engine: Mutex::new(engine),
            depth,
        })
    }
}

impl Player for EnginePlayer {
    fn select(&self, state: &GameState) -> Result<String> {
        let a = self
            .engine
            .lock()
            .expect("engine lock")
            .analyse(&state.fen(), self.depth)?;
        Ok(a.best_move)
    }
    fn name(&self) -> &str {
        "Engine"
    }
}

/// The KnightLM model served over HTTP. Speaks the canonical packet contract;
/// a model failure is an explicit error, never a silent legal-move fallback.
pub struct ModelPlayer {
    url: String,
    agent: Agent,
}

impl ModelPlayer {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_owned(),
            agent: Agent::new_with_defaults(),
        }
    }
}

impl Player for ModelPlayer {
    fn select(&self, state: &GameState) -> Result<String> {
        let packet = state.compile(PacketOptions {
            legal_choices: true,
            derived_features: false,
        });
        let body = serde_json::json!({
            "packet": packet,
            "packet_format_version": packet.schema_version,
        });
        let mut resp = self
            .agent
            .post(&self.url)
            .send_json(body)
            .context("model request failed")?;
        let v: serde_json::Value = resp
            .body_mut()
            .read_json()
            .context("model response was not JSON")?;
        let mv = v["move"]
            .as_str()
            .context("response missing `move`")?
            .to_owned();
        // Validate the model's move against the compiler; an illegal move is a
        // reported failure (RQ-010), never repaired.
        if !state.is_legal(&mv) {
            bail!("model produced illegal move: {mv}");
        }
        Ok(mv)
    }
    fn name(&self) -> &str {
        "KnightLM"
    }
}
