//! Pure data types and helpers shared by the app (no reactive state).

use crate::api;
use wasm_bindgen::JsCast;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Home,
    Database,
    Analysis,
    Practice,
    Openings,
    Puzzles,
    Stats,
    Settings,
}

pub fn default_workspace_kind() -> String {
    "analysis".into()
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Workspace {
    pub key: u32,
    #[serde(default = "default_workspace_kind")]
    pub kind: String,
    pub game_id: Option<i64>,
    pub title: String,
    pub pgn: String,
    pub ply: usize,
    pub variant: String,
    pub preview_only: bool,
    pub moves: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Session {
    pub workspaces: Vec<Workspace>,
    pub active: u32,
    pub seq: u32,
    pub tab: String,
}

const SESSION_KEY: &str = "rbc.session.v1";
const RECENT_FILES_KEY: &str = "rbc.recentFiles";
const CLIENT_KEY: &str = "rbc.client";

/// Stable per-browser id so server-side sessions never leak between clients.
pub fn client_id() -> String {
    let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) else {
        return format!("anon-{}", js_sys::Date::now() as u64);
    };
    if let Ok(Some(id)) = storage.get_item(CLIENT_KEY) {
        if id.len() >= 8 {
            return id;
        }
    }
    let id = format!(
        "c{}-{}",
        js_sys::Date::now() as u64,
        (js_sys::Math::random() * 1_000_000_000.0) as u64
    );
    let _ = storage.set_item(CLIENT_KEY, &id);
    id
}

pub fn tab_name(tab: Tab) -> &'static str {
    match tab {
        Tab::Home => "home",
        Tab::Database => "database",
        Tab::Analysis => "analysis",
        Tab::Practice => "practice",
        Tab::Openings => "openings",
        Tab::Puzzles => "puzzles",
        Tab::Stats => "stats",
        Tab::Settings => "settings",
    }
}

pub fn tab_from_name(name: &str) -> Option<Tab> {
    Some(match name {
        "home" => Tab::Home,
        "database" => Tab::Database,
        "analysis" => Tab::Analysis,
        "practice" => Tab::Practice,
        "openings" => Tab::Openings,
        "puzzles" => Tab::Puzzles,
        "stats" => Tab::Stats,
        "settings" => Tab::Settings,
        _ => return None,
    })
}

pub fn read_session() -> Option<Session> {
    let storage = web_sys::window()?.local_storage().ok().flatten()?;
    let raw = storage.get_item(SESSION_KEY).ok().flatten()?;
    serde_json::from_str(&raw).ok()
}

pub fn read_recent_files() -> Vec<(String, usize)> {
    let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) else {
        return Vec::new();
    };
    storage
        .get_item(RECENT_FILES_KEY)
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn write_recent_files(files: &[(String, usize)]) {
    let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) else {
        return;
    };
    if let Ok(raw) = serde_json::to_string(files) {
        let _ = storage.set_item(RECENT_FILES_KEY, &raw);
    }
}

pub fn write_session(session: &Session) {
    let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
    else {
        return;
    };
    if let Ok(raw) = serde_json::to_string(session) {
        let _ = storage.set_item(SESSION_KEY, &raw);
    }
}

#[derive(Clone, Debug)]
pub struct TreeItem {
    pub path: String,
    pub ply: usize,
    pub san: String,
    pub depth: usize,
    pub line: Vec<String>,
    pub clock: Option<String>,
}

pub fn download_text(name: &str, text: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let encoded = js_sys::encode_uri_component(text);
    let href = format!("data:text/markdown;charset=utf-8,{encoded}");
    if let Ok(anchor) = document.create_element("a") {
        let _ = anchor.set_attribute("href", &href);
        let _ = anchor.set_attribute("download", name);
        if let Some(element) = anchor.dyn_into::<web_sys::HtmlElement>().ok() {
            element.click();
        }
    }
}

pub fn clock_of(node: &api::PgnNode) -> Option<String> {
    for comment in [node.pre_comment.as_ref(), node.post_comment.as_ref()]
        .into_iter()
        .flatten()
    {
        if let Some(start) = comment.find("[%clk ") {
            let rest = &comment[start + 6..];
            if let Some(end) = rest.find(']') {
                return Some(rest[..end].to_owned());
            }
        }
    }
    None
}

pub fn flatten_nodes(
    nodes: &[api::PgnNode],
    prefix: &str,
    ply0: usize,
    depth: usize,
    line: &mut Vec<String>,
    out: &mut Vec<TreeItem>,
) {
    let mut ply = ply0;
    for (i, node) in nodes.iter().enumerate() {
        let path = format!("{prefix}{i}");
        // A variation replaces this move, so it branches from the position
        // *before* it, not from the line that already contains it.
        let prior = line.clone();
        line.push(node.san.clone());
        out.push(TreeItem {
            path: path.clone(),
            ply,
            san: node.san.clone(),
            depth,
            line: line.clone(),
            clock: clock_of(node),
        });
        for (j, variation) in node.variations.iter().enumerate() {
            let mut branch = prior.clone();
            flatten_nodes(
                variation,
                &format!("{path}~{j}."),
                ply,
                depth + 1,
                &mut branch,
                out,
            );
        }
        ply += 1;
    }
}

pub fn tree_items(nodes: &[api::PgnNode]) -> Vec<TreeItem> {
    let mut out = Vec::new();
    flatten_nodes(nodes, "", 0, 0, &mut Vec::new(), &mut out);
    out
}

pub fn plain_items(sans: &[String]) -> Vec<TreeItem> {
    let mut out = Vec::new();
    for (i, san) in sans.iter().enumerate() {
        out.push(TreeItem {
            path: i.to_string(),
            ply: i,
            san: san.clone(),
            depth: 0,
            line: sans[..=i].to_vec(),
            clock: None,
        });
    }
    out
}

pub fn line_pgn(fen: &str, sans: &[String], variant: &str) -> String {
    let standard = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    let black_start = fen.split_whitespace().nth(1) == Some("b");
    let mut out = String::new();
    let mut has_tags = false;
    if !variant.is_empty() && variant != "standard" {
        out.push_str(&format!("[Variant \"{variant}\"]\n"));
        has_tags = true;
    }
    if fen != standard && !fen.is_empty() {
        out.push_str(&format!("[SetUp \"1\"]\n[FEN \"{fen}\"]\n"));
        has_tags = true;
    }
    if has_tags {
        out.push('\n');
    }
    for (i, san) in sans.iter().enumerate() {
        if i == 0 && black_start {
            out.push_str("1... ");
        } else if i % 2 == 0 {
            out.push_str(&format!("{}. ", i / 2 + 1));
        }
        out.push_str(san);
        out.push(' ');
    }
    out.push('*');
    out
}

pub fn find_annotation<'a>(
    anns: &'a [api::Annotation],
    path: &str,
    ply: i64,
) -> Option<&'a api::Annotation> {
    anns.iter()
        .find(|a| a.path.as_deref() == Some(path))
        .or_else(|| {
            anns.iter()
                .find(|a| a.path.as_deref().map(str::is_empty).unwrap_or(true) && a.ply == ply)
        })
}

pub fn annotation_shapes(ann: Option<&api::Annotation>) -> Vec<brick_chess::Shape> {
    ann.and_then(|a| a.shapes.as_deref())
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default()
}

pub fn eval_value(row: &api::EvalRow) -> f64 {
    if let Some(mate) = row.mate {
        if mate > 0 { 1200.0 } else { -1200.0 }
    } else {
        row.cp.unwrap_or(0) as f64
    }
}

pub fn pieces_from_fen(fen: &str) -> [Option<char>; 64] {
    let mut out = [None; 64];
    let board = fen.split_whitespace().next().unwrap_or_default();
    let mut rank: i32 = 7;
    let mut file = 0usize;
    for ch in board.chars() {
        match ch {
            '/' => {
                rank -= 1;
                file = 0;
            }
            '1'..='8' => file += ch.to_digit(10).unwrap_or(0) as usize,
            _ => {
                if (0..8).contains(&rank) && file < 8 {
                    out[rank as usize * 8 + file] = Some(ch);
                    file += 1;
                }
            }
        }
    }
    out
}

pub fn fen_from_editor(
    pieces: &[Option<char>; 64],
    turn: &str,
    castling: &str,
    ep: &str,
) -> String {
    let mut placement = String::new();
    for rank in (0..8).rev() {
        let mut empty = 0;
        for file in 0..8 {
            match pieces[rank * 8 + file] {
                Some(ch) => {
                    if empty > 0 {
                        placement.push_str(&empty.to_string());
                        empty = 0;
                    }
                    placement.push(ch);
                }
                None => empty += 1,
            }
        }
        if empty > 0 {
            placement.push_str(&empty.to_string());
        }
        if rank > 0 {
            placement.push('/');
        }
    }
    let castling = if castling.is_empty() { "-" } else { castling };
    let ep = if ep.trim().is_empty() { "-" } else { ep.trim() };
    format!("{placement} {turn} {castling} {ep} 0 1")
}

pub fn piece_glyph(ch: char) -> &'static str {
    match ch {
        'K' => "♔", 'Q' => "♕", 'R' => "♖", 'B' => "♗", 'N' => "♘", 'P' => "♙",
        'k' => "♚", 'q' => "♛", 'r' => "♜", 'b' => "♝", 'n' => "♞", 'p' => "♟",
        _ => "·",
    }
}

pub fn parse_time_control(value: &str) -> Option<(i64, i64)> {
    let value = value.trim();
    if value.is_empty() || value.contains('/') {
        return None;
    }
    let (base, inc) = value.split_once('+').unwrap_or((value, "0"));
    let base = base.parse::<i64>().ok()?;
    let inc = inc.parse::<i64>().unwrap_or(0);
    (base > 0 && base <= 86_400).then_some((base, inc))
}

pub fn clock_pgn(seconds: i64) -> String {
    let seconds = seconds.max(0);
    format!(
        "{}:{:02}:{:02}",
        seconds / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    )
}

pub fn format_clock(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    }
}

pub fn eval_text(row: &api::EvalRow) -> String {
    if let Some(mate) = row.mate {
        format!("M{mate}")
    } else {
        format!("{:+.2}", row.cp.unwrap_or(0) as f64 / 100.0)
    }
}

pub fn eval_point(row: &api::EvalRow) -> (f64, f64) {
    // Engine scores are from the side to move; the graph is White's POV.
    let white = if row.ply % 2 == 1 {
        -eval_value(row)
    } else {
        eval_value(row)
    };
    let clamped = (white / 800.0).clamp(-1.0, 1.0);
    (row.ply as f64, (1.0 - clamped) * 45.0 + 5.0)
}
