//! HTTP API: games, collections, players/events, position search, engine
//! analysis, move classification, AI chat.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use brick_chess_analysis::{AnalysisConfig, Eval, MoveInput};
use rbc_db::{Annotation, AnnotationInput, Collection, Db, EventRow, Game, GameQuery, GameTags, PlayerRow, QueryResponse};
use shakmaty::{CastlingMode, Chess, EnPassantMode, Position, fen::Fen, san::{San, SanPlus}, uci::UciMove};
use shakmaty_syzygy::Tablebase;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    /// Directory that holds the selectable `.db` files.
    pub db_dir: std::path::PathBuf,
    pub data_dir: std::path::PathBuf,
    pub engine_path: Option<String>,
    pub model_url: Option<String>,
    pub default_depth: u32,
    /// Persistent UCI session used by the engine console.
    pub console: Arc<Mutex<Option<crate::console::Console>>>,
}

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    NotFound,
    Unavailable(&'static str),
    Internal(anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not found".to_owned()),
            ApiError::Unavailable(m) => (StatusCode::SERVICE_UNAVAILABLE, m.to_owned()),
            ApiError::Internal(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        ApiError::Internal(e)
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        ApiError::Internal(e.into())
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        ApiError::Internal(e.into())
    }
}

impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> Self {
        ApiError::Internal(e.into())
    }
}

pub async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "ok": true,
        "app": "rusty-brick-chess",
        "games": state.db.count().unwrap_or(0),
        "engine": state.engine_path.is_some(),
        "model": state.model_url.is_some(),
    }))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineEntry {
    pub name: String,
    pub path: String,
    /// UCI options applied when this engine is spawned (Hash, Threads,
    /// MultiPV, SyzygyPath, Skill Level, UCI_Elo, ...).
    #[serde(default)]
    pub options: BTreeMap<String, String>,
}

#[derive(Deserialize)]
pub struct RepertoireBody {
    pub name: String,
    pub color: String,
}

pub async fn repertoires(
    State(state): State<AppState>,
) -> Result<Json<Vec<rbc_db::Repertoire>>, ApiError> {
    Ok(Json(state.db.repertoires()?))
}

pub async fn create_repertoire(
    State(state): State<AppState>,
    Json(body): Json<RepertoireBody>,
) -> Result<(StatusCode, Json<rbc_db::Repertoire>), ApiError> {
    let rep = state.db.create_repertoire(&body.name, &body.color)?;
    Ok((StatusCode::CREATED, Json(rep)))
}

pub async fn delete_repertoire(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    if state.db.delete_repertoire(id)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

#[derive(Deserialize)]
pub struct RepertoireImport {
    pub pgn: String,
}

pub async fn import_repertoire(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<RepertoireImport>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let added = state.db.import_repertoire_pgn(id, &body.pgn)?;
    Ok(Json(serde_json::json!({ "imported": added })))
}

pub async fn import_repertoire_game(
    State(state): State<AppState>,
    Path((repertoire_id, game_id)): Path<(i64, i64)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pgn = state.db.export_pgn(game_id)?.ok_or(ApiError::NotFound)?;
    let added = state.db.import_repertoire_pgn(repertoire_id, &pgn)?;
    Ok(Json(serde_json::json!({ "imported": added })))
}

pub async fn next_repertoire_card(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Option<rbc_db::RepertoireCard>>, ApiError> {
    Ok(Json(state.db.next_repertoire_card(id)?))
}

#[derive(Deserialize)]
pub struct ReviewBody {
    pub card_id: i64,
    pub correct: bool,
}

pub async fn review_repertoire_card(
    State(state): State<AppState>,
    Json(body): Json<ReviewBody>,
) -> Result<Json<rbc_db::RepertoireCard>, ApiError> {
    Ok(Json(
        state.db.review_repertoire_card(body.card_id, body.correct)?,
    ))
}

pub async fn stats(State(state): State<AppState>) -> Result<Json<rbc_db::Stats>, ApiError> {
    Ok(Json(state.db.stats()?))
}

#[derive(Deserialize)]
pub struct LimitQuery {
    #[serde(default)]
    pub limit: Option<u32>,
}

pub async fn puzzle_attempts(
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> Result<Json<Vec<rbc_db::PuzzleAttempt>>, ApiError> {
    Ok(Json(state.db.puzzle_attempts(q.limit.unwrap_or(10).min(100))?))
}

#[derive(Deserialize)]
pub struct SoundUpload {
    pub data: String,
}

fn sound_dir(state: &AppState) -> std::path::PathBuf {
    state.data_dir.join("sounds")
}

pub async fn upload_sound(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<SoundUpload>,
) -> Result<StatusCode, ApiError> {
    let safe = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !safe {
        return Err(ApiError::Internal(anyhow::anyhow!("invalid sound name")));
    }
    let payload = body
        .data
        .split_once(',')
        .map(|(_, data)| data)
        .unwrap_or(&body.data);
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("invalid audio: {e}")))?;
    if bytes.is_empty() || bytes.len() > 8 * 1024 * 1024 {
        return Err(ApiError::Internal(anyhow::anyhow!("invalid audio size")));
    }
    let dir = sound_dir(&state);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(&name), bytes)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn custom_sound(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Response, ApiError> {
    let path = sound_dir(&state).join(&name);
    let bytes = std::fs::read(&path).map_err(|_| ApiError::NotFound)?;
    Ok((
        [(header::CONTENT_TYPE, "audio/ogg"), (header::CACHE_CONTROL, "no-cache")],
        bytes,
    )
        .into_response())
}

#[derive(Serialize)]
pub struct VisionStatus {
    pub installed: bool,
    pub path: String,
    pub bytes: u64,
    pub name: &'static str,
    pub license: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size: u64,
}

fn vision_status(state: &AppState) -> VisionStatus {
    let status = crate::vision::status(&state.data_dir);
    VisionStatus {
        installed: status.installed,
        path: status.path,
        bytes: status.bytes,
        name: crate::vision::NAME,
        license: crate::vision::LICENSE,
        url: crate::vision::URL,
        sha256: crate::vision::SHA256,
        size: crate::vision::SIZE,
    }
}

pub async fn get_vision(State(state): State<AppState>) -> Json<VisionStatus> {
    Json(vision_status(&state))
}

pub async fn download_vision(
    State(state): State<AppState>,
) -> Result<Json<VisionStatus>, ApiError> {
    let data_dir = state.data_dir.clone();
    tokio::task::spawn_blocking(move || crate::vision::download(&data_dir))
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))??;
    Ok(Json(vision_status(&state)))
}

#[derive(Deserialize)]
pub struct VisionImage {
    pub image: String,
}

#[derive(Serialize)]
pub struct VisionResult {
    pub placement: String,
    pub confidence: f32,
    pub board: [f32; 4],
    pub detections: usize,
    pub turn: String,
}

pub async fn recognize_image(
    State(state): State<AppState>,
    Json(body): Json<VisionImage>,
) -> Result<Json<VisionResult>, ApiError> {
    use base64::Engine;
    let payload = body
        .image
        .split_once(',')
        .map(|(_, data)| data)
        .unwrap_or(&body.image);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("invalid image: {e}")))?;
    let model = crate::vision::model_path(&state.data_dir);
    let result = tokio::task::spawn_blocking(move || crate::vision::recognize(&model, &bytes))
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))??;
    Ok(Json(VisionResult {
        placement: result.placement,
        confidence: result.confidence,
        board: result.board,
        detections: result.detections,
        turn: result.turn,
    }))
}

#[derive(Serialize)]
pub struct DatabaseInfo {
    pub name: String,
    pub path: String,
    pub current: bool,
}

fn canonical(path: &std::path::Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

pub async fn databases(
    State(state): State<AppState>,
) -> Result<Json<Vec<DatabaseInfo>>, ApiError> {
    let current = canonical(std::path::Path::new(&state.db.path()));
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&state.db_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("db") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            out.push(DatabaseInfo {
                name: name.to_owned(),
                path: canonical(&path),
                current: canonical(&path) == current,
            });
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct DatabaseBody {
    pub name: String,
}

fn database_path(state: &AppState, name: &str) -> Result<std::path::PathBuf, ApiError> {
    let name = name.trim();
    let safe = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ' ');
    if !safe {
        return Err(ApiError::Internal(anyhow::anyhow!(
            "database names may use letters, digits, spaces, '-' and '_'"
        )));
    }
    Ok(state.db_dir.join(format!("{name}.db")))
}

pub async fn create_database(
    State(state): State<AppState>,
    Json(body): Json<DatabaseBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let path = database_path(&state, &body.name)?;
    state.db.switch_to(&path)?;
    Ok(Json(serde_json::json!({
        "name": body.name.trim(),
        "path": path.display().to_string(),
    })))
}

pub async fn switch_database(
    State(state): State<AppState>,
    Json(body): Json<DatabaseBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let path = database_path(&state, &body.name)?;
    if !path.exists() {
        return Err(ApiError::NotFound);
    }
    state.db.switch_to(&path)?;
    Ok(Json(serde_json::json!({
        "name": body.name.trim(),
        "path": path.display().to_string(),
    })))
}

#[derive(Serialize, Clone)]
pub struct SettingsResponse {
    pub engine_path: Option<String>,
    pub model_url: Option<String>,
    pub depth: u32,
    pub theme: String,
    pub pieces: String,
    pub board_light: String,
    pub board_dark: String,
    pub show_coords: bool,
    pub language: String,
    pub threads: u32,
    pub hash_mb: u32,
    pub syzygy_path: Option<String>,
    pub variant_engine_path: Option<String>,
    pub online_explorer: bool,
    pub animate_last_move: bool,
    pub sound_enabled: bool,
    pub sound_theme: String,
    pub sound_volume: u32,
    pub piece_animation_ms: u32,
    pub puzzle_rating: i32,
    pub profile_name: String,
    pub profile_player: Option<String>,
    pub keybinds: String,
    pub engines: Vec<EngineEntry>,
}

fn effective(state: &AppState) -> SettingsResponse {
    let stored = state.db.settings().unwrap_or_default();
    let get = |k: &str| stored.get(k).cloned().filter(|v| !v.trim().is_empty());
    SettingsResponse {
        engine_path: get("engine_path").or_else(|| state.engine_path.clone()),
        model_url: get("model_url").or_else(|| state.model_url.clone()),
        depth: get("depth")
            .and_then(|d| d.parse().ok())
            .unwrap_or(state.default_depth),
        theme: get("theme").unwrap_or_else(|| "green".into()),
        pieces: get("pieces").unwrap_or_else(|| "cburnett".into()),
        board_light: get("board_light").unwrap_or_else(|| "#eeeed2".into()),
        board_dark: get("board_dark").unwrap_or_else(|| "#769656".into()),
        show_coords: get("show_coords").as_deref() != Some("false"),
        language: get("language").unwrap_or_else(|| "en".into()),
        threads: get("threads").and_then(|v| v.parse().ok()).unwrap_or(0),
        hash_mb: get("hash_mb").and_then(|v| v.parse().ok()).unwrap_or(0),
        syzygy_path: get("syzygy_path"),
        variant_engine_path: get("variant_engine_path"),
        online_explorer: get("online_explorer").as_deref() == Some("true"),
        animate_last_move: get("animate_last_move").as_deref() == Some("true"),
        sound_enabled: get("sound_enabled").as_deref() != Some("false"),
        sound_theme: get("sound_theme").unwrap_or_else(|| "sfx".into()),
        sound_volume: get("sound_volume")
            .and_then(|v| v.parse().ok())
            .unwrap_or(70),
        piece_animation_ms: get("piece_animation_ms")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        puzzle_rating: get("puzzle_rating")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1500),
        profile_name: get("profile_name").unwrap_or_else(|| "Player".into()),
        profile_player: get("profile_player"),
        keybinds: get("keybinds").unwrap_or_else(|| {
            r#"{"previous":"ArrowLeft","next":"ArrowRight","start":"Home","end":"End","flip":"f"}"#
                .into()
        }),
        engines: get("engines")
            .and_then(|json| serde_json::from_str::<Vec<EngineEntry>>(&json).ok())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| {
                state
                    .engine_path
                    .clone()
                    .map(|path| {
                        vec![EngineEntry {
                            name: "Stockfish".into(),
                            path,
                            options: BTreeMap::new(),
                        }]
                    })
                    .unwrap_or_default()
            }),
    }
}

fn canonical_variant(name: &str) -> String {
    match name.trim().to_ascii_lowercase().as_str() {
        "" | "standard" | "chess" | "from position" => "standard".into(),
        "chess960" | "chess 960" | "fischer random" => "chess960".into(),
        "king of the hill" | "kingofthehill" => "kingofthehill".into(),
        "three-check" | "three check" | "threecheck" => "3check".into(),
        other => other.to_owned(),
    }
}

fn variant_uci(name: &str) -> Option<&'static str> {
    match name {
        "atomic" => Some("atomic"),
        "antichess" => Some("antichess"),
        "kingofthehill" => Some("kingofthehill"),
        "3check" => Some("3check"),
        "crazyhouse" => Some("crazyhouse"),
        "racingkings" => Some("racingkings"),
        "horde" => Some("horde"),
        _ => None,
    }
}

fn apply_entry_options(
    options: &mut knightlm_engine::EngineOptions,
    entry: Option<&EngineEntry>,
) {
    if let Some(entry) = entry {
        for (name, value) in &entry.options {
            if value.trim().is_empty() {
                continue;
            }
            options.extra.push((name.clone(), value.clone()));
        }
    }
}

fn engine_options(
    settings: &SettingsResponse,
    fen: &str,
    variant: &str,
) -> knightlm_engine::EngineOptions {
    let canonical = canonical_variant(variant);
    let chess960 = canonical == "chess960"
        || knightlm_core::state::parse_chess(fen)
            .map(|(_, mode)| mode.is_chess960())
            .unwrap_or(false);
    knightlm_engine::EngineOptions {
        threads: (settings.threads > 0).then_some(settings.threads),
        hash_mb: (settings.hash_mb > 0).then_some(settings.hash_mb),
        syzygy_path: settings.syzygy_path.clone(),
        multipv: None,
        chess960,
        variant: variant_uci(&canonical).map(str::to_owned),
        extra: Vec::new(),
    }
}

fn engine_for(
    settings: &SettingsResponse,
    fen: &str,
    variant: &str,
) -> Result<(String, knightlm_engine::EngineOptions), ApiError> {
    let canonical = canonical_variant(variant);
    let path = if variant_uci(&canonical).is_some() {
        settings
            .variant_engine_path
            .clone()
            .ok_or(ApiError::Unavailable(
                "configure a variant-capable engine (e.g. Fairy-Stockfish)",
            ))?
    } else {
        settings
            .engine_path
            .clone()
            .ok_or(ApiError::Unavailable("engine path is not set"))?
    };
    let mut options = engine_options(settings, fen, &canonical);
    let entry = settings.engines.iter().find(|e| e.path == path);
    apply_entry_options(&mut options, entry);
    Ok((path, options))
}

pub async fn get_settings(State(state): State<AppState>) -> Json<SettingsResponse> {
    Json(effective(&state))
}

#[derive(Deserialize)]
pub struct SettingsInput {
    #[serde(default)]
    pub engine_path: Option<String>,
    #[serde(default)]
    pub model_url: Option<String>,
    #[serde(default)]
    pub depth: Option<u32>,
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub pieces: Option<String>,
    #[serde(default)]
    pub board_light: Option<String>,
    #[serde(default)]
    pub board_dark: Option<String>,
    #[serde(default)]
    pub show_coords: Option<bool>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub threads: Option<u32>,
    #[serde(default)]
    pub hash_mb: Option<u32>,
    #[serde(default)]
    pub syzygy_path: Option<String>,
    #[serde(default)]
    pub variant_engine_path: Option<String>,
    #[serde(default)]
    pub online_explorer: Option<bool>,
    #[serde(default)]
    pub animate_last_move: Option<bool>,
    #[serde(default)]
    pub sound_enabled: Option<bool>,
    #[serde(default)]
    pub sound_theme: Option<String>,
    #[serde(default)]
    pub sound_volume: Option<u32>,
    #[serde(default)]
    pub piece_animation_ms: Option<u32>,
    #[serde(default)]
    pub profile_name: Option<String>,
    #[serde(default)]
    pub profile_player: Option<String>,
    #[serde(default)]
    pub keybinds: Option<String>,
    #[serde(default)]
    pub engines: Option<Vec<EngineEntry>>,
}

pub async fn put_settings(
    State(state): State<AppState>,
    Json(body): Json<SettingsInput>,
) -> Result<Json<SettingsResponse>, ApiError> {
    if let Some(v) = body.engine_path {
        state.db.set_setting("engine_path", v.trim())?;
    }
    if let Some(v) = body.model_url {
        state.db.set_setting("model_url", v.trim())?;
    }
    if let Some(d) = body.depth {
        state.db.set_setting("depth", &d.to_string())?;
    }
    if let Some(v) = body.theme {
        state.db.set_setting("theme", v.trim())?;
    }
    if let Some(v) = body.pieces {
        state.db.set_setting("pieces", v.trim())?;
    }
    if let Some(v) = body.board_light {
        state.db.set_setting("board_light", v.trim())?;
    }
    if let Some(v) = body.board_dark {
        state.db.set_setting("board_dark", v.trim())?;
    }
    if let Some(v) = body.show_coords {
        state
            .db
            .set_setting("show_coords", if v { "true" } else { "false" })?;
    }
    if let Some(v) = body.language {
        state.db.set_setting("language", v.trim())?;
    }
    if let Some(v) = body.threads {
        state.db.set_setting("threads", &v.to_string())?;
    }
    if let Some(v) = body.hash_mb {
        state.db.set_setting("hash_mb", &v.to_string())?;
    }
    if let Some(v) = body.syzygy_path {
        state.db.set_setting("syzygy_path", v.trim())?;
    }
    if let Some(v) = body.variant_engine_path {
        state.db.set_setting("variant_engine_path", v.trim())?;
    }
    if let Some(v) = body.online_explorer {
        state.db.set_setting("online_explorer", if v { "true" } else { "false" })?;
    }
    if let Some(v) = body.animate_last_move {
        state.db
            .set_setting("animate_last_move", if v { "true" } else { "false" })?;
    }
    if let Some(v) = body.sound_enabled {
        state.db
            .set_setting("sound_enabled", if v { "true" } else { "false" })?;
    }
    if let Some(v) = body.sound_theme {
        state.db.set_setting("sound_theme", v.trim())?;
    }
    if let Some(v) = body.sound_volume {
        state.db
            .set_setting("sound_volume", &v.min(100).to_string())?;
    }
    if let Some(v) = body.piece_animation_ms {
        state.db
            .set_setting("piece_animation_ms", &v.min(1000).to_string())?;
    }
    if let Some(v) = body.profile_name {
        state.db.set_setting("profile_name", v.trim())?;
    }
    if let Some(v) = body.profile_player {
        state.db.set_setting("profile_player", v.trim())?;
    }
    if let Some(v) = body.keybinds {
        let parsed: serde_json::Value = serde_json::from_str(&v)
            .map_err(|e| ApiError::BadRequest(format!("keybinds: {e}")))?;
        state.db.set_setting("keybinds", &parsed.to_string())?;
    }
    if let Some(v) = body.engines {
        state.db.set_setting("engines", &serde_json::to_string(&v)?)?;
    }
    Ok(Json(effective(&state)))
}

#[derive(Deserialize)]
pub struct LinesQuery {
    pub fen: String,
    #[serde(default)]
    pub variant: Option<String>,
    #[serde(default)]
    pub depth: Option<u32>,
    #[serde(default)]
    pub lines: Option<u32>,
}

pub async fn analyze_lines(
    State(state): State<AppState>,
    Query(q): Query<LinesQuery>,
) -> Result<Json<Vec<knightlm_engine::Analysis>>, ApiError> {
    let settings = effective(&state);
    let depth = q.depth.unwrap_or(settings.depth).clamp(1, 30);
    let lines = q.lines.unwrap_or(3).clamp(1, 8);
    let fen = q.fen;
    let variant = q.variant.unwrap_or_else(|| "standard".into());
    let (path, options) = engine_for(&settings, &fen, &variant)?;
    let out = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let mut engine = knightlm_engine::Engine::spawn_with(&path, 60, options)?;
        engine.analyse_lines(&fen, depth, lines)
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))??;
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct ProfileQuery {
    pub player: String,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Serialize)]
pub struct Profile {
    pub name: String,
    pub games: i64,
    pub as_white: i64,
    pub as_black: i64,
    pub wins: i64,
    pub draws: i64,
    pub losses: i64,
    pub unfinished: i64,
    pub score_pct: f32,
    pub recent: Vec<rbc_db::Game>,
}

pub async fn profile(
    State(state): State<AppState>,
    Query(q): Query<ProfileQuery>,
) -> Result<Json<Profile>, ApiError> {
    let name = q.player.trim().to_owned();
    if name.is_empty() {
        return Err(ApiError::BadRequest("player is required".into()));
    }
    let query = rbc_db::GameQuery {
        player: Some(name.clone()),
        sort: Some("newest".into()),
        limit: Some(q.limit.unwrap_or(12).clamp(1, 100)),
        ..Default::default()
    };
    let all = state.db.query(&rbc_db::GameQuery {
        player: Some(name.clone()),
        limit: Some(100_000),
        ..Default::default()
    })?;
    let mut out = Profile {
        name,
        games: all.total,
        as_white: 0,
        as_black: 0,
        wins: 0,
        draws: 0,
        losses: 0,
        unfinished: 0,
        score_pct: 0.0,
        recent: state.db.query(&query)?.games,
    };
    for game in &all.games {
        let white = game.white.eq_ignore_ascii_case(&out.name);
        if white {
            out.as_white += 1;
        } else if game.black.eq_ignore_ascii_case(&out.name) {
            out.as_black += 1;
        }
        match game.result.as_str() {
            "1-0" if white => out.wins += 1,
            "0-1" if !white => out.wins += 1,
            "1/2-1/2" => out.draws += 1,
            "*" => out.unfinished += 1,
            _ => out.losses += 1,
        }
    }
    let decided = out.wins + out.draws + out.losses;
    if decided > 0 {
        out.score_pct = (out.wins as f32 + out.draws as f32 * 0.5) / decided as f32 * 100.0;
    }
    Ok(Json(out))
}

pub async fn list_games(
    State(state): State<AppState>,
    Query(q): Query<GameQuery>,
) -> Result<Json<QueryResponse>, ApiError> {
    Ok(Json(state.db.query(&q)?))
}

#[derive(Deserialize)]
pub struct ImportBody {
    pub pgn: String,
    #[serde(default)]
    pub collection_id: Option<i64>,
}

pub async fn import_game(
    State(state): State<AppState>,
    Json(body): Json<ImportBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let ids = state.db.import_pgn(&body.pgn, body.collection_id)?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "imported": ids }))))
}

#[derive(Deserialize)]
pub struct OnlineImport {
    pub username: String,
    #[serde(default)]
    pub max: Option<u32>,
    #[serde(default)]
    pub months: Option<u32>,
}

fn valid_username(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn fetch_lichess_pgn(username: &str, max: u32) -> anyhow::Result<String> {
    let url = format!(
        "https://lichess.org/api/games/user/{username}?max={}&pgnInJson=false&clocks=false&evals=false",
        max.clamp(1, 200)
    );
    let agent = ureq::Agent::new_with_defaults();
    let mut resp = agent
        .get(&url)
        .header("Accept", "application/x-chess-pgn")
        .call()?;
    Ok(resp.body_mut().read_to_string()?)
}

fn fetch_chesscom_pgn(username: &str, months: u32) -> anyhow::Result<String> {
    let agent = ureq::Agent::new_with_defaults();
    let archives_url =
        format!("https://api.chess.com/pub/player/{username}/games/archives");
    let mut resp = agent.get(&archives_url).call()?;
    let v: serde_json::Value = resp.body_mut().read_json()?;
    let urls: Vec<String> = v
        .get("archives")
        .and_then(|a| a.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|u| u.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let take = months.clamp(1, 12) as usize;
    let mut out = String::new();
    for url in urls.iter().rev().take(take) {
        let mut resp = agent.get(&format!("{url}/pgn")).call()?;
        out.push_str(&resp.body_mut().read_to_string()?);
        out.push_str("\n\n");
    }
    Ok(out)
}

pub async fn import_lichess(
    State(state): State<AppState>,
    Json(body): Json<OnlineImport>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !valid_username(body.username.trim()) {
        return Err(ApiError::Internal(anyhow::anyhow!("invalid username")));
    }
    let username = body.username.trim().to_owned();
    let max = body.max.unwrap_or(30);
    let pgn = tokio::task::spawn_blocking(move || fetch_lichess_pgn(&username, max))
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))??;
    let (ids, skipped) = state.db.import_pgn_lenient(&pgn, None)?;
    Ok(Json(serde_json::json!({ "imported": ids.len(), "skipped": skipped })))
}

pub async fn import_chesscom(
    State(state): State<AppState>,
    Json(body): Json<OnlineImport>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !valid_username(body.username.trim()) {
        return Err(ApiError::Internal(anyhow::anyhow!("invalid username")));
    }
    let username = body.username.trim().to_owned();
    let months = body.months.or(body.max).unwrap_or(1);
    let pgn = tokio::task::spawn_blocking(move || fetch_chesscom_pgn(&username, months))
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))??;
    let (ids, skipped) = state.db.import_pgn_lenient(&pgn, None)?;
    Ok(Json(serde_json::json!({ "imported": ids.len(), "skipped": skipped })))
}

#[derive(Serialize)]
pub struct EngineAnalysis {
    pub name: String,
    pub path: String,
    pub lines: Vec<knightlm_engine::Analysis>,
    pub error: Option<String>,
}

pub async fn analyze_engines(
    State(state): State<AppState>,
    Query(q): Query<LinesQuery>,
) -> Result<Json<Vec<EngineAnalysis>>, ApiError> {
    let settings = effective(&state);
    let engines: Vec<EngineEntry> = settings.engines.iter().take(4).cloned().collect();
    if engines.is_empty() {
        return Err(ApiError::Unavailable("no engines are configured"));
    }
    let depth = q.depth.unwrap_or(settings.depth).clamp(1, 30);
    let lines = q.lines.unwrap_or(3).clamp(1, 8);
    let fen = q.fen;
    let variant = canonical_variant(q.variant.as_deref().unwrap_or("standard"));
    if variant_uci(&variant).is_some() && settings.variant_engine_path.is_none() {
        return Err(ApiError::Unavailable(
            "configure a variant-capable engine (e.g. Fairy-Stockfish)",
        ));
    }

    let engines: Vec<EngineEntry> = if variant_uci(&variant).is_some() {
        vec![EngineEntry {
            name: "Variant engine".into(),
            path: settings.variant_engine_path.clone().unwrap_or_default(),
            options: BTreeMap::new(),
        }]
    } else {
        settings.engines.iter().take(4).cloned().collect()
    };
    let out = tokio::task::spawn_blocking(move || {
        std::thread::scope(|scope| {
            let handles: Vec<_> = engines
                .iter()
                .map(|engine| {
                    let fen = fen.clone();
                    let mut options = engine_options(&settings, &fen, &variant);
                    apply_entry_options(&mut options, Some(engine));
                    scope.spawn(move || {
                        let result = knightlm_engine::Engine::spawn_with(
                            &engine.path, 30, options,
                        )
                        .and_then(|mut e| e.analyse_lines(&fen, depth, lines));
                        (engine.clone(), result)
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| {
                    let (engine, result) = handle.join().expect("engine thread");
                    match result {
                        Ok(lines) => EngineAnalysis {
                            name: engine.name,
                            path: engine.path,
                            lines,
                            error: None,
                        },
                        Err(e) => EngineAnalysis {
                            name: engine.name,
                            path: engine.path,
                            lines: Vec::new(),
                            error: Some(e.to_string()),
                        },
                    }
                })
                .collect::<Vec<_>>()
        })
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))?;
    Ok(Json(out))
}

pub async fn get_game(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Game>, ApiError> {
    state.db.get(id)?.map(Json).ok_or(ApiError::NotFound)
}

pub async fn update_game(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(tags): Json<GameTags>,
) -> Result<Json<Game>, ApiError> {
    if !state.db.update_tags(id, &tags)? {
        return Err(ApiError::NotFound);
    }
    state.db.get(id)?.map(Json).ok_or(ApiError::NotFound)
}

#[derive(Deserialize)]
pub struct MovesBody {
    pub moves: String,
}

#[derive(Deserialize)]
pub struct TreeBody {
    pub pgn: String,
}

#[derive(Deserialize)]
pub struct PathBody {
    pub path: String,
}

#[derive(Deserialize)]
pub struct AddMoveBody {
    pub parent: String,
    pub san: String,
    #[serde(default)]
    pub clock: Option<String>,
}

pub async fn add_tree_move(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<AddMoveBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (game, path) =
        state
            .db
            .add_tree_move(id, &body.parent, &body.san, body.clock.as_deref())?;
    Ok(Json(serde_json::json!({ "game": game, "path": path })))
}

pub async fn update_tree(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<TreeBody>,
) -> Result<Json<Game>, ApiError> {
    if !state.db.update_pgn_tree(id, &body.pgn)? {
        return Err(ApiError::NotFound);
    }
    state.db.get(id)?.map(Json).ok_or(ApiError::NotFound)
}

pub async fn delete_tree_node(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<PathBody>,
) -> Result<Json<Game>, ApiError> {
    if !state.db.delete_tree_node(id, &body.path)? {
        return Err(ApiError::NotFound);
    }
    state.db.get(id)?.map(Json).ok_or(ApiError::NotFound)
}

pub async fn promote_variation(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<PathBody>,
) -> Result<Json<Game>, ApiError> {
    if !state.db.promote_variation(id, &body.path)? {
        return Err(ApiError::NotFound);
    }
    state.db.get(id)?.map(Json).ok_or(ApiError::NotFound)
}

pub async fn update_moves(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<MovesBody>,
) -> Result<Json<Game>, ApiError> {
    if !state.db.update_moves(id, &body.moves)? {
        return Err(ApiError::NotFound);
    }
    state.db.get(id)?.map(Json).ok_or(ApiError::NotFound)
}

#[derive(Deserialize)]
pub struct TruncateBody {
    pub ply: usize,
}

pub async fn truncate_moves(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<TruncateBody>,
) -> Result<Json<Game>, ApiError> {
    let game = state.db.get(id)?.ok_or(ApiError::NotFound)?;
    let kept: Vec<&str> = game.moves.split_whitespace().take(body.ply).collect();
    if !state.db.update_moves(id, &kept.join(" "))? {
        return Err(ApiError::NotFound);
    }
    state.db.get(id)?.map(Json).ok_or(ApiError::NotFound)
}

pub async fn game_pgn(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    let pgn = state.db.export_pgn(id)?.ok_or(ApiError::NotFound)?;
    Ok((
        [(header::CONTENT_TYPE, "application/x-chess-pgn")],
        pgn,
    )
        .into_response())
}

pub async fn delete_game(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    if state.db.delete_game(id)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

pub async fn game_evals(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(q): Query<ReportQuery>,
) -> Result<Json<Vec<rbc_db::EvalRow>>, ApiError> {
    let rows = match q.depth {
        Some(depth) => state.db.evals(id, depth)?,
        None => state.db.evals_latest(id)?,
    };
    Ok(Json(rows))
}

pub async fn collections(State(state): State<AppState>) -> Result<Json<Vec<Collection>>, ApiError> {
    Ok(Json(state.db.collections()?))
}

#[derive(Deserialize)]
pub struct CreateCollection {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

pub async fn create_collection(
    State(state): State<AppState>,
    Json(body): Json<CreateCollection>,
) -> Result<(StatusCode, Json<Collection>), ApiError> {
    let c = state
        .db
        .create_collection(&body.name, body.description.as_deref())?;
    Ok((StatusCode::CREATED, Json(c)))
}

#[derive(Deserialize)]
pub struct RenameCollection {
    pub name: String,
}

pub async fn rename_collection(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<RenameCollection>,
) -> Result<StatusCode, ApiError> {
    if state.db.rename_collection(id, &body.name)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

pub async fn delete_collection(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    if state.db.delete_collection(id)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

pub async fn add_to_collection(
    State(state): State<AppState>,
    Path((collection_id, game_id)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    state.db.add_to_collection(collection_id, game_id)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn players(State(state): State<AppState>) -> Result<Json<Vec<PlayerRow>>, ApiError> {
    Ok(Json(state.db.players()?))
}

pub async fn events(State(state): State<AppState>) -> Result<Json<Vec<EventRow>>, ApiError> {
    Ok(Json(state.db.events()?))
}

#[derive(Deserialize)]
pub struct PositionQuery {
    pub fen: String,
    #[serde(default)]
    pub variant: Option<String>,
    #[serde(default)]
    pub partial: bool,
}

pub async fn positions(
    State(state): State<AppState>,
    Query(q): Query<PositionQuery>,
) -> Result<Json<Vec<Game>>, ApiError> {
    let variant = canonical_variant(q.variant.as_deref().unwrap_or("standard"));
    let games = if q.partial {
        state.db.search_position_partial(&q.fen)?
    } else if variant == "standard" || variant == "chess960" {
        state.db.search_position(&q.fen)?
    } else {
        state.db.search_position_variant(&q.fen, &variant)?
    };
    Ok(Json(games))
}

pub async fn annotations(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<Annotation>>, ApiError> {
    Ok(Json(state.db.annotations(id)?))
}

pub async fn add_annotation(
    State(state): State<AppState>,
    Json(input): Json<AnnotationInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let id = state.db.add_annotation(&input)?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

#[derive(Deserialize)]
pub struct OpeningQuery {
    pub fen: String,
    #[serde(default)]
    pub variant: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
}

fn parse_lichess_explorer(body: &str) -> Option<rbc_db::Explorer> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let opening = v.get("opening");
    Some(rbc_db::Explorer {
        eco: opening
            .and_then(|o| o.get("eco"))
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        name: opening
            .and_then(|o| o.get("name"))
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        continuations: v
            .get("moves")?
            .as_array()?
            .iter()
            .map(|m| {
                let num = |k: &str| m.get(k).and_then(|v| v.as_i64()).unwrap_or(0);
                rbc_db::Continuation {
                    uci: m
                        .get("uci")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_owned(),
                    san: m
                        .get("san")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_owned(),
                    games: num("white") + num("draws") + num("black"),
                    white: num("white"),
                    draw: num("draws"),
                    black: num("black"),
                }
            })
            .collect(),
    })
}

fn fetch_lichess_explorer(fen: &str, source: &str) -> anyhow::Result<String> {
    let encoded = fen.replace(' ', "%20");
    let url = if source == "masters" {
        format!("https://explorer.lichess.ovh/masters?fen={encoded}&moves=12")
    } else {
        format!(
            "https://explorer.lichess.ovh/lichess?fen={encoded}&speeds=blitz,rapid,classical&ratings=1600,1800,2000,2200&moves=12"
        )
    };
    let agent = ureq::Agent::new_with_defaults();
    let mut resp = agent.get(&url).call()?;
    Ok(resp.body_mut().read_to_string()?)
}

pub async fn openings(
    State(state): State<AppState>,
    Query(q): Query<OpeningQuery>,
) -> Result<Json<rbc_db::Explorer>, ApiError> {
    if let Some(source) = q
        .source
        .as_deref()
        .filter(|s| *s == "lichess" || *s == "masters")
    {
        if let Some(body) = state.db.cached_explorer(source, &q.fen)? {
            if let Some(e) = parse_lichess_explorer(&body) {
                return Ok(Json(e));
            }
        }
        let fen = q.fen.clone();
        let source = source.to_owned();
        let db = state.db.clone();
        let fetched = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
            let body = fetch_lichess_explorer(&fen, &source)?;
            db.cache_explorer(&source, &fen, &body)?;
            Ok(body)
        })
        .await;
        if let Ok(Ok(body)) = fetched {
            if let Some(e) = parse_lichess_explorer(&body) {
                return Ok(Json(e));
            }
        }
        // Offline or malformed upstream: fall back to the local database.
    }
    Ok(Json(state.db.explore(&q.fen)?))
}

#[derive(Deserialize)]
pub struct PuzzleQuery {
    #[serde(default)]
    pub rating: Option<i32>,
    #[serde(default)]
    pub theme: Option<String>,
}

pub async fn next_puzzle(
    State(state): State<AppState>,
    Query(q): Query<PuzzleQuery>,
) -> Result<Json<Option<rbc_db::Puzzle>>, ApiError> {
    let rating = q.rating.or(Some(effective(&state).puzzle_rating));
    Ok(Json(state.db.next_puzzle(rating, q.theme.as_deref())?))
}

pub async fn get_puzzle(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<rbc_db::Puzzle>, ApiError> {
    state.db.get_puzzle(&id)?.map(Json).ok_or(ApiError::NotFound)
}

#[derive(Serialize)]
pub struct ThemeCount {
    pub theme: String,
    pub count: i64,
}

pub async fn puzzle_themes(
    State(state): State<AppState>,
) -> Result<Json<Vec<ThemeCount>>, ApiError> {
    let themes = state
        .db
        .puzzle_themes()?
        .into_iter()
        .map(|(theme, count)| ThemeCount { theme, count })
        .collect();
    Ok(Json(themes))
}

#[derive(Deserialize)]
pub struct PuzzleAttemptBody {
    pub puzzle_id: String,
    pub puzzle_rating: i32,
    pub solved: bool,
}

pub async fn puzzle_attempt(
    State(state): State<AppState>,
    Json(body): Json<PuzzleAttemptBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (before, after) =
        state
            .db
            .record_puzzle_attempt(&body.puzzle_id, body.puzzle_rating, body.solved)?;
    Ok(Json(serde_json::json!({
        "rating_before": before,
        "rating_after": after,
    })))
}

#[derive(Deserialize)]
pub struct PuzzleImport {
    pub csv: String,
}

pub async fn import_puzzles(
    State(state): State<AppState>,
    Json(body): Json<PuzzleImport>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let n = state.db.import_puzzles_csv(&body.csv)?;
    Ok(Json(serde_json::json!({ "imported": n })))
}

#[derive(Deserialize)]
pub struct ReportQuery {
    #[serde(default)]
    pub depth: Option<u32>,
}

fn fen_of<P: Position>(pos: &P) -> String {
    Fen::from_position(pos, EnPassantMode::Legal).to_string()
}

fn to_eval(a: &knightlm_engine::Analysis) -> Eval {
    if let Some(m) = a.mate {
        Eval::Mate(m)
    } else {
        Eval::Cp(a.score_cp.unwrap_or(0))
    }
}

fn negate(e: Eval) -> Eval {
    match e {
        Eval::Cp(c) => Eval::Cp(-c),
        Eval::Mate(m) => Eval::Mate(-m),
    }
}

/// Replay a stored game, evaluate every position with Stockfish, and classify
/// the moves with Lichess labels + clean-room Expected Points V2.
pub async fn game_report(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(q): Query<ReportQuery>,
) -> Result<Json<ReportResponse>, ApiError> {
    let game = state.db.get(id)?.ok_or(ApiError::NotFound)?;
    let settings = effective(&state);
    let variant = game.variant.clone().unwrap_or_else(|| "standard".into());
    let depth = q.depth.unwrap_or(settings.depth).clamp(1, 20);
    let (path, options) = engine_for(&settings, &game.fen, &variant)?;
    let (report, eval_rows) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let canonical = canonical_variant(&variant);
        let variant_type = knightlm_core::state::variant_for_name(&canonical);
        let mode = if variant_type == shakmaty::variant::Variant::Chess {
            if canonical == "chess960" {
                shakmaty::CastlingMode::Chess960
            } else {
                shakmaty::CastlingMode::Standard
            }
        } else {
            shakmaty::CastlingMode::Standard
        };
        let (mut pos, mode) =
            knightlm_core::state::parse_variant_fen(&game.fen, variant_type, mode)?;
        let sans: Vec<String> = game.moves.split_whitespace().map(str::to_owned).collect();
        let mut ucis = Vec::new();
        let mut fens = vec![fen_of(&pos)];
        for san in &sans {
            let mv = SanPlus::from_ascii(san.as_bytes())?.san.to_move(&pos)?;
            ucis.push(UciMove::from_move(mv, mode).to_string());
            pos = pos.play(mv)?;
            fens.push(fen_of(&pos));
        }
        let mut engine = knightlm_engine::Engine::spawn_with(&path, 120, options)?;
        let mut evals = Vec::with_capacity(fens.len());
        for f in &fens {
            evals.push(engine.analyse(f, depth)?);
        }
        let eval_rows: Vec<(i64, Option<i32>, Option<i32>)> = evals
            .iter()
            .enumerate()
            .map(|(i, a)| (i as i64, a.score_cp, a.mate))
            .collect();
        let mut inputs = Vec::with_capacity(sans.len());
        for i in 0..ucis.len() {
            inputs.push(MoveInput {
                san: sans[i].clone(),
                uci: ucis[i].clone(),
                eval_before: to_eval(&evals[i]),
                // The eval after the move is from the opponent's point of view.
                eval_after: negate(to_eval(&evals[i + 1])),
                best_move: Some(evals[i].best_move.clone()),
            });
        }
        Ok((
            brick_chess_analysis::analyze(&inputs, &AnalysisConfig::default()),
            eval_rows,
        ))
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))??;
    state.db.save_evals(id, depth, &eval_rows)?;
    Ok(Json(ReportResponse { report }))
}

#[derive(Deserialize)]
pub struct AnalyzeBody {
    pub fen: String,
    #[serde(default)]
    pub variant: Option<String>,
    #[serde(default)]
    pub depth: Option<u32>,
}

pub async fn analyze(
    State(state): State<AppState>,
    Json(body): Json<AnalyzeBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let settings = effective(&state);
    let depth = body.depth.unwrap_or(settings.depth).clamp(1, 30);
    let fen = body.fen;
    let variant = body.variant.unwrap_or_else(|| "standard".into());
    let (path, options) = engine_for(&settings, &fen, &variant)?;
    let analysis = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let mut engine = knightlm_engine::Engine::spawn_with(&path, 60, options)?;
        engine.analyse(&fen, depth)
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))??;
    Ok(Json(serde_json::to_value(analysis)?))
}

#[derive(Deserialize)]
pub struct EngineMoveBody {
    pub fen: String,
    pub engine: String,
    #[serde(default)]
    pub depth: Option<u32>,
    #[serde(default)]
    pub variant: Option<String>,
}

#[derive(Serialize)]
pub struct EngineMove {
    pub best_move: String,
    pub score_cp: Option<i32>,
    pub mate: Option<i32>,
    pub pv: Vec<String>,
    pub game_over: Option<String>,
}

fn game_over_for(fen: &str) -> Option<String> {
    let (pos, _) = knightlm_core::state::parse_chess(fen).ok()?;
    if pos.is_checkmate() {
        Some("checkmate".into())
    } else if pos.is_stalemate() {
        Some("stalemate".into())
    } else if pos.is_insufficient_material() {
        Some("insufficient material".into())
    } else if pos.legal_moves().is_empty() {
        Some("no legal moves".into())
    } else {
        None
    }
}

/// One move from a specific configured engine (engine-vs-engine matches).
pub async fn analyze_engine(
    State(state): State<AppState>,
    Json(body): Json<EngineMoveBody>,
) -> Result<Json<EngineMove>, ApiError> {
    let settings = effective(&state);
    let depth = body.depth.unwrap_or(settings.depth).clamp(1, 30);
    let variant = body.variant.unwrap_or_else(|| "standard".into());
    let entry = settings
        .engines
        .iter()
        .find(|e| e.path == body.engine || e.name == body.engine)
        .cloned()
        .ok_or_else(|| ApiError::BadRequest("unknown engine".into()))?;
    let game_over = game_over_for(&body.fen);
    if game_over.is_some() {
        return Ok(Json(EngineMove {
            best_move: String::new(),
            score_cp: None,
            mate: None,
            pv: Vec::new(),
            game_over,
        }));
    }
    let mut options = engine_options(&settings, &body.fen, &variant);
    apply_entry_options(&mut options, Some(&entry));
    let fen = body.fen.clone();
    let lines = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let mut engine = knightlm_engine::Engine::spawn_with(&entry.path, 60, options)?;
        engine.analyse_lines(&fen, depth, 1)
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))??;
    let best = lines.into_iter().next().ok_or_else(|| {
        ApiError::Internal(anyhow::anyhow!("engine returned no analysis"))
    })?;
    Ok(Json(EngineMove {
        best_move: best.best_move,
        score_cp: best.score_cp,
        mate: best.mate,
        pv: best.pv,
        game_over,
    }))
}

#[derive(Deserialize)]
pub struct ReportBody {
    pub moves: Vec<brick_chess_analysis::MoveInput>,
    #[serde(default)]
    pub config: Option<brick_chess_analysis::AnalysisConfig>,
}

#[derive(Serialize)]
pub struct ReportResponse {
    pub report: brick_chess_analysis::GameReport,
}

pub async fn report(Json(body): Json<ReportBody>) -> Json<ReportResponse> {
    let config = body.config.unwrap_or_default();
    Json(ReportResponse {
        report: brick_chess_analysis::analyze(&body.moves, &config),
    })
}

static TABLEBASE: OnceLock<Mutex<Option<(String, Tablebase<Chess>)>>> = OnceLock::new();

#[derive(Serialize)]
pub struct TablebaseReply {
    pub wdl: String,
    pub best_move: Option<String>,
    pub pieces: usize,
    pub path: String,
}

pub async fn tablebase(
    State(state): State<AppState>,
    Query(q): Query<OpeningQuery>,
) -> Result<Json<TablebaseReply>, ApiError> {
    let path = effective(&state)
        .syzygy_path
        .ok_or(ApiError::Unavailable("syzygy path is not set"))?;
    let fen = q.fen;
    let variant_name = q.variant.as_deref().unwrap_or("standard");
    if canonical_variant(variant_name) != "standard" {
        return Err(ApiError::Unavailable("tablebases are only supported for standard chess"));
    }
    let out = tokio::task::spawn_blocking(move || -> anyhow::Result<TablebaseReply> {
        let mut guard = TABLEBASE
            .get_or_init(|| Mutex::new(None))
            .lock()
            .expect("tablebase lock");
        let needs_load = guard
            .as_ref()
            .map(|(loaded, _)| loaded != &path)
            .unwrap_or(true);
        if needs_load {
            let mut tb = Tablebase::new();
            tb.add_directory(&path)?;
            *guard = Some((path.clone(), tb));
        }
        let tb = &guard.as_ref().expect("tablebase").1;
        let pos: Chess = Fen::from_ascii(fen.as_bytes())?
            .into_position(CastlingMode::Standard)?;
        let pieces = pos.board().occupied().count();
        let wdl = tb.probe_wdl_after_zeroing(&pos)?;
        let wdl = match wdl {
            shakmaty_syzygy::Wdl::Win => "win",
            shakmaty_syzygy::Wdl::CursedWin => "cursed-win",
            shakmaty_syzygy::Wdl::Draw => "draw",
            shakmaty_syzygy::Wdl::BlessedLoss => "blessed-loss",
            shakmaty_syzygy::Wdl::Loss => "loss",
        };
        let best = tb
            .best_move(&pos)?
            .map(|(mv, _)| UciMove::from_move(mv, CastlingMode::Standard).to_string());
        Ok(TablebaseReply {
            wdl: wdl.to_owned(),
            best_move: best,
            pieces,
            path,
        })
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))??;
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct PieceSetBody {
    pub name: String,
    pub files: std::collections::BTreeMap<String, String>,
}

pub async fn piece_sets(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::pieces::PieceSet>>, ApiError> {
    Ok(Json(crate::pieces::list(&state.data_dir)?))
}

pub async fn upload_piece_set(
    State(state): State<AppState>,
    Json(body): Json<PieceSetBody>,
) -> Result<Json<Vec<crate::pieces::PieceSet>>, ApiError> {
    crate::pieces::save(&state.data_dir, body.name.trim(), &body.files)?;
    Ok(Json(crate::pieces::list(&state.data_dir)?))
}

pub async fn delete_piece_set(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Vec<crate::pieces::PieceSet>>, ApiError> {
    crate::pieces::delete(&state.data_dir, &name)?;
    Ok(Json(crate::pieces::list(&state.data_dir)?))
}

pub async fn piece_file(
    State(state): State<AppState>,
    Path((name, code)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let bytes = crate::pieces::read(&state.data_dir, &name, &code)?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/svg+xml"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        bytes,
    )
        .into_response())
}

#[derive(Serialize)]
pub struct AccountView {
    pub provider: String,
    pub linked: bool,
    pub username: Option<String>,
    pub has_token: bool,
    pub profile: Option<crate::accounts::Profile>,
}

fn accounts_view(state: &AppState) -> Vec<AccountView> {
    let stored = state.db.settings().unwrap_or_default();
    let view = |provider: &str, user_key: &str, token_key: &str, profile_key: &str| {
        let username = stored
            .get(user_key)
            .cloned()
            .filter(|value| !value.trim().is_empty());
        let profile = stored
            .get(profile_key)
            .and_then(|raw| serde_json::from_str(&raw).ok());
        let has_token = stored
            .get(token_key)
            .map(|token| !token.trim().is_empty())
            .unwrap_or(false);
        AccountView {
            provider: provider.to_owned(),
            linked: username.is_some(),
            username,
            has_token,
            profile,
        }
    };
    vec![
        view("lichess", "lichess_user", "lichess_token", "lichess_profile"),
        view("chesscom", "chesscom_user", "", "chesscom_profile"),
    ]
}

pub async fn accounts(
    State(state): State<AppState>,
) -> Result<Json<Vec<AccountView>>, ApiError> {
    Ok(Json(accounts_view(&state)))
}

#[derive(Deserialize)]
pub struct LinkBody {
    pub provider: String,
    pub username: String,
    #[serde(default)]
    pub token: Option<String>,
}

pub async fn link_account(
    State(state): State<AppState>,
    Json(body): Json<LinkBody>,
) -> Result<Json<Vec<AccountView>>, ApiError> {
    let provider = body.provider.to_lowercase();
    let username = body.username.trim().to_owned();
    let token = body
        .token
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_owned());
    let profile = tokio::task::spawn_blocking({
        let provider = provider.clone();
        let username = username.clone();
        let token = token.clone();
        move || -> anyhow::Result<crate::accounts::Profile> {
            match provider.as_str() {
                "lichess" => crate::accounts::lichess_profile(&username, token.as_deref()),
                "chesscom" => crate::accounts::chesscom_profile(&username),
                other => anyhow::bail!("unknown provider: {other}"),
            }
        }
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))??;
    match provider.as_str() {
        "lichess" => {
            state.db.set_setting("lichess_user", &profile.username)?;
            if let Some(token) = token {
                state.db.set_setting("lichess_token", &token)?;
            }
            state
                .db
                .set_setting("lichess_profile", &serde_json::to_string(&profile)?)?;
        }
        "chesscom" => {
            state.db.set_setting("chesscom_user", &profile.username)?;
            state
                .db
                .set_setting("chesscom_profile", &serde_json::to_string(&profile)?)?;
        }
        _ => {}
    }
    Ok(Json(accounts_view(&state)))
}

#[derive(Deserialize)]
pub struct ProviderBody {
    pub provider: String,
}

pub async fn unlink_account(
    State(state): State<AppState>,
    Json(body): Json<ProviderBody>,
) -> Result<Json<Vec<AccountView>>, ApiError> {
    match body.provider.to_lowercase().as_str() {
        "lichess" => {
            state.db.set_setting("lichess_user", "")?;
            state.db.set_setting("lichess_token", "")?;
            state.db.set_setting("lichess_profile", "")?;
        }
        "chesscom" => {
            state.db.set_setting("chesscom_user", "")?;
            state.db.set_setting("chesscom_profile", "")?;
        }
        _ => {}
    }
    Ok(Json(accounts_view(&state)))
}

#[derive(Deserialize)]
pub struct AccountImportBody {
    pub provider: String,
    #[serde(default)]
    pub max: Option<u32>,
}

pub async fn import_account_games(
    State(state): State<AppState>,
    Json(body): Json<AccountImportBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let stored = state.db.settings()?;
    let provider = body.provider.to_lowercase();
    let max = body.max.unwrap_or(50).clamp(1, 500);
    let (username, token) = match provider.as_str() {
        "lichess" => (
            stored
                .get("lichess_user")
                .cloned()
                .unwrap_or_default()
                .trim()
                .to_owned(),
            stored.get("lichess_token").cloned(),
        ),
        "chesscom" => (
            stored
                .get("chesscom_user")
                .cloned()
                .unwrap_or_default()
                .trim()
                .to_owned(),
            None,
        ),
        other => return Err(ApiError::BadRequest(format!("unknown provider: {other}"))),
    };
    if username.is_empty() {
        return Err(ApiError::BadRequest("link an account first".into()));
    }
    let pgn = tokio::task::spawn_blocking({
        let provider = provider.clone();
        let username = username.clone();
        move || -> anyhow::Result<String> {
            match provider.as_str() {
                "lichess" => crate::accounts::lichess_games(&username, token.as_deref(), max),
                "chesscom" => crate::accounts::chesscom_latest_pgn(&username)
                    .map(|pgn| crate::accounts::trim_games(&pgn, max)),
                _ => anyhow::bail!("unknown provider"),
            }
        }
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))??;
    let db = state.db.clone();
    let (imported, skipped) = tokio::task::spawn_blocking(move || {
        db.import_pgn_lenient(&pgn, None)
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))??;
    Ok(Json(serde_json::json!({
        "imported": imported.len(),
        "skipped": skipped,
        "provider": provider,
    })))
}

#[derive(Deserialize)]
pub struct EngineTestBody {
    pub path: String,
}

pub async fn test_engine(Json(body): Json<EngineTestBody>) -> Json<crate::console::EngineInfo> {
    let info = tokio::task::spawn_blocking(move || crate::console::probe(&body.path))
        .await
        .unwrap_or_else(|e| crate::console::EngineInfo {
            ok: false,
            name: String::new(),
            author: String::new(),
            options: Vec::new(),
            error: Some(e.to_string()),
        });
    Json(info)
}

#[derive(Deserialize)]
pub struct ConsoleStartBody {
    #[serde(default)]
    pub path: Option<String>,
}

pub async fn console_start(
    State(state): State<AppState>,
    Json(body): Json<ConsoleStartBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let settings = effective(&state);
    let path = body
        .path
        .filter(|p| !p.trim().is_empty())
        .or_else(|| settings.engines.first().map(|e| e.path.clone()))
        .ok_or(ApiError::Unavailable("no engine is configured"))?;
    let slot = state.console.clone();
    let name = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
        let mut guard = slot.lock().expect("console lock");
        if let Some(old) = guard.take() {
            old.stop();
        }
        let console = crate::console::Console::spawn(&path)?;
        let name = console.name.clone();
        *guard = Some(console);
        Ok(name)
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))??;
    Ok(Json(serde_json::json!({
        "running": true,
        "name": if name.is_empty() { "engine".to_string() } else { name },
    })))
}

#[derive(Deserialize)]
pub struct ConsoleCommandBody {
    pub command: String,
    #[serde(default)]
    pub wait_ms: Option<u64>,
}

pub async fn console_command(
    State(state): State<AppState>,
    Json(body): Json<ConsoleCommandBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let slot = state.console.clone();
    let output = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<String>> {
        let mut guard = slot.lock().expect("console lock");
        let console = guard
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("engine console is not running"))?;
        console.command(&body.command, body.wait_ms.unwrap_or(500))
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))??;
    Ok(Json(serde_json::json!({ "output": output })))
}

pub async fn console_stop(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let slot = state.console.clone();
    tokio::task::spawn_blocking(move || {
        let mut guard = slot.lock().expect("console lock");
        if let Some(console) = guard.take() {
            console.stop();
        }
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))?;
    Ok(Json(serde_json::json!({ "running": false })))
}

pub async fn console_status(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let guard = state.console.lock().expect("console lock");
    let (running, name) = match guard.as_ref() {
        Some(console) => (true, console.name.clone()),
        None => (false, String::new()),
    };
    Ok(Json(serde_json::json!({ "running": running, "name": name })))
}

#[derive(Deserialize)]
pub struct ChatBody {
    pub message: String,
    #[serde(default)]
    pub fen: Option<String>,
    #[serde(default)]
    pub depth: Option<u32>,
}

#[derive(Serialize)]
pub struct ChatReply {
    pub reply: String,
    #[serde(default)]
    pub actions: Vec<serde_json::Value>,
    #[serde(default)]
    pub tools: Vec<String>,
}

fn chat_tools() -> serde_json::Value {
    let function = |name: &str, description: &str, parameters: serde_json::Value| {
        serde_json::json!({
            "type": "function",
            "function": { "name": name, "description": description, "parameters": parameters }
        })
    };
    serde_json::json!([
        function("get_position", "The position on the board: FEN, side to move and piece count.", serde_json::json!({"type":"object","properties":{}})),
        function("legal_moves", "Every legal move in the current position as SAN.", serde_json::json!({"type":"object","properties":{}})),
        function("analyze", "Run the engine on the current position and return the best move, score and line.", serde_json::json!({"type":"object","properties":{"depth":{"type":"integer","minimum":1,"maximum":30}}})),
        function("draw_arrow", "Draw an arrow on the board to point something out.", serde_json::json!({"type":"object","properties":{"from":{"type":"string","description":"origin square, e.g. e2"},"to":{"type":"string","description":"target square, e.g. e4"},"color":{"type":"string","enum":["green","red","blue","yellow"]}},"required":["from","to"]})),
        function("draw_circle", "Circle a square on the board.", serde_json::json!({"type":"object","properties":{"square":{"type":"string"},"color":{"type":"string","enum":["green","red","blue","yellow"]}},"required":["square"]})),
        function("clear_shapes", "Remove every arrow and circle from the board.", serde_json::json!({"type":"object","properties":{}})),
        function("play_move", "Play a legal move on the board (SAN or UCI).", serde_json::json!({"type":"object","properties":{"move":{"type":"string","description":"e.g. Nf3 or g1f3"}},"required":["move"]}))
    ])
}

fn square_ok(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() == 2
        && (b'a'..=b'h').contains(&bytes[0])
        && (b'1'..=b'8').contains(&bytes[1])
}

fn legal_moves_san(fen: &str) -> anyhow::Result<Vec<(String, String)>> {
    let (pos, mode) = knightlm_core::state::parse_chess(fen)?;
    let mut out = Vec::new();
    for mv in pos.legal_moves() {
        let san = SanPlus::from_move(pos.clone(), mv).to_string();
        let uci = UciMove::from_move(mv, mode).to_string();
        out.push((san, uci));
    }
    Ok(out)
}

fn run_chat_tool(
    name: &str,
    arguments: &serde_json::Value,
    fen: &str,
    settings: &SettingsResponse,
    depth: u32,
) -> (String, Option<serde_json::Value>) {
    let color = arguments
        .get("color")
        .and_then(|v| v.as_str())
        .unwrap_or("green");
    match name {
        "get_position" => {
            let legal = legal_moves_san(fen).map(|m| m.len()).unwrap_or(0);
            let turn = fen.split_whitespace().nth(1).unwrap_or("w");
            (
                format!(
                    "FEN {fen} · {} to move · {legal} legal moves",
                    if turn == "b" { "Black" } else { "White" }
                ),
                None,
            )
        }
        "legal_moves" => match legal_moves_san(fen) {
            Ok(moves) => {
                let list: Vec<String> = moves.iter().take(60).map(|(san, _)| san.clone()).collect();
                (list.join(" "), None)
            }
            Err(error) => (format!("could not read the position: {error}"), None),
        },
        "analyze" => {
            let depth = arguments
                .get("depth")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32)
                .unwrap_or(depth)
                .clamp(1, 30);
            let analysed: Result<Vec<knightlm_engine::Analysis>, String> =
                match engine_for(settings, fen, "standard") {
                    Ok((path, options)) => knightlm_engine::Engine::spawn_with(&path, 30, options)
                        .and_then(|mut engine| engine.analyse_lines(fen, depth, 1))
                        .map_err(|e| e.to_string()),
                    Err(error) => Err(format!("{error:?}")),
                };
            match analysed {
                Ok(lines) => match lines.first() {
                    Some(line) => {
                        let score = line
                            .mate
                            .map(|m| format!("mate {m}"))
                            .or_else(|| line.score_cp.map(|c| format!("{:+.2}", c as f64 / 100.0)))
                            .unwrap_or_else(|| "?".into());
                        (
                            format!(
                                "best {} ({score}) · pv {}",
                                line.best_move,
                                line.pv.join(" ")
                            ),
                            None,
                        )
                    }
                    None => ("no analysis returned".into(), None),
                },
                Err(error) => (format!("engine unavailable: {error:?}"), None),
            }
        }
        "draw_arrow" => {
            let from = arguments.get("from").and_then(|v| v.as_str()).unwrap_or("");
            let to = arguments.get("to").and_then(|v| v.as_str()).unwrap_or("");
            if !square_ok(from) || !square_ok(to) {
                return ("invalid squares".into(), None);
            }
            (
                "arrow drawn".into(),
                Some(serde_json::json!({"kind":"arrow","from":from,"to":to,"color":color})),
            )
        }
        "draw_circle" => {
            let square = arguments
                .get("square")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !square_ok(square) {
                return ("invalid square".into(), None);
            }
            (
                "circle drawn".into(),
                Some(serde_json::json!({"kind":"circle","square":square,"color":color})),
            )
        }
        "clear_shapes" => (
            "shapes cleared".into(),
            Some(serde_json::json!({"kind":"clear"})),
        ),
        "play_move" => {
            let raw = arguments
                .get("move")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if raw.is_empty() {
                return ("no move given".into(), None);
            }
            let Ok((pos, mode)) = knightlm_core::state::parse_chess(fen) else {
                return ("the board position is invalid".into(), None);
            };
            let mv = if let Ok(uci) = UciMove::from_ascii(raw.as_bytes()) {
                uci.to_move(&pos).ok()
            } else {
                San::from_ascii(raw.as_bytes())
                    .ok()
                    .and_then(|san| san.to_move(&pos).ok())
            };
            match mv {
                Some(mv) if pos.is_legal(mv) => {
                    let san = SanPlus::from_move(pos.clone(), mv).to_string();
                    let uci = UciMove::from_move(mv, mode).to_string();
                    (
                        format!("played {san}"),
                        Some(serde_json::json!({"kind":"move","uci":uci,"san":san})),
                    )
                }
                _ => (format!("{raw} is not legal here"), None),
            }
        }
        other => (format!("unknown tool {other}"), None),
    }
}

fn chat_with_tools(
    url: &str,
    message: &str,
    fen: &str,
    depth: u32,
    settings: &SettingsResponse,
) -> anyhow::Result<ChatReply> {
    let agent = ureq::Agent::new_with_defaults();
    let tools = chat_tools();
    let mut messages = vec![
        serde_json::json!({
            "role": "system",
            "content": "You are the KnightLM chess assistant. The board position is given as a FEN. Use the tools to read the position, run the engine, draw arrows/circles and play legal moves. Keep replies short; the board shows your actions automatically."
        }),
        serde_json::json!({ "role": "user", "content": message }),
    ];
    let mut actions = Vec::new();
    let mut used = Vec::new();
    let mut reply = String::new();
    for _ in 0..3 {
        let payload = serde_json::json!({
            "model": "knightlm",
            "messages": messages,
            "tools": tools,
            "tool_choice": "auto",
            "temperature": 0.2,
            "fen": fen,
        });
        let mut response = agent.post(url).send_json(payload)?;
        let value: serde_json::Value = response.body_mut().read_json()?;
        // Plain-text servers that do not implement the OpenAI chat shape.
        if let Some(plain) = value.get("reply").and_then(|v| v.as_str()) {
            return Ok(ChatReply {
                reply: plain.to_owned(),
                actions,
                tools: used,
            });
        }
        let Some(choice) = value
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|c| c.first())
        else {
            anyhow::bail!("model response has neither choices nor reply");
        };
        let msg = &choice["message"];
        if let Some(content) = msg.get("content").and_then(|v| v.as_str()) {
            if !content.trim().is_empty() {
                reply = content.trim().to_owned();
            }
        }
        let calls = msg
            .get("tool_calls")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if calls.is_empty() {
            return Ok(ChatReply {
                reply,
                actions,
                tools: used,
            });
        }
        messages.push(serde_json::json!({
            "role": "assistant",
            "content": msg.get("content").cloned().unwrap_or(serde_json::Value::Null),
            "tool_calls": calls,
        }));
        for call in calls {
            let name = call["function"]["name"].as_str().unwrap_or_default();
            let raw_args = call["function"]["arguments"]
                .as_str()
                .unwrap_or("{}");
            let arguments: serde_json::Value =
                serde_json::from_str(raw_args).unwrap_or(serde_json::json!({}));
            let (result, action) = run_chat_tool(name, &arguments, fen, settings, depth);
            used.push(name.to_owned());
            if let Some(action) = action {
                actions.push(action);
            }
            messages.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": call["id"].as_str().unwrap_or_default(),
                "content": result,
            }));
        }
    }
    Ok(ChatReply {
        reply,
        actions,
        tools: used,
    })
}

pub async fn chat(
    State(state): State<AppState>,
    Json(body): Json<ChatBody>,
) -> Result<Json<ChatReply>, ApiError> {
    let settings = effective(&state);
    let url = settings
        .model_url
        .clone()
        .ok_or(ApiError::Unavailable("model URL is not set"))?;
    let fen = body
        .fen
        .clone()
        .filter(|f| !f.trim().is_empty())
        .unwrap_or_else(|| {
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".to_owned()
        });
    let depth = body.depth.unwrap_or(settings.depth).clamp(1, 30);
    let message = body.message;
    let reply = tokio::task::spawn_blocking(move || {
        chat_with_tools(&url, &message, &fen, depth, &settings)
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::anyhow!(e)))??;
    Ok(Json(reply))
}
