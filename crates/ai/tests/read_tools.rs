//! Read tools against a real repository and fake services.
//!
//! Position questions must be answered from the board's own move list; the
//! engine, opening book, database and tablebase are faked here, because what
//! the tools promise is the *shape* of the answer and the fact that an
//! unconfigured service says so instead of inventing an empty result.
#![cfg(feature = "host")]

use std::cell::RefCell;

use rbc_ai::agent::ToolHost;
use rbc_ai::protocol::{ToolCall, result_code};
use rbc_ai::reads::{
    Engine, EngineLine, GameSample, Games, OpeningMove, Openings, ReadHost, Tablebase,
    TablebaseEntry,
};
use rbc_db::{Board, Db};

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

fn setup(moves: &[&str]) -> (Db, Board) {
    let db = Db::open_memory().expect("in-memory database");
    let workspace = db.create_workspace("Analysis", "analysis").expect("workspace");
    let content = serde_json::json!({ "moves": moves }).to_string();
    let board = db
        .create_board("workspace", &workspace.id, "analysis", "Board 1", START, &content)
        .expect("board");
    (db, board)
}

fn call(name: &str, arguments: serde_json::Value) -> ToolCall {
    ToolCall::new("call_1", name, arguments)
}

fn on_board(board: &Board) -> serde_json::Value {
    serde_json::json!({"board_id": board.id})
}

// --- fake services ---------------------------------------------------------

struct FakeEngine {
    fingerprint: String,
    seen: RefCell<Vec<(String, u32)>>,
}

impl FakeEngine {
    fn new() -> Self {
        Self {
            fingerprint: "fake-engine-1".into(),
            seen: RefCell::new(Vec::new()),
        }
    }
}

impl Engine for FakeEngine {
    fn analyse(&self, fen: &str, depth: u32) -> Result<EngineLine, String> {
        self.seen.borrow_mut().push((fen.to_string(), depth));
        Ok(EngineLine {
            best_move: Some("Nf3".into()),
            score_cp: Some(31),
            mate_in: None,
            pv: vec!["Nf3".into(), "Nc6".into()],
            depth,
        })
    }

    fn fingerprint(&self) -> String {
        self.fingerprint.clone()
    }
}

struct FakeOpenings;

impl Openings for FakeOpenings {
    fn explore(&self, _fen: &str, _source: &str) -> Result<Vec<OpeningMove>, String> {
        Ok(vec![OpeningMove {
            san: "e4".into(),
            games: 1200,
            white_wins: 600,
            draws: 400,
            black_wins: 200,
        }])
    }
}

struct FakeGames;

impl Games for FakeGames {
    fn search(&self, _query: &str) -> Result<Vec<GameSample>, String> {
        Ok(vec![GameSample {
            id: "42".into(),
            white: "Carlsen, M".into(),
            black: "Nakamura, H".into(),
            result: "1-0".into(),
            year: Some(2023),
            moves: "1. e4 e5".into(),
        }])
    }
}

struct FakeTablebase;

impl Tablebase for FakeTablebase {
    fn probe(&self, _fen: &str) -> Result<TablebaseEntry, String> {
        Ok(TablebaseEntry {
            outcome: "win".into(),
            dtz: Some(9),
            dtm: Some(21),
            best_move: Some("Kg4".into()),
        })
    }
}

// --- position --------------------------------------------------------------

#[test]
fn read_tools_get_position_reports_the_fen_and_whose_turn_it_is() {
    let (db, board) = setup(&["e4", "e5", "Nf3"]);
    let mut host = ReadHost::new(&db);

    let result = host.call(&call("get_position", on_board(&board)));

    assert!(result.ok, "{result:?}");
    let data = result.data.as_ref().expect("data");
    assert_eq!(
        data["fen"],
        serde_json::json!("rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2")
    );
    assert_eq!(data["turn"], serde_json::json!("black"));
    assert_eq!(data["move_number"], serde_json::json!(2));
    assert_eq!(data["plies"], serde_json::json!(3));
    assert_eq!(data["piece_count"], serde_json::json!(32));
    assert_eq!(data["revision"], serde_json::json!(1));
    assert_eq!(data["mutability"], serde_json::json!("mutable"));
}

#[test]
fn read_tools_get_history_replays_every_ply_with_its_fen() {
    let (db, board) = setup(&["e4", "e5"]);
    let mut host = ReadHost::new(&db);

    let result = host.call(&call("get_history", on_board(&board)));

    assert!(result.ok, "{result:?}");
    let plies = result.data.as_ref().expect("data")["plies"]
        .as_array()
        .expect("plies")
        .clone();
    assert_eq!(plies.len(), 2);
    assert_eq!(plies[0]["ply"], serde_json::json!(1));
    assert_eq!(plies[0]["san"], serde_json::json!("e4"));
    assert_eq!(plies[1]["san"], serde_json::json!("e5"));
    assert!(plies[1]["fen"].as_str().expect("fen").contains("3.5N2")
        || plies[1]["fen"].as_str().expect("fen").starts_with("rnbqkbnr/pppp1ppp/8/4p3"));
}

#[test]
fn read_tools_legal_moves_are_sorted_san_and_uci() {
    let (db, board) = setup(&[]);
    let mut host = ReadHost::new(&db);

    let result = host.call(&call("legal_moves", on_board(&board)));

    assert!(result.ok, "{result:?}");
    let moves = result.data.as_ref().expect("data")["moves"]
        .as_array()
        .expect("moves")
        .clone();
    assert_eq!(moves.len(), 20, "twenty legal first moves");
    let sans: Vec<&str> = moves
        .iter()
        .map(|mv| mv["san"].as_str().expect("san"))
        .collect();
    let mut sorted = sans.clone();
    sorted.sort();
    assert_eq!(sans, sorted, "sorted by SAN");
    let nf3 = moves
        .iter()
        .find(|mv| mv["san"] == serde_json::json!("Nf3"))
        .expect("Nf3 is legal");
    assert_eq!(nf3["uci"], serde_json::json!("g1f3"));
}

#[test]
fn read_tools_refuse_a_board_whose_stored_moves_cannot_be_played() {
    let (db, _board) = setup(&["Qh9"]);
    let board = db
        .boards("workspace", &db.workspaces(false).expect("workspaces")[0].id)
        .expect("boards")
        .remove(0);
    let mut host = ReadHost::new(&db);

    let result = host.call(&call("get_position", on_board(&board)));

    assert!(!result.ok);
    assert_eq!(result.code, result_code::INTERNAL);
    assert!(result.summary.contains("Qh9"), "{}", result.summary);
}

// --- engine ----------------------------------------------------------------

#[test]
fn read_tools_analyze_returns_lines_and_pins_the_engine_fingerprint() {
    let (db, board) = setup(&["e4"]);
    let engine = FakeEngine::new();
    let mut host = ReadHost::new(&db).with_engine(&engine);

    let result = host.call(&call(
        "analyze",
        serde_json::json!({"board_id": board.id, "depth": 18}),
    ));

    assert!(result.ok, "{result:?}");
    let data = result.data.as_ref().expect("data");
    assert_eq!(data["best_move"], serde_json::json!("Nf3"));
    assert_eq!(data["score_cp"], serde_json::json!(31));
    assert_eq!(data["depth"], serde_json::json!(18));
    assert_eq!(data["engine"], serde_json::json!("fake-engine-1"));
    assert_eq!(data["pv"], serde_json::json!(["Nf3", "Nc6"]));
    assert_eq!(result.summary, "Nf3 at depth 18 (+31 cp)");

    // The engine saw the position after the board's own move.
    let seen = engine.seen.borrow();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].0.starts_with("rnbqkbnr/pppppppp/8/8/4P3"), "{}", seen[0].0);
    assert_eq!(seen[0].1, 18);
}

#[test]
fn read_tools_evaluate_answers_with_a_shallow_search() {
    let (db, board) = setup(&[]);
    let engine = FakeEngine::new();
    let mut host = ReadHost::new(&db).with_engine(&engine);

    let result = host.call(&call("evaluate", on_board(&board)));

    assert!(result.ok, "{result:?}");
    assert_eq!(result.data.as_ref().expect("data")["depth"], serde_json::json!(12));
}

#[test]
fn read_tools_say_so_when_no_engine_is_configured() {
    let (db, board) = setup(&[]);
    let mut host = ReadHost::new(&db);

    let result = host.call(&call("analyze", on_board(&board)));

    assert!(!result.ok);
    assert_eq!(result.code, result_code::UNAVAILABLE);
    assert!(result.summary.contains("engine"));
}

// --- external sources ------------------------------------------------------

#[test]
fn read_tools_opening_explorer_lists_moves_with_counts() {
    let (db, board) = setup(&["e4"]);
    let mut host = ReadHost::new(&db).with_openings(&FakeOpenings);

    let result = host.call(&call(
        "opening_explorer",
        serde_json::json!({"board_id": board.id, "source": "lichess"}),
    ));

    assert!(result.ok, "{result:?}");
    let data = result.data.as_ref().expect("data");
    assert_eq!(data["source"], serde_json::json!("lichess"));
    assert_eq!(data["moves"][0]["san"], serde_json::json!("e4"));
    assert_eq!(data["moves"][0]["games"], serde_json::json!(1200));
    assert_eq!(data["moves"][0]["draws"], serde_json::json!(400));
}

#[test]
fn read_tools_search_database_returns_samples() {
    let (db, _board) = setup(&[]);
    let mut host = ReadHost::new(&db).with_games(&FakeGames);

    let result = host.call(&call(
        "search_database",
        serde_json::json!({"query": "Carlsen"}),
    ));

    assert!(result.ok, "{result:?}");
    let data = result.data.as_ref().expect("data");
    assert_eq!(data["count"], serde_json::json!(1));
    assert_eq!(data["samples"][0]["white"], serde_json::json!("Carlsen, M"));
    assert_eq!(data["samples"][0]["result"], serde_json::json!("1-0"));
}

#[test]
fn read_tools_tablebase_reports_the_outcome_and_distance() {
    let (db, board) = setup(&[]);
    let mut host = ReadHost::new(&db).with_tablebase(&FakeTablebase);

    let result = host.call(&call("tablebase", on_board(&board)));

    assert!(result.ok, "{result:?}");
    let data = result.data.as_ref().expect("data");
    assert_eq!(data["outcome"], serde_json::json!("win"));
    assert_eq!(data["dtz"], serde_json::json!(9));
    assert_eq!(data["best_move"], serde_json::json!("Kg4"));
}

#[test]
fn read_tools_say_so_when_a_source_is_not_configured() {
    let (db, board) = setup(&[]);
    let mut host = ReadHost::new(&db);

    for (tool, arguments, what) in [
        ("opening_explorer", on_board(&board), "opening"),
        (
            "search_database",
            serde_json::json!({"query": "x"}),
            "database",
        ),
        ("tablebase", on_board(&board), "tablebase"),
    ] {
        let result = host.call(&call(tool, arguments));
        assert_eq!(result.code, result_code::UNAVAILABLE, "{tool}");
        assert!(result.summary.contains(what), "{tool}: {}", result.summary);
    }
}

// --- workspaces ------------------------------------------------------------

#[test]
fn read_tools_list_workspaces_counts_their_boards() {
    let (db, _board) = setup(&[]);
    let mut host = ReadHost::new(&db);

    let result = host.call(&call("list_workspaces", serde_json::json!({})));

    assert!(result.ok, "{result:?}");
    let rows = result.data.as_ref().expect("data").as_array().expect("rows").clone();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["title"], serde_json::json!("Analysis"));
    assert_eq!(rows[0]["boards"], serde_json::json!(1));
}

#[test]
fn read_tools_refuse_an_unknown_board() {
    let (db, _board) = setup(&[]);
    let mut host = ReadHost::new(&db);

    let result = host.call(&call(
        "get_position",
        serde_json::json!({"board_id": "nope"}),
    ));

    assert_eq!(result.code, result_code::NOT_FOUND);
}
