use anyhow::Result;
use knightlm_core::state::{GameRecord, GameState, START_FEN};
use knightlm_core::SCHEMA_VERSION;
use std::io::{BufRead, BufReader, Write};
use vampirc_uci::{parse_one, UciMessage};

pub trait Player {
    fn select(&self, state: &GameState) -> Result<String>;
    fn name(&self) -> &str {
        "player"
    }
}

pub struct UciEngine {
    pub player: Box<dyn Player>,
    pub state: GameState,
    pub name: String,
    pub author: String,
}

fn start() -> GameState {
    GameState::replay(&GameRecord {
        schema_version: SCHEMA_VERSION,
        game_id: "uci".into(),
        source: "uci".into(),
        initial_fen: START_FEN.into(),
        history_complete: true,
        moves: vec![],

        variant: "standard".into(),
    })
    .expect("start position")
}

impl UciEngine {
    pub fn new(player: Box<dyn Player>, name: &str, author: &str) -> Self {
        Self {
            player,
            state: start(),
            name: name.to_owned(),
            author: author.to_owned(),
        }
    }

    pub fn run(&mut self) {
        let stdin = std::io::stdin();
        let reader = BufReader::new(stdin.lock());
        let mut stdout = std::io::stdout();
        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            if self.handle(&mut stdout, line.trim()) {
                break;
            }
            let _ = stdout.flush();
        }
    }

    /// Handle one command. Returns true to quit.
    pub fn handle(&mut self, out: &mut dyn Write, line: &str) -> bool {
        let line = line.trim();
        if line.is_empty() {
            return false;
        }
        // Non-standard helper: `perft <depth>`.
        if let Some(rest) = line.strip_prefix("perft ") {
            let depth: u32 = rest.parse().unwrap_or(1);
            let nodes = perft(&self.state, depth);
            let _ = writeln!(out, "info string perft {} = {}", depth, nodes);
            return false;
        }

        match parse_one(line) {
            UciMessage::Uci => {
                let _ = writeln!(out, "id name {}", self.name);
                let _ = writeln!(out, "id author {}", self.author);
                let _ = writeln!(out, "option name UCI_Chess960 type check default false");
                let _ = writeln!(out, "uciok");
            }
            UciMessage::Debug(_) => {}
            UciMessage::IsReady => {
                let _ = writeln!(out, "readyok");
            }
            UciMessage::SetOption { .. } => {}
            UciMessage::UciNewGame => {
                self.state = start();
            }
            UciMessage::Position {
                startpos,
                fen,
                moves,
            } => {
                let fen_str = if startpos {
                    START_FEN.to_owned()
                } else {
                    fen.map(|f| f.to_string())
                        .unwrap_or_else(|| START_FEN.to_owned())
                };
                let mut full = fen_str;
                let n = full.split_whitespace().count();
                if n < 6 {
                    for _ in n..6 {
                        full.push_str(" 0");
                    }
                }
                let rec = GameRecord {
                    schema_version: SCHEMA_VERSION,
                    game_id: "uci".into(),
                    source: "uci".into(),
                    initial_fen: full,
                    history_complete: true,
                    moves: moves.iter().map(|m| m.to_string()).collect(),
                    variant: "standard".into(),
                };
                // Refuse an invalid position; keep the previous state rather than
                // silently repairing it.
                if let Ok(s) = GameState::replay(&rec) {
                    self.state = s;
                }
            }
            UciMessage::Go { .. } => self.go(out),
            UciMessage::Stop | UciMessage::PonderHit => {}
            UciMessage::Quit => return true,
            _ => {}
        }
        false
    }

    fn go(&mut self, out: &mut dyn Write) {
        if self.state.facts().terminal_reason.is_some() {
            let _ = writeln!(out, "info string position is terminal");
            let _ = writeln!(out, "bestmove (none)");
            return;
        }
        match self.player.select(&self.state) {
            Ok(mv) if self.state.is_legal(&mv) => {
                let _ = writeln!(out, "bestmove {}", mv);
            }
            Ok(mv) => {
                let _ = writeln!(out, "info string illegal move produced: {}", mv);
                let _ = writeln!(out, "bestmove (none)");
            }
            Err(e) => {
                let _ = writeln!(out, "info string player error: {}", e);
                let _ = writeln!(out, "bestmove (none)");
            }
        }
    }
}

pub fn perft(state: &GameState, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }
    if state.facts().terminal_reason.is_some() {
        return 1;
    }
    let mut total = 0u64;
    for mv in state.legal_moves() {
        let mut s = state.clone();
        if s.play(&mv).is_ok() {
            total += perft(&s, depth - 1);
        }
    }
    total
}
