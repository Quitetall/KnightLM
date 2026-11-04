//! Read-only chess tools bound to services.
//!
//! Position questions (`get_position`, `get_history`, `legal_moves`) are
//! answered from the board's own move list, so the model cannot be told a
//! position the board does not have. Everything else — the engine, the opening
//! explorer, the game database, Syzygy — sits behind a small trait with no
//! transport of its own: the server passes its adapters in, and a test passes a
//! fake. A service that was not configured answers [`result_code::UNAVAILABLE`]
//! rather than pretending to have found nothing.

use serde_json::Value;
use shakmaty::{
    CastlingMode, Chess, Color, EnPassantMode, Position, fen::Fen, san::SanPlus, uci::UciMove,
};

use crate::agent::ToolHost;
use crate::board::BoardContent;
use crate::protocol::{ToolCall, ToolResult, result_code};
use crate::result::{internal, ok, refused, unavailable};
use rbc_db::Db;

/// One line from an engine.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineLine {
    /// Best move in SAN, when the engine reported one.
    pub best_move: Option<String>,
    /// Score from the side to move, in centipawns.
    pub score_cp: Option<i32>,
    /// Forced mate distance, when the engine reports mate.
    pub mate_in: Option<i32>,
    /// Principal variation in SAN.
    pub pv: Vec<String>,
    pub depth: u32,
}

/// A chess engine the read tools may call.
pub trait Engine {
    /// Analyse a position to `depth`.
    fn analyse(&self, fen: &str, depth: u32) -> Result<EngineLine, String>;

    /// A short evaluation, for `evaluate`.
    fn evaluate(&self, fen: &str) -> Result<EngineLine, String> {
        self.analyse(fen, 12)
    }

    /// The engine build this server is running, recorded in the turn.
    fn fingerprint(&self) -> String;
}

/// One candidate move from an opening source.
#[derive(Debug, Clone, PartialEq)]
pub struct OpeningMove {
    pub san: String,
    pub games: u64,
    pub white_wins: u64,
    pub draws: u64,
    pub black_wins: u64,
}

/// An opening book, local or remote.
pub trait Openings {
    /// Moves played in this position, most popular first.
    fn explore(&self, fen: &str, source: &str) -> Result<Vec<OpeningMove>, String>;
}

/// One game returned by the database.
#[derive(Debug, Clone, PartialEq)]
pub struct GameSample {
    pub id: String,
    pub white: String,
    pub black: String,
    pub result: String,
    pub year: Option<i32>,
    pub moves: String,
}

/// The game database.
pub trait Games {
    /// Aggregate answer for a query, with a few sample games.
    fn search(&self, query: &str) -> Result<Vec<GameSample>, String>;
}

/// A Syzygy answer.
#[derive(Debug, Clone, PartialEq)]
pub struct TablebaseEntry {
    /// `win`, `draw`, `loss` or `unknown`, from the side to move.
    pub outcome: String,
    pub dtz: Option<u32>,
    pub dtm: Option<u32>,
    pub best_move: Option<String>,
}

/// A tablebase.
pub trait Tablebase {
    /// Probe one position.
    fn probe(&self, fen: &str) -> Result<TablebaseEntry, String>;
}

/// The read tools, over the repository and whatever services are configured.
pub struct ReadHost<'a> {
    db: &'a Db,
    engine: Option<&'a dyn Engine>,
    openings: Option<&'a dyn Openings>,
    games: Option<&'a dyn Games>,
    tablebase: Option<&'a dyn Tablebase>,
}

impl<'a> ReadHost<'a> {
    /// Position tools only.
    pub fn new(db: &'a Db) -> Self {
        Self {
            db,
            engine: None,
            openings: None,
            games: None,
            tablebase: None,
        }
    }

    /// Attach an engine.
    pub fn with_engine(mut self, engine: &'a dyn Engine) -> Self {
        self.engine = Some(engine);
        self
    }

    /// Attach an opening book.
    pub fn with_openings(mut self, openings: &'a dyn Openings) -> Self {
        self.openings = Some(openings);
        self
    }

    /// Attach a game database.
    pub fn with_games(mut self, games: &'a dyn Games) -> Self {
        self.games = Some(games);
        self
    }

    /// Attach a tablebase.
    pub fn with_tablebase(mut self, tablebase: &'a dyn Tablebase) -> Self {
        self.tablebase = Some(tablebase);
        self
    }
}

impl ToolHost for ReadHost<'_> {
    fn call(&mut self, call: &ToolCall) -> ToolResult {
        match call.name.as_str() {
            "get_position" => self.get_position(call),
            "get_history" => self.get_history(call),
            "legal_moves" => self.legal_moves(call),
            "analyze" => self.analyze(call),
            "evaluate" => self.evaluate(call),
            "opening_explorer" => self.opening_explorer(call),
            "search_database" => self.search_database(call),
            "tablebase" => self.tablebase(call),
            "list_workspaces" => self.list_workspaces(call),
            other => refused(
                call,
                result_code::NOT_FOUND,
                format!("no tool called {other}"),
            ),
        }
    }
}

impl ReadHost<'_> {
    fn get_position(&mut self, call: &ToolCall) -> ToolResult {
        let (board, position) = match self.position(call) {
            Ok(pair) => pair,
            Err(result) => return result,
        };
        let fen = fen_of(&position);
        let fields: Vec<&str> = fen.split(' ').collect();
        ok(
            call,
            format!(
                "{} to move, {} piece(s)",
                if position.turn() == Color::White {
                    "white"
                } else {
                    "black"
                },
                position.board().occupied().count()
            ),
            serde_json::json!({
                "board_id": board.id,
                "title": board.title,
                "fen": fen,
                "start_fen": board.start_fen,
                "turn": if position.turn() == Color::White { "white" } else { "black" },
                "move_number": fields.get(5).and_then(|n| n.parse::<u32>().ok()).unwrap_or(1),
                "halfmove_clock": fields.get(4).and_then(|n| n.parse::<u32>().ok()).unwrap_or(0),
                "piece_count": position.board().occupied().count(),
                "plies": self.moves(call).unwrap_or_default().len(),
                "revision": board.revision,
                "mutability": board.mutability,
            }),
        )
    }

    fn get_history(&mut self, call: &ToolCall) -> ToolResult {
        let board = match self.board(call) {
            Ok(board) => board,
            Err(result) => return result,
        };
        let content = match BoardContent::parse(&board.content_json) {
            Ok(content) => content,
            Err(error) => return internal(call, error),
        };
        let (mut position, mode) = match parse_position(&board.start_fen) {
            Ok(pair) => pair,
            Err(message) => return refused(call, result_code::INTERNAL, message),
        };
        let mut plies = Vec::with_capacity(content.moves.len());
        for text in &content.moves {
            let Some(mv) = resolve_move(&position, mode, text) else {
                return refused(
                    call,
                    result_code::INTERNAL,
                    format!("stored move {text} is not legal on this board"),
                );
            };
            let san = SanPlus::from_move(position.clone(), mv).to_string();
            let Ok(played) = position.play(mv) else {
                return refused(
                    call,
                    result_code::INTERNAL,
                    format!("stored move {text} could not be played"),
                );
            };
            position = played;
            plies.push(serde_json::json!({
                "ply": plies.len() + 1,
                "san": san,
                "fen": fen_of(&position),
            }));
        }
        ok(
            call,
            format!("{} ply/plies", plies.len()),
            serde_json::json!({
                "board_id": board.id,
                "start_fen": board.start_fen,
                "plies": plies,
                "fen": fen_of(&position),
                "revision": board.revision,
            }),
        )
    }

    fn legal_moves(&mut self, call: &ToolCall) -> ToolResult {
        let (_board, position) = match self.position(call) {
            Ok(pair) => pair,
            Err(result) => return result,
        };
        let mode = match parse_position(&fen_of(&position)) {
            Ok((_, mode)) => mode,
            Err(message) => return refused(call, result_code::INTERNAL, message),
        };
        let mut moves: Vec<(String, String)> = position
            .legal_moves()
            .into_iter()
            .map(|mv| {
                (
                    SanPlus::from_move(position.clone(), mv).to_string(),
                    UciMove::from_move(mv, mode).to_string(),
                )
            })
            .collect();
        moves.sort();
        let rows: Vec<Value> = moves
            .into_iter()
            .map(|(san, uci)| serde_json::json!({"san": san, "uci": uci}))
            .collect();
        ok(
            call,
            format!("{} legal move(s)", rows.len()),
            serde_json::json!({"moves": rows}),
        )
    }

    fn analyze(&mut self, call: &ToolCall) -> ToolResult {
        let Some(engine) = self.engine else {
            return unavailable(call, "the engine");
        };
        let (_board, position) = match self.position(call) {
            Ok(pair) => pair,
            Err(result) => return result,
        };
        let depth = call
            .arguments
            .get("depth")
            .and_then(Value::as_u64)
            .unwrap_or(12)
            .clamp(1, 30) as u32;
        let fen = fen_of(&position);
        match engine.analyse(&fen, depth) {
            Ok(line) => ok(
                call,
                engine_summary(&line),
                engine_data(&fen, &line, engine),
            ),
            Err(message) => refused(call, result_code::INTERNAL, message),
        }
    }

    fn evaluate(&mut self, call: &ToolCall) -> ToolResult {
        let Some(engine) = self.engine else {
            return unavailable(call, "the engine");
        };
        let (_board, position) = match self.position(call) {
            Ok(pair) => pair,
            Err(result) => return result,
        };
        let fen = fen_of(&position);
        match engine.evaluate(&fen) {
            Ok(line) => ok(
                call,
                engine_summary(&line),
                engine_data(&fen, &line, engine),
            ),
            Err(message) => refused(call, result_code::INTERNAL, message),
        }
    }

    fn opening_explorer(&mut self, call: &ToolCall) -> ToolResult {
        let Some(openings) = self.openings else {
            return unavailable(call, "the opening explorer");
        };
        let (board, position) = match self.position(call) {
            Ok(pair) => pair,
            Err(result) => return result,
        };
        let source = call
            .arguments
            .get("source")
            .and_then(Value::as_str)
            .unwrap_or("local")
            .to_string();
        let fen = call
            .arguments
            .get("fen")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| fen_of(&position));
        match openings.explore(&fen, &source) {
            Ok(moves) => {
                let rows: Vec<Value> = moves
                    .iter()
                    .map(|mv| {
                        serde_json::json!({
                            "san": mv.san,
                            "games": mv.games,
                            "white": mv.white_wins,
                            "draws": mv.draws,
                            "black": mv.black_wins,
                        })
                    })
                    .collect();
                ok(
                    call,
                    format!("{} move(s) from the {source} source", rows.len()),
                    serde_json::json!({
                        "board_id": board.id,
                        "source": source,
                        "fen": fen,
                        "moves": rows,
                    }),
                )
            }
            Err(message) => refused(call, result_code::INTERNAL, message),
        }
    }

    fn search_database(&mut self, call: &ToolCall) -> ToolResult {
        let Some(games) = self.games else {
            return unavailable(call, "the game database");
        };
        let Some(query) = call.arguments.get("query").and_then(Value::as_str) else {
            return refused(call, result_code::INVALID_ARGUMENTS, "query is required");
        };
        match games.search(query) {
            Ok(samples) => {
                let rows: Vec<Value> = samples
                    .iter()
                    .map(|sample| {
                        serde_json::json!({
                            "game_id": sample.id,
                            "white": sample.white,
                            "black": sample.black,
                            "result": sample.result,
                            "year": sample.year,
                        })
                    })
                    .collect();
                ok(
                    call,
                    format!("{} sample(s) for {query:?}", rows.len()),
                    serde_json::json!({"query": query, "count": rows.len(), "samples": rows}),
                )
            }
            Err(message) => refused(call, result_code::INTERNAL, message),
        }
    }

    fn tablebase(&mut self, call: &ToolCall) -> ToolResult {
        let Some(tablebase) = self.tablebase else {
            return unavailable(call, "the tablebase");
        };
        let (board, position) = match self.position(call) {
            Ok(pair) => pair,
            Err(result) => return result,
        };
        let fen = fen_of(&position);
        match tablebase.probe(&fen) {
            Ok(entry) => ok(
                call,
                format!(
                    "{:?}: {}",
                    entry.outcome,
                    entry.best_move.clone().unwrap_or_default()
                )
                .trim()
                .to_string(),
                serde_json::json!({
                    "board_id": board.id,
                    "fen": fen,
                    "outcome": entry.outcome,
                    "dtz": entry.dtz,
                    "dtm": entry.dtm,
                    "best_move": entry.best_move,
                }),
            ),
            Err(message) => refused(call, result_code::INTERNAL, message),
        }
    }

    fn list_workspaces(&mut self, call: &ToolCall) -> ToolResult {
        match self.db.workspaces(false) {
            Ok(workspaces) => {
                let mut rows = Vec::with_capacity(workspaces.len());
                for workspace in workspaces {
                    let boards = self
                        .db
                        .boards("workspace", &workspace.id)
                        .map(|boards| boards.len())
                        .unwrap_or(0);
                    rows.push(serde_json::json!({
                        "workspace_id": workspace.id,
                        "title": workspace.title,
                        "kind": workspace.kind,
                        "boards": boards,
                    }));
                }
                ok(
                    call,
                    format!("{} workspace(s)", rows.len()),
                    Value::Array(rows),
                )
            }
            Err(error) => refused(
                call,
                result_code::INTERNAL,
                format!("could not list workspaces: {error}"),
            ),
        }
    }

    // --- helpers -----------------------------------------------------------

    fn board(&self, call: &ToolCall) -> Result<rbc_db::Board, ToolResult> {
        let Some(board_id) = call.arguments.get("board_id").and_then(Value::as_str) else {
            return Err(refused(
                call,
                result_code::INVALID_ARGUMENTS,
                "board_id is required",
            ));
        };
        match self.db.board(board_id) {
            Ok(Some(board)) => Ok(board),
            Ok(None) => Err(refused(
                call,
                result_code::NOT_FOUND,
                format!("no board with id {board_id}"),
            )),
            Err(error) => Err(refused(
                call,
                result_code::INTERNAL,
                format!("could not read the board: {error}"),
            )),
        }
    }

    fn moves(&self, call: &ToolCall) -> Option<Vec<String>> {
        let board = self.board(call).ok()?;
        BoardContent::parse(&board.content_json)
            .ok()
            .map(|content| content.moves)
    }

    fn position(&self, call: &ToolCall) -> Result<(rbc_db::Board, Chess), ToolResult> {
        let board = self.board(call)?;
        let content =
            BoardContent::parse(&board.content_json).map_err(|error| internal(call, error))?;
        let position = content
            .position(&board.start_fen)
            .map_err(|message| refused(call, result_code::INTERNAL, message))?;
        Ok((board, position))
    }
}

fn engine_summary(line: &EngineLine) -> String {
    let score = match (line.mate_in, line.score_cp) {
        (Some(mate), _) => format!("mate in {mate}"),
        (None, Some(cp)) => format!("{cp:+} cp"),
        _ => "no score".to_string(),
    };
    match &line.best_move {
        Some(best) => format!("{best} at depth {} ({score})", line.depth),
        None => format!("depth {} ({score})", line.depth),
    }
}

fn engine_data(fen: &str, line: &EngineLine, engine: &dyn Engine) -> Value {
    serde_json::json!({
        "fen": fen,
        "best_move": line.best_move,
        "score_cp": line.score_cp,
        "mate_in": line.mate_in,
        "pv": line.pv,
        "depth": line.depth,
        "engine": engine.fingerprint(),
    })
}

fn parse_position(fen: &str) -> Result<(Chess, CastlingMode), String> {
    knightlm_core::state::parse_chess(fen).map_err(|error| format!("invalid FEN: {error}"))
}

fn resolve_move(position: &Chess, mode: CastlingMode, text: &str) -> Option<shakmaty::Move> {
    let wanted = text.trim().trim_end_matches(['!', '?', '+', '#']);
    for mv in position.legal_moves() {
        if UciMove::from_move(mv, mode).to_string() == wanted {
            return Some(mv);
        }
        let san = SanPlus::from_move(position.clone(), mv).to_string();
        if san == wanted || san.trim_end_matches(['!', '?', '+', '#']) == wanted {
            return Some(mv);
        }
    }
    None
}

fn fen_of(position: &Chess) -> String {
    Fen::from_position(position, EnPassantMode::Legal).to_string()
}
