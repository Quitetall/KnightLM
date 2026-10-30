//! RustyBrickChess — the full app server, reusable by the desktop shell.
//!
//! Leptos/BrickChess frontend (static WASM) + an En-Croissant-shaped backend
//! (games database, engine analysis, classification) + AI chat.

pub mod api;
pub mod importer;
pub mod vision;

use axum::{
    Router,
    routing::{get, post},
};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

/// Assets as built in the source tree (fallback for `cargo run`).
pub const COMPILE_STATIC_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/frontend/public");

/// Where the web assets live.
///
/// Resolution order: `RBC_STATIC`, `<exe>/../share/rustybrickchess/frontend`
/// (installed layout), `<exe>/frontend/public`, then the source tree.
pub fn static_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("RBC_STATIC") {
        let path = std::path::PathBuf::from(dir);
        if path.join("index.html").exists() {
            return path;
        }
    }
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()));
    if let Some(dir) = exe_dir.as_ref() {
        let candidates = [
            dir.join("../share/rustybrickchess/frontend"),
            dir.join("frontend/public"),
            dir.join("frontend"),
        ];
        for candidate in candidates {
            if candidate.join("index.html").exists() {
                return candidate;
            }
        }
    }
    std::path::PathBuf::from(COMPILE_STATIC_DIR)
}

/// Build the axum application with the given state.
pub fn router(state: api::AppState) -> Router {
    let static_dir = static_dir();
    let index = static_dir.join("index.html").display().to_string();
    Router::new()
        .route("/api/health", get(api::health))
        .route("/api/stats", get(api::stats))
        .route("/api/repertoires", get(api::repertoires).post(api::create_repertoire))
        .route("/api/repertoires/{id}", axum::routing::delete(api::delete_repertoire))
        .route("/api/repertoires/{id}/import", post(api::import_repertoire))
        .route("/api/repertoires/{id}/import-game/{game_id}", post(api::import_repertoire_game))
        .route("/api/repertoires/{id}/next", get(api::next_repertoire_card))
        .route("/api/repertoires/review", post(api::review_repertoire_card))
        .route("/api/databases", get(api::databases).post(api::create_database))
        .route("/api/databases/switch", post(api::switch_database))
        .route("/api/sounds/custom/{name}", get(api::custom_sound).post(api::upload_sound))
        .route("/api/vision", get(api::get_vision))
        .route("/api/vision/download", post(api::download_vision))
        .route("/api/vision/recognize", post(api::recognize_image))
        .route("/api/settings", get(api::get_settings).put(api::put_settings))
        .route("/api/analyze/lines", get(api::analyze_lines))
        .route("/api/games", get(api::list_games).post(api::import_game))
        .route("/api/profile", get(api::profile))
        .route("/api/import/lichess", post(api::import_lichess))
        .route("/api/import/chesscom", post(api::import_chesscom))
        .route("/api/analyze/engines", get(api::analyze_engines))
        .route("/api/games/{id}", get(api::get_game).patch(api::update_game).delete(api::delete_game))
        .route("/api/games/{id}/moves", axum::routing::post(api::update_moves))
        .route("/api/games/{id}/moves/truncate", post(api::truncate_moves))
        .route("/api/games/{id}/tree", axum::routing::put(api::update_tree))
        .route("/api/games/{id}/tree/delete", post(api::delete_tree_node))
        .route("/api/games/{id}/tree/promote", post(api::promote_variation))
        .route("/api/games/{id}/tree/add", post(api::add_tree_move))
        .route("/api/games/{id}/pgn", get(api::game_pgn))
        .route("/api/games/{id}/evals", get(api::game_evals))
        .route("/api/games/{id}/annotations", get(api::annotations).post(api::add_annotation))
        .route("/api/collections", get(api::collections).post(api::create_collection))
        .route("/api/collections/{id}", axum::routing::patch(api::rename_collection).delete(api::delete_collection))
        .route("/api/collections/{id}/games/{game_id}", post(api::add_to_collection))
        .route("/api/players", get(api::players))
        .route("/api/events", get(api::events))
        .route("/api/positions", get(api::positions))
        .route("/api/openings", get(api::openings))
        .route("/api/tablebase", get(api::tablebase))
        .route("/api/puzzles/next", get(api::next_puzzle))
        .route("/api/puzzles/themes", get(api::puzzle_themes))
        .route("/api/puzzles/{id}", get(api::get_puzzle))
        .route("/api/puzzles/attempts", get(api::puzzle_attempts))
        .route("/api/puzzles/attempt", post(api::puzzle_attempt))
        .route("/api/puzzles/import", post(api::import_puzzles))
        .route("/api/games/{id}/report", get(api::game_report))
        .route("/api/analyze", post(api::analyze))
        .route("/api/report", post(api::report))
        .route("/api/chat", post(api::chat))
        .fallback_service(ServeDir::new(static_dir).fallback(ServeFile::new(index)))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Default per-user data root (`$XDG_DATA_HOME/rustybrickchess`).
pub fn default_data_root() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
        if !dir.trim().is_empty() {
            return std::path::Path::new(&dir).join("rustybrickchess");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return std::path::Path::new(&home).join(".local/share/rustybrickchess");
        }
    }
    std::path::PathBuf::from(".")
}

/// Path of the default database file.
pub fn default_db_path() -> std::path::PathBuf {
    default_data_root().join("rusty-brick-chess.db")
}

/// Directory that holds the selectable `.db` files.
pub fn db_dir_from_env() -> std::path::PathBuf {
    if let Ok(db_path) = std::env::var("RBC_DB") {
        return std::path::Path::new(&db_path)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from("."));
    }
    default_data_root()
}

/// Directory for downloaded models and uploaded sounds.
pub fn data_dir_from_env() -> std::path::PathBuf {
    std::env::var("RBC_DATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| db_dir_from_env().join("data"))
}

/// State and paths assembled from the environment (`RBC_DB`, `RBC_DATA`, ...).
pub fn state_from_env() -> anyhow::Result<(api::AppState, String)> {
    let db_path = std::env::var("RBC_DB")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| default_db_path());
    let db_path = db_path.display().to_string();
    if let Some(parent) = std::path::Path::new(&db_path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let db = rbc_db::Db::open(&db_path)?;
    let db_dir = db_dir_from_env();
    let data_dir = data_dir_from_env();
    std::fs::create_dir_all(&data_dir)?;
    let state = api::AppState {
        db,
        db_dir,
        data_dir,
        engine_path: std::env::var("STOCKFISH_PATH").ok(),
        model_url: std::env::var("MODEL_URL").ok(),
        default_depth: std::env::var("RBC_DEPTH")
            .ok()
            .and_then(|d| d.parse().ok())
            .unwrap_or(15),
    };
    Ok((state, db_path))
}

/// Serve on `addr` (e.g. `127.0.0.1:8090`).
pub async fn serve_on(addr: &str) -> anyhow::Result<()> {
    let (state, db_path) = state_from_env()?;
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("RustyBrickChess on http://{addr}  (db: {db_path})");
    println!("  STOCKFISH_PATH: {:?}", std::env::var("STOCKFISH_PATH").ok());
    println!("  MODEL_URL: {:?}", std::env::var("MODEL_URL").ok());
    axum::serve(listener, app).await?;
    Ok(())
}

/// Serve using `RBC_ADDR` (default `127.0.0.1:8090`).
pub async fn serve_from_env() -> anyhow::Result<()> {
    let addr = std::env::var("RBC_ADDR").unwrap_or_else(|_| "127.0.0.1:8090".into());
    serve_on(&addr).await
}
