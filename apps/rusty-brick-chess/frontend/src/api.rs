//! Frontend API client (gloo-net).

use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct GameSummary {
    pub id: i64,
    pub event: Option<String>,
    pub site: Option<String>,
    pub date: Option<String>,
    pub white: String,
    pub black: String,
    pub result: String,
    pub moves: String,
    pub fen: String,
    pub ply_count: i64,
    #[serde(default)]
    pub variant: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GameDetail {
    pub id: i64,
    pub event: Option<String>,
    pub site: Option<String>,
    pub date: Option<String>,
    pub round: Option<String>,
    pub white: String,
    pub black: String,
    pub white_elo: Option<i32>,
    pub black_elo: Option<i32>,
    pub result: String,
    pub fen: String,
    pub moves: String,
    pub movetext: Option<String>,
    pub ply_count: i64,
    pub time_control: Option<String>,
    pub other: Option<serde_json::Value>,
    #[serde(default)]
    pub variant: Option<String>,
}

pub async fn get_game(id: i64) -> Result<GameDetail, String> {
    let resp = Request::get(&format!("/api/games/{id}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PgnNode {
    pub san: String,
    #[serde(default)]
    pub nags: Vec<u32>,
    #[serde(default)]
    pub pre_comment: Option<String>,
    #[serde(default)]
    pub post_comment: Option<String>,
    #[serde(default)]
    pub variations: Vec<Vec<PgnNode>>,
}

pub fn parse_movetext(json: &str) -> Vec<PgnNode> {
    serde_json::from_str(json).unwrap_or_default()
}

pub fn game_pgn_url(id: i64) -> String {
    format!("/api/games/{id}/pgn")
}

pub async fn delete_game(id: i64) -> Result<(), String> {
    let resp = Request::delete(&format!("/api/games/{id}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.ok() {
        Ok(())
    } else {
        Err(resp.text().await.unwrap_or_default())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct EvalRow {
    pub ply: i64,
    pub depth: u32,
    pub cp: Option<i32>,
    pub mate: Option<i32>,
}

pub async fn evals(id: i64, depth: Option<u32>) -> Result<Vec<EvalRow>, String> {
    let url = match depth {
        Some(d) => format!("/api/games/{id}/evals?depth={d}"),
        None => format!("/api/games/{id}/evals"),
    };
    let resp = Request::get(&url).send().await.map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

#[derive(Deserialize)]
struct QueryResponse {
    games: Vec<GameSummary>,
    total: i64,
}

#[derive(Debug, Clone, Default)]
pub struct Filters {
    pub player: String,
    pub event: Option<String>,
    pub collection: Option<i64>,
    pub date_from: String,
    pub date_to: String,
    pub result: String,
    pub elo_min: String,
    pub elo_max: String,
    pub variant: String,
    pub sort: String,
}

pub async fn list_games(filters: &Filters) -> Result<(Vec<GameSummary>, i64), String> {
    let mut url = String::from("/api/games?limit=100");
    if !filters.player.trim().is_empty() {
        url.push_str(&format!("&player={}", encode(filters.player.trim())));
    }
    if let Some(event) = filters.event.as_deref().filter(|e| !e.is_empty()) {
        url.push_str(&format!("&event={}", encode(event)));
    }
    if let Some(id) = filters.collection {
        url.push_str(&format!("&collection_id={id}"));
    }
    if !filters.date_from.trim().is_empty() {
        url.push_str(&format!("&date_from={}", encode(filters.date_from.trim())));
    }
    if !filters.date_to.trim().is_empty() {
        url.push_str(&format!("&date_to={}", encode(filters.date_to.trim())));
    }
    if !filters.result.trim().is_empty() {
        url.push_str(&format!("&result={}", encode(filters.result.trim())));
    }
    if let Ok(min) = filters.elo_min.trim().parse::<i32>() {
        url.push_str(&format!("&elo_min={min}"));
    }
    if let Ok(max) = filters.elo_max.trim().parse::<i32>() {
        url.push_str(&format!("&elo_max={max}"));
    }
    if !filters.variant.trim().is_empty() && filters.variant != "any" {
        url.push_str(&format!("&variant={}", encode(filters.variant.trim())));
    }
    if !filters.sort.trim().is_empty() && filters.sort != "newest" {
        url.push_str(&format!("&sort={}", encode(filters.sort.trim())));
    }
    let resp = Request::get(&url).send().await.map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let r: QueryResponse = resp.json().await.map_err(|e| e.to_string())?;
    Ok((r.games, r.total))
}

pub async fn import_pgn(pgn: &str) -> Result<usize, String> {
    let resp = Request::post("/api/games")
        .json(&serde_json::json!({ "pgn": pgn }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(v["imported"].as_array().map(|a| a.len()).unwrap_or(0))
}

#[derive(Debug, Clone, Deserialize)]
pub struct Analysis {
    pub best_move: String,
    pub score_cp: Option<i32>,
    pub mate: Option<i32>,
    pub depth: u32,
    pub pv: Vec<String>,
    pub nodes: Option<u64>,
}

pub async fn analyze(fen: &str, depth: u32, variant: &str) -> Result<Analysis, String> {
    let resp = Request::post("/api/analyze")
        .json(&serde_json::json!({ "fen": fen, "depth": depth, "variant": variant }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[derive(Debug, Clone, Deserialize)]
pub struct Continuation {
    pub uci: String,
    pub san: String,
    pub games: i64,
    pub white: i64,
    pub draw: i64,
    pub black: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Explorer {
    pub eco: Option<String>,
    pub name: Option<String>,
    pub continuations: Vec<Continuation>,
}

pub async fn explore(fen: &str, source: Option<&str>) -> Result<Explorer, String> {
    let mut url = format!("/api/openings?fen={}", encode(fen));
    if let Some(source) = source {
        url.push_str(&format!("&source={}", encode(source)));
    }
    let resp = Request::get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct Puzzle {
    pub id: String,
    pub fen: String,
    pub moves: String,
    pub moves_san: String,
    pub rating: i32,
    pub themes: Option<String>,
    pub game_url: Option<String>,
}

pub async fn next_puzzle(rating: Option<i32>, theme: Option<&str>) -> Result<Option<Puzzle>, String> {
    let mut url = "/api/puzzles/next".to_owned();
    let mut sep = '?';
    if let Some(r) = rating {
        url.push_str(&format!("{sep}rating={r}"));
        sep = '&';
    }
    if let Some(theme) = theme.filter(|t| !t.is_empty()) {
        url.push_str(&format!("{sep}theme={}", encode(theme)));
    }
    let resp = Request::get(&url).send().await.map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThemeCount {
    pub theme: String,
    pub count: i64,
}

pub async fn puzzle_themes() -> Result<Vec<ThemeCount>, String> {
    let resp = Request::get("/api/puzzles/themes")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn puzzle_attempt(
    puzzle_id: &str,
    puzzle_rating: i32,
    solved: bool,
) -> Result<(i32, i32), String> {
    let resp = Request::post("/api/puzzles/attempt")
        .json(&serde_json::json!({
            "puzzle_id": puzzle_id,
            "puzzle_rating": puzzle_rating,
            "solved": solved,
        }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok((
        v["rating_before"].as_i64().unwrap_or(0) as i32,
        v["rating_after"].as_i64().unwrap_or(0) as i32,
    ))
}


#[derive(Debug, Clone, Default, Deserialize)]
pub struct LabelCounts {
    pub best: usize,
    pub excellent: usize,
    pub good: usize,
    pub inaccuracy: usize,
    pub mistake: usize,
    pub blunder: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnalyzedMove {
    pub ply: usize,
    pub san: String,
    pub uci: String,
    pub lichess: String,
    pub expected_points: String,
    pub win_before: f64,
    pub win_after: f64,
    pub accuracy: f64,
    pub best_move: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GameReport {
    pub moves: Vec<AnalyzedMove>,
    pub white_accuracy: Option<f64>,
    pub black_accuracy: Option<f64>,
    pub lichess_counts: LabelCounts,
    pub expected_points_counts: LabelCounts,
}

#[derive(Deserialize)]
struct ReportResponse {
    report: GameReport,
}

pub async fn game_report(id: i64, depth: u32) -> Result<GameReport, String> {
    let resp = Request::get(&format!("/api/games/{id}/report?depth={depth}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    let r: ReportResponse = resp.json().await.map_err(|e| e.to_string())?;
    Ok(r.report)
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct EngineEntry {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub options: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Settings {
    pub engine_path: Option<String>,
    pub model_url: Option<String>,
    pub depth: u32,
    pub theme: String,
    pub pieces: String,
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

impl Default for Settings {
    fn default() -> Self {
        Self {
            engine_path: None,
            model_url: None,
            depth: 15,
            theme: "green".into(),
            pieces: "cburnett".into(),
            threads: 0,
            hash_mb: 0,
            syzygy_path: None,
            variant_engine_path: None,
            online_explorer: false,
            animate_last_move: false,
            sound_enabled: true,
            sound_theme: "sfx".into(),
            sound_volume: 70,
            piece_animation_ms: 0,
            puzzle_rating: 1500,
            profile_name: "Player".into(),
            profile_player: None,
            keybinds: "{\"previous\":\"ArrowLeft\",\"next\":\"ArrowRight\",\"start\":\"Home\",\"end\":\"End\",\"flip\":\"f\"}".into(),
            engines: Vec::new(),
        }
    }
}

pub async fn get_settings() -> Result<Settings, String> {
    let resp = Request::get("/api/settings").send().await.map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn save_settings(settings: &Settings) -> Result<Settings, String> {
    let resp = Request::put("/api/settings")
        .json(&serde_json::json!({
            "engine_path": settings.engine_path,
            "model_url": settings.model_url,
            "depth": settings.depth,
            "theme": settings.theme,
            "pieces": settings.pieces,
            "threads": settings.threads,
            "hash_mb": settings.hash_mb,
            "syzygy_path": settings.syzygy_path,
            "variant_engine_path": settings.variant_engine_path,
            "online_explorer": settings.online_explorer,
            "animate_last_move": settings.animate_last_move,
            "sound_enabled": settings.sound_enabled,
            "sound_theme": settings.sound_theme,
            "sound_volume": settings.sound_volume,
            "piece_animation_ms": settings.piece_animation_ms,
            "profile_name": settings.profile_name,
            "profile_player": settings.profile_player,
            "keybinds": settings.keybinds,
            "engines": settings.engines,
        }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct Annotation {
    pub id: i64,
    pub game_id: i64,
    pub ply: i64,
    pub path: Option<String>,
    pub comment: Option<String>,
    pub nag: Option<i64>,
    pub shapes: Option<String>,
}

pub async fn annotations(game_id: i64) -> Result<Vec<Annotation>, String> {
    let resp = Request::get(&format!("/api/games/{game_id}/annotations"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn add_annotation(
    game_id: i64,
    ply: i64,
    path: Option<String>,
    comment: Option<String>,
    nag: Option<i64>,
    shapes: Option<String>,
) -> Result<(), String> {
    let resp = Request::post(&format!("/api/games/{game_id}/annotations"))
        .json(&serde_json::json!({
            "game_id": game_id,
            "ply": ply,
            "path": path,
            "comment": comment,
            "nag": nag,
            "shapes": shapes,
        }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct EngineLine {
    pub best_move: String,
    pub score_cp: Option<i32>,
    pub mate: Option<i32>,
    pub depth: u32,
    pub pv: Vec<String>,
    pub multipv: Option<u16>,
}

pub async fn engine_lines(
    fen: &str,
    depth: u32,
    lines: u32,
    variant: &str,
) -> Result<Vec<EngineLine>, String> {
    let resp = Request::get(&format!(
        "/api/analyze/lines?fen={}&depth={depth}&lines={lines}&variant={}",
        encode(fen),
        encode(variant)
    ))
    .send()
    .await
    .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn import_puzzles(csv: &str) -> Result<usize, String> {
    let resp = Request::post("/api/puzzles/import")
        .json(&serde_json::json!({ "csv": csv }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    #[derive(Deserialize)]
    struct Imported {
        imported: usize,
    }
    let r: Imported = resp.json().await.map_err(|e| e.to_string())?;
    Ok(r.imported)
}

pub async fn chat(fen: &str, message: &str) -> Result<ChatReply, String> {
    let resp = Request::post("/api/chat")
        .json(&serde_json::json!({ "message": message, "fen": fen }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    let r: ChatReply = resp.json().await.map_err(|e| e.to_string())?;
    Ok(r)
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatReply {
    pub reply: String,
    #[serde(default)]
    pub actions: Vec<serde_json::Value>,
    #[serde(default)]
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EngineOptionInfo {
    pub name: String,
    pub kind: String,
    pub default: Option<String>,
    pub min: Option<String>,
    pub max: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EngineInfo {
    pub ok: bool,
    pub name: String,
    pub author: String,
    pub options: Vec<EngineOptionInfo>,
    pub error: Option<String>,
}

pub async fn test_engine(path: &str) -> Result<EngineInfo, String> {
    let resp = Request::post("/api/engines/test")
        .json(&serde_json::json!({ "path": path }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    resp.json().await.map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConsoleStatus {
    pub running: bool,
    pub name: String,
}

pub async fn console_status() -> Result<ConsoleStatus, String> {
    let resp = Request::get("/api/engine/console/status")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn console_start(path: Option<&str>) -> Result<ConsoleStatus, String> {
    let resp = Request::post("/api/engine/console/start")
        .json(&serde_json::json!({ "path": path }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn console_command(command: &str, wait_ms: u64) -> Result<Vec<String>, String> {
    let resp = Request::post("/api/engine/console/command")
        .json(&serde_json::json!({ "command": command, "wait_ms": wait_ms }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    #[derive(Deserialize)]
    struct Output {
        output: Vec<String>,
    }
    let out: Output = resp.json().await.map_err(|e| e.to_string())?;
    Ok(out.output)
}

pub async fn console_stop() -> Result<(), String> {
    Request::post("/api/engine/console/stop")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub games: i64,
}

pub async fn collections() -> Result<Vec<Collection>, String> {
    let resp = Request::get("/api/collections").send().await.map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn create_collection(name: &str) -> Result<Collection, String> {
    let resp = Request::post("/api/collections")
        .json(&serde_json::json!({ "name": name }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn add_to_collection(collection_id: i64, game_id: i64) -> Result<(), String> {
    let resp = Request::post(&format!("/api/collections/{collection_id}/games/{game_id}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.ok() {
        Ok(())
    } else {
        Err(resp.text().await.unwrap_or_default())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerRow {
    pub name: String,
    pub elo: Option<i32>,
    pub games: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EventRow {
    pub name: String,
    pub games: i64,
}

pub async fn players() -> Result<Vec<PlayerRow>, String> {
    let resp = Request::get("/api/players").send().await.map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn events() -> Result<Vec<EventRow>, String> {
    let resp = Request::get("/api/events").send().await.map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn search_position(
    fen: &str,
    partial: bool,
    variant: &str,
) -> Result<Vec<GameSummary>, String> {
    let resp = Request::get(&format!(
        "/api/positions?fen={}&partial={partial}&variant={}",
        encode(fen),
        encode(variant)
    ))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn save_tags(id: i64, tags: serde_json::Value) -> Result<GameDetail, String> {
    let resp = Request::patch(&format!("/api/games/{id}"))
        .json(&tags)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct Tablebase {
    pub wdl: String,
    pub best_move: Option<String>,
    pub pieces: usize,
    pub path: String,
}

pub async fn tablebase(fen: &str, variant: &str) -> Result<Tablebase, String> {
    let resp = Request::get(&format!(
        "/api/tablebase?fen={}&variant={}",
        encode(fen),
        encode(variant)
    ))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseInfo {
    pub name: String,
    pub path: String,
    pub current: bool,
}

pub async fn databases() -> Result<Vec<DatabaseInfo>, String> {
    let resp = Request::get("/api/databases").send().await.map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn create_database(name: &str) -> Result<(), String> {
    let resp = Request::post("/api/databases")
        .json(&serde_json::json!({ "name": name }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.ok() {
        Ok(())
    } else {
        Err(resp.text().await.unwrap_or_default())
    }
}

pub async fn switch_database(name: &str) -> Result<(), String> {
    let resp = Request::post("/api/databases/switch")
        .json(&serde_json::json!({ "name": name }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.ok() {
        Ok(())
    } else {
        Err(resp.text().await.unwrap_or_default())
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Stats {
    pub games: i64,
    pub white_wins: i64,
    pub draws: i64,
    pub black_wins: i64,
    pub unfinished: i64,
    pub players: i64,
    pub events: i64,
    pub collections: i64,
    pub puzzles: i64,
    pub top_players: Vec<PlayerRow>,
    pub top_events: Vec<EventRow>,
}

pub async fn stats() -> Result<Stats, String> {
    let resp = Request::get("/api/stats").send().await.map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct PuzzleAttempt {
    pub id: i64,
    pub puzzle_id: String,
    pub solved: bool,
    pub rating_before: i32,
    pub rating_after: i32,
    pub at: String,
}

pub async fn puzzle_attempts(limit: u32) -> Result<Vec<PuzzleAttempt>, String> {
    let resp = Request::get(&format!("/api/puzzles/attempts?limit={limit}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct EngineAnalysis {
    pub name: String,
    pub path: String,
    pub lines: Vec<EngineLine>,
    pub error: Option<String>,
}

pub async fn analyze_engines(
    fen: &str,
    depth: u32,
    lines: u32,
    variant: &str,
) -> Result<Vec<EngineAnalysis>, String> {
    let resp = Request::get(&format!(
        "/api/analyze/engines?fen={}&depth={depth}&lines={lines}&variant={}",
        encode(fen),
        encode(variant)
    ))
    .send()
    .await
    .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn import_lichess(username: &str, max: u32) -> Result<(usize, usize), String> {
    let resp = Request::post("/api/import/lichess")
        .json(&serde_json::json!({ "username": username, "max": max }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok((
        v["imported"].as_u64().unwrap_or(0) as usize,
        v["skipped"].as_u64().unwrap_or(0) as usize,
    ))
}

pub async fn import_chesscom(username: &str, months: u32) -> Result<(usize, usize), String> {
    let resp = Request::post("/api/import/chesscom")
        .json(&serde_json::json!({ "username": username, "months": months }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok((
        v["imported"].as_u64().unwrap_or(0) as usize,
        v["skipped"].as_u64().unwrap_or(0) as usize,
    ))
}

#[derive(Debug, Clone, Deserialize)]
pub struct Repertoire {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub cards: i64,
    pub due: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RepertoireCard {
    pub id: i64,
    pub repertoire_id: i64,
    pub fen: String,
    pub uci: String,
    pub san: String,
    pub due_at: String,
    pub interval_days: f64,
    pub ease: f64,
    pub reviews: i64,
    pub lapses: i64,
}

pub async fn repertoires() -> Result<Vec<Repertoire>, String> {
    let resp = Request::get("/api/repertoires").send().await.map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn create_repertoire(name: &str, color: &str) -> Result<Repertoire, String> {
    let resp = Request::post("/api/repertoires")
        .json(&serde_json::json!({ "name": name, "color": color }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn delete_repertoire(id: i64) -> Result<(), String> {
    let resp = Request::delete(&format!("/api/repertoires/{id}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.ok() {
        Ok(())
    } else {
        Err(resp.text().await.unwrap_or_default())
    }
}

pub async fn import_repertoire_pgn(id: i64, pgn: &str) -> Result<usize, String> {
    let resp = Request::post(&format!("/api/repertoires/{id}/import"))
        .json(&serde_json::json!({ "pgn": pgn }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(v["imported"].as_u64().unwrap_or(0) as usize)
}

pub async fn next_repertoire_card(id: i64) -> Result<Option<RepertoireCard>, String> {
    let resp = Request::get(&format!("/api/repertoires/{id}/next"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn review_repertoire_card(card_id: i64, correct: bool) -> Result<RepertoireCard, String> {
    let resp = Request::post("/api/repertoires/review")
        .json(&serde_json::json!({ "card_id": card_id, "correct": correct }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn rename_collection(id: i64, name: &str) -> Result<(), String> {
    let resp = Request::patch(&format!("/api/collections/{id}"))
        .json(&serde_json::json!({ "name": name }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.ok() {
        Ok(())
    } else {
        Err(resp.text().await.unwrap_or_default())
    }
}

pub async fn delete_collection(id: i64) -> Result<(), String> {
    let resp = Request::delete(&format!("/api/collections/{id}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.ok() {
        Ok(())
    } else {
        Err(resp.text().await.unwrap_or_default())
    }
}

pub async fn update_moves(id: i64, moves: &str) -> Result<GameDetail, String> {
    let resp = Request::post(&format!("/api/games/{id}/moves"))
        .json(&serde_json::json!({ "moves": moves }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn truncate_moves(id: i64, ply: usize) -> Result<GameDetail, String> {
    let resp = Request::post(&format!("/api/games/{id}/moves/truncate"))
        .json(&serde_json::json!({ "ply": ply }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn game_pgn_text(id: i64) -> Result<String, String> {
    let resp = Request::get(&format!("/api/games/{id}/pgn"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.text().await.map_err(|e| e.to_string())
}

pub async fn update_tree(id: i64, pgn: &str) -> Result<GameDetail, String> {
    let resp = Request::put(&format!("/api/games/{id}/tree"))
        .json(&serde_json::json!({ "pgn": pgn }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn delete_tree_node(id: i64, path: &str) -> Result<GameDetail, String> {
    let resp = Request::post(&format!("/api/games/{id}/tree/delete"))
        .json(&serde_json::json!({ "path": path }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn promote_variation(id: i64, path: &str) -> Result<GameDetail, String> {
    let resp = Request::post(&format!("/api/games/{id}/tree/promote"))
        .json(&serde_json::json!({ "path": path }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn import_repertoire_game(repertoire_id: i64, game_id: i64) -> Result<usize, String> {
    let resp = Request::post(&format!(
        "/api/repertoires/{repertoire_id}/import-game/{game_id}"
    ))
    .send()
    .await
    .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(v["imported"].as_u64().unwrap_or(0) as usize)
}

pub async fn add_tree_move(
    id: i64,
    parent: &str,
    san: &str,
    clock: Option<&str>,
) -> Result<(GameDetail, String), String> {
    let resp = Request::post(&format!("/api/games/{id}/tree/add"))
        .json(&serde_json::json!({ "parent": parent, "san": san, "clock": clock }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let game: GameDetail =
        serde_json::from_value(v["game"].clone()).map_err(|e| e.to_string())?;
    Ok((game, v["path"].as_str().unwrap_or_default().to_owned()))
}

#[derive(Debug, Clone, Deserialize)]
pub struct VisionStatus {
    pub installed: bool,
    pub path: String,
    pub bytes: u64,
    pub name: String,
    pub license: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

pub async fn vision_status() -> Result<VisionStatus, String> {
    let resp = Request::get("/api/vision").send().await.map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn download_vision() -> Result<VisionStatus, String> {
    let resp = Request::post("/api/vision/download")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct VisionResult {
    pub placement: String,
    pub confidence: f32,
    pub board: [f32; 4],
    pub detections: usize,
    pub turn: String,
}

pub async fn recognize_image(image: &str) -> Result<VisionResult, String> {
    let resp = Request::post("/api/vision/recognize")
        .json(&serde_json::json!({ "image": image }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn upload_sound(name: &str, data_url: &str) -> Result<(), String> {
    let resp = Request::post(&format!("/api/sounds/custom/{name}"))
        .json(&serde_json::json!({ "data": data_url }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.ok() {
        Ok(())
    } else {
        Err(resp.text().await.unwrap_or_default())
    }
}

#[derive(Debug, Clone, Deserialize)]
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
    pub recent: Vec<GameSummary>,
}

pub async fn profile(player: &str) -> Result<Profile, String> {
    let resp = Request::get(&format!("/api/profile?player={}", url_component(player)))
    .send()
    .await
    .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

/// Minimal percent-encoding for query values (no extra dependency).
fn url_component(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
