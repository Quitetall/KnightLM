//! RustyBrickChess — the full app server, reusable by the desktop shell.
//!
//! Leptos/BrickChess frontend (static WASM) + an En-Croissant-shaped backend
//! (games database, engine analysis, classification) + AI chat.

pub mod accounts;
pub mod ai_host;
pub mod api;
pub mod archive;
pub mod console;
pub mod domain_api;
pub mod importer;
mod lamu_runtime;
mod mcp;
pub mod pieces;
mod provider_api;
mod providers;
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
        .route(
            "/api/repertoires",
            get(api::repertoires).post(api::create_repertoire),
        )
        .route(
            "/api/repertoires/{id}",
            axum::routing::delete(api::delete_repertoire),
        )
        .route("/api/repertoires/{id}/import", post(api::import_repertoire))
        .route(
            "/api/repertoires/{id}/import-game/{game_id}",
            post(api::import_repertoire_game),
        )
        .route("/api/repertoires/{id}/next", get(api::next_repertoire_card))
        .route("/api/repertoires/review", post(api::review_repertoire_card))
        .route(
            "/api/databases",
            get(api::databases).post(api::create_database),
        )
        .route("/api/databases/switch", post(api::switch_database))
        .route(
            "/api/sounds/custom/{name}",
            get(api::custom_sound).post(api::upload_sound),
        )
        .route("/api/accounts", get(api::accounts))
        .route("/api/session", get(api::get_session).put(api::put_session))
        .route("/api/session/notice", get(api::session_notice))
        .route(
            "/api/workspaces",
            get(domain_api::list_workspaces).post(domain_api::create_workspace),
        )
        .route(
            "/api/workspaces/{id}",
            get(domain_api::get_workspace)
                .patch(domain_api::update_workspace)
                .delete(domain_api::delete_workspace),
        )
        .route("/api/workspaces/{id}/boards", get(domain_api::list_boards))
        .route("/api/boards", post(domain_api::create_board))
        .route(
            "/api/boards/{id}",
            get(domain_api::get_board).put(domain_api::update_board),
        )
        .route("/api/boards/{id}/copy", post(domain_api::copy_board))
        .route(
            "/api/boards/{id}/reparent",
            post(domain_api::reparent_board),
        )
        .route("/api/boards/{id}/lock", post(domain_api::lock_board))
        .route("/api/boards/{id}/unlock", post(domain_api::unlock_board))
        .route(
            "/api/studies",
            get(domain_api::list_studies).post(domain_api::create_study),
        )
        .route(
            "/api/studies/{id}/chapters",
            get(domain_api::list_chapters).post(domain_api::create_chapter),
        )
        .route("/api/studies/{id}/publish", post(domain_api::publish_study))
        .route("/api/studies/{id}/copy", post(domain_api::copy_study))
        .route(
            "/api/studies/{id}/progress",
            get(domain_api::get_progress).put(domain_api::put_progress),
        )
        .route("/api/studies/{id}/lock", post(domain_api::lock_study))
        .route("/api/studies/{id}/branch", post(domain_api::branch_study))
        .route(
            "/api/studies/{id}/versions",
            get(domain_api::list_study_versions),
        )
        .route(
            "/api/studies/{id}/bundle",
            get(domain_api::export_study_bundle),
        )
        .route("/api/compare", get(domain_api::compare_studies))
        .route(
            "/api/compare/scope",
            get(domain_api::get_compare_scope)
                .put(domain_api::set_compare_scope)
                .delete(domain_api::clear_compare_scope),
        )
        .route("/api/studies/import", post(domain_api::import_study_bundle))
        .route(
            "/api/studies/import-pgn",
            post(domain_api::import_study_pgn),
        )
        .route(
            "/api/studies/{id}",
            get(domain_api::get_study)
                .put(domain_api::update_study)
                .delete(domain_api::delete_study),
        )
        .route(
            "/api/studies/{id}/chapters/{chapter_id}",
            axum::routing::put(domain_api::update_chapter).delete(domain_api::delete_chapter),
        )
        .route(
            "/api/studies/{id}/chapters/{chapter_id}/duplicate",
            post(domain_api::duplicate_chapter),
        )
        .route(
            "/api/studies/{id}/chapters/{chapter_id}/boards",
            post(domain_api::add_chapter_board),
        )
        .route(
            "/api/studies/{id}/chapters/{chapter_id}/primary",
            post(domain_api::set_chapter_primary),
        )
        .route(
            "/api/studies/{id}/chapters/reorder",
            post(domain_api::reorder_chapters),
        )
        .route(
            "/api/ai/sessions",
            get(domain_api::list_ai_sessions).post(domain_api::create_ai_session),
        )
        .route(
            "/api/ai/sessions/{id}",
            get(domain_api::get_ai_session).delete(domain_api::archive_ai_session),
        )
        .route(
            "/api/ai/sessions/{id}/rename",
            post(domain_api::rename_ai_session),
        )
        .route(
            "/api/ai/sessions/{id}/target",
            post(domain_api::set_ai_target),
        )
        .route(
            "/api/ai/sessions/{id}/turns",
            get(domain_api::list_ai_turns).post(domain_api::append_ai_turn),
        )
        .route("/api/ai/sessions/{id}/send", post(domain_api::send_ai_turn))
        .route(
            "/api/ai/sessions/{id}/replay/{seq}",
            get(domain_api::replay_ai_turn),
        )
        .route(
            "/api/ai/sessions/{id}/stream",
            get(domain_api::stream_ai_session),
        )
        .route(
            "/api/ai/sessions/{id}/cancel",
            post(domain_api::cancel_ai_turn),
        )
        .route("/mcp", axum::routing::post(mcp::mcp_http))
        .route("/api/mcp/config", axum::routing::get(mcp::mcp_config))
        .route("/api/mcp/rotate", axum::routing::post(mcp::mcp_rotate))
        .route("/api/ai/approvals", get(domain_api::list_approvals))
        .route(
            "/api/ai/sessions/{id}/permissions",
            axum::routing::put(domain_api::set_ai_permissions),
        )
        .route("/api/ai/approvals/{id}", get(domain_api::get_approval))
        .route(
            "/api/ai/approvals/{id}/approve",
            post(domain_api::approve_approval),
        )
        .route(
            "/api/ai/approvals/{id}/deny",
            post(domain_api::deny_approval),
        )
        .route(
            "/api/ai/matches",
            get(domain_api::list_matches).post(domain_api::create_match),
        )
        .route("/api/ai/ratings", get(domain_api::list_ratings))
        .route("/api/ai/metrics", get(domain_api::list_metrics))
        .route(
            "/api/ai/tournaments",
            get(domain_api::list_tournaments).post(domain_api::create_tournament),
        )
        .route("/api/ai/tournaments/{id}", get(domain_api::get_tournament))
        .route(
            "/api/ai/tournaments/{id}/pgn",
            get(domain_api::tournament_pgn),
        )
        .route(
            "/api/ai/tournaments/{id}/{action}",
            post(domain_api::tournament_action),
        )
        .route("/api/ai/matches/{id}", get(domain_api::get_match))
        .route(
            "/api/ai/matches/{id}/move",
            post(domain_api::play_match_move),
        )
        .route(
            "/api/ai/matches/{id}/model-move",
            post(domain_api::model_match_move),
        )
        .route(
            "/api/ai/matches/{id}/takeback",
            post(domain_api::takeback_match_move),
        )
        .route("/api/ai/matches/{id}/chat", post(domain_api::chat_match))
        .route(
            "/api/ai/matches/{id}/watch/step",
            post(domain_api::watch_step),
        )
        .route(
            "/api/ai/matches/{id}/watch/pause",
            post(domain_api::watch_pause),
        )
        .route(
            "/api/ai/matches/{id}/watch/resume",
            post(domain_api::watch_resume),
        )
        .route(
            "/api/ai/matches/{id}/watch/reset",
            post(domain_api::watch_reset),
        )
        .route(
            "/api/ai/matches/{id}/resign",
            post(domain_api::resign_match),
        )
        .route("/api/ai/matches/{id}/pgn", get(domain_api::match_pgn))
        .route("/api/ai/context/compile", post(domain_api::compile_context))
        .route(
            "/api/pieces",
            get(api::piece_sets).post(api::upload_piece_set),
        )
        .route(
            "/api/pieces/{name}",
            axum::routing::delete(api::delete_piece_set),
        )
        .route("/api/pieces/{name}/{code}", get(api::piece_file))
        .route("/api/accounts/link", post(api::link_account))
        .route("/api/accounts/unlink", post(api::unlink_account))
        .route("/api/accounts/import", post(api::import_account_games))
        .route("/api/cloud-eval", get(api::cloud_eval))
        .route("/api/engines/test", post(api::test_engine))
        .route("/api/engine/console/start", post(api::console_start))
        .route("/api/engine/console/command", post(api::console_command))
        .route("/api/engine/console/stop", post(api::console_stop))
        .route("/api/engine/console/status", get(api::console_status))
        .route("/api/vision", get(api::get_vision))
        .route(
            "/api/providers",
            get(provider_api::list_providers).post(provider_api::add_provider),
        )
        .route(
            "/api/providers/{id}",
            axum::routing::put(provider_api::edit_provider).delete(provider_api::delete_provider),
        )
        .route(
            "/api/providers/{id}/test",
            post(provider_api::test_provider),
        )
        .route(
            "/api/providers/{id}/models",
            post(provider_api::refresh_models),
        )
        .route("/api/providers/scan", get(provider_api::scan_local))
        .route("/api/vision/download", post(api::download_vision))
        .route("/api/vision/recognize", post(api::recognize_image))
        .route(
            "/api/settings",
            get(api::get_settings).put(api::put_settings),
        )
        .route("/api/analyze/lines", get(api::analyze_lines))
        .route("/api/games", get(api::list_games).post(api::import_game))
        .route("/api/profile", get(api::profile))
        .route("/api/import/lichess", post(api::import_lichess))
        .route("/api/import/chesscom", post(api::import_chesscom))
        .route("/api/analyze/engines", get(api::analyze_engines))
        .route(
            "/api/games/{id}",
            get(api::get_game)
                .patch(api::update_game)
                .delete(api::delete_game),
        )
        .route(
            "/api/games/{id}/moves",
            axum::routing::post(api::update_moves),
        )
        .route("/api/games/{id}/moves/truncate", post(api::truncate_moves))
        .route("/api/games/{id}/tree", axum::routing::put(api::update_tree))
        .route("/api/games/{id}/tree/delete", post(api::delete_tree_node))
        .route("/api/games/{id}/tree/promote", post(api::promote_variation))
        .route("/api/games/{id}/tree/add", post(api::add_tree_move))
        .route("/api/games/{id}/pgn", get(api::game_pgn))
        .route("/api/games/{id}/evals", get(api::game_evals))
        .route(
            "/api/games/{id}/annotations",
            get(api::annotations).post(api::add_annotation),
        )
        .route(
            "/api/collections",
            get(api::collections).post(api::create_collection),
        )
        .route(
            "/api/collections/{id}",
            axum::routing::patch(api::rename_collection).delete(api::delete_collection),
        )
        .route(
            "/api/collections/{id}/games/{game_id}",
            post(api::add_to_collection),
        )
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
        .route("/api/analyze/engine", post(api::analyze_engine))
        .route("/api/report", post(api::report))
        .route("/api/chat", post(api::chat))
        .fallback_service(ServeDir::new(static_dir).fallback(ServeFile::new(index)))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Default per-user data root (`$XDG_DATA_HOME/rustybrickchess`).
pub fn default_data_root() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("XDG_DATA_HOME")
        && !dir.trim().is_empty()
    {
        return std::path::Path::new(&dir).join("rustybrickchess");
    }
    if let Ok(home) = std::env::var("HOME")
        && !home.trim().is_empty()
    {
        return std::path::Path::new(&home).join(".local/share/rustybrickchess");
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
        console: std::sync::Arc::new(std::sync::Mutex::new(None)),
        turn_cancels: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        lamu_child: Default::default(),
    };
    Ok((state, db_path))
}

/// Serve on `addr` (e.g. `127.0.0.1:8090`).
pub async fn serve_on(addr: &str) -> anyhow::Result<()> {
    let (state, db_path) = state_from_env()?;
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    // Report where we actually bound. A test passes port 0 and reads this line,
    // and a person who lost the port to another process sees which one it got.
    let bound = listener
        .local_addr()
        .map(|local| local.to_string())
        .unwrap_or_else(|_| addr.to_string());
    println!("RustyBrickChess on http://{bound}  (db: {db_path})");
    println!(
        "  STOCKFISH_PATH: {:?}",
        std::env::var("STOCKFISH_PATH").ok()
    );
    println!("  MODEL_URL: {:?}", std::env::var("MODEL_URL").ok());
    axum::serve(listener, app).await?;
    Ok(())
}

/// Serve using `RBC_ADDR` (default `127.0.0.1:8090`).
pub async fn serve_from_env() -> anyhow::Result<()> {
    let addr = std::env::var("RBC_ADDR").unwrap_or_else(|_| "127.0.0.1:8090".into());
    serve_on(&addr).await
}

/// Run the MCP stdio server against this database.
pub fn serve_mcp_stdio(db: &rbc_db::Db) -> anyhow::Result<()> {
    mcp::serve_stdio(db)
}
