//! RustyBrickChess frontend — a Leptos, En-Croissant-shaped app.

pub mod api;
pub mod board;
pub mod sound;

use brick_chess::scene::index_to_name;
use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

use board::{Board, Keybinds};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Home,
    Database,
    Analysis,
    Practice,
    Openings,
    Puzzles,
    Stats,
    Settings,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Workspace {
    key: u32,
    game_id: Option<i64>,
    title: String,
    pgn: String,
    ply: usize,
    variant: String,
    preview_only: bool,
    moves: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Session {
    workspaces: Vec<Workspace>,
    active: u32,
    seq: u32,
    tab: String,
}

const SESSION_KEY: &str = "rbc.session.v1";

fn tab_name(tab: Tab) -> &'static str {
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

fn tab_from_name(name: &str) -> Option<Tab> {
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

fn read_session() -> Option<Session> {
    let storage = web_sys::window()?.local_storage().ok().flatten()?;
    let raw = storage.get_item(SESSION_KEY).ok().flatten()?;
    serde_json::from_str(&raw).ok()
}

fn write_session(session: &Session) {
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
struct TreeItem {
    path: String,
    ply: usize,
    san: String,
    depth: usize,
    line: Vec<String>,
    clock: Option<String>,
}

fn clock_of(node: &api::PgnNode) -> Option<String> {
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

fn flatten_nodes(
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

fn tree_items(nodes: &[api::PgnNode]) -> Vec<TreeItem> {
    let mut out = Vec::new();
    flatten_nodes(nodes, "", 0, 0, &mut Vec::new(), &mut out);
    out
}

fn plain_items(sans: &[String]) -> Vec<TreeItem> {
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

fn line_pgn(fen: &str, sans: &[String], variant: &str) -> String {
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

fn find_annotation<'a>(
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

fn annotation_shapes(ann: Option<&api::Annotation>) -> Vec<brick_chess::Shape> {
    ann.and_then(|a| a.shapes.as_deref())
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default()
}

fn eval_value(row: &api::EvalRow) -> f64 {
    if let Some(mate) = row.mate {
        if mate > 0 { 1200.0 } else { -1200.0 }
    } else {
        row.cp.unwrap_or(0) as f64
    }
}

fn pieces_from_fen(fen: &str) -> [Option<char>; 64] {
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

fn fen_from_editor(
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

fn piece_glyph(ch: char) -> &'static str {
    match ch {
        'K' => "♔", 'Q' => "♕", 'R' => "♖", 'B' => "♗", 'N' => "♘", 'P' => "♙",
        'k' => "♚", 'q' => "♛", 'r' => "♜", 'b' => "♝", 'n' => "♞", 'p' => "♟",
        _ => "·",
    }
}

fn parse_time_control(value: &str) -> Option<(i64, i64)> {
    let value = value.trim();
    if value.is_empty() || value.contains('/') {
        return None;
    }
    let (base, inc) = value.split_once('+').unwrap_or((value, "0"));
    let base = base.parse::<i64>().ok()?;
    let inc = inc.parse::<i64>().unwrap_or(0);
    (base > 0 && base <= 86_400).then_some((base, inc))
}

fn clock_pgn(seconds: i64) -> String {
    let seconds = seconds.max(0);
    format!(
        "{}:{:02}:{:02}",
        seconds / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    )
}

fn format_clock(seconds: i64) -> String {
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

fn eval_text(row: &api::EvalRow) -> String {
    if let Some(mate) = row.mate {
        format!("M{mate}")
    } else {
        format!("{:+.2}", row.cp.unwrap_or(0) as f64 / 100.0)
    }
}

fn eval_point(row: &api::EvalRow) -> (f64, f64) {
    // Engine scores are from the side to move; the graph is White's POV.
    let white = if row.ply % 2 == 1 {
        -eval_value(row)
    } else {
        eval_value(row)
    };
    let clamped = (white / 800.0).clamp(-1.0, 1.0);
    (row.ply as f64, (1.0 - clamped) * 45.0 + 5.0)
}

#[component]
pub fn App() -> impl IntoView {
    let tab = RwSignal::new(Tab::Home);
    let workspaces = RwSignal::new(Vec::<Workspace>::new());
    let active_ws = RwSignal::new(0u32);
    let ws_seq = RwSignal::new(1u32);
    let session_restored = RwSignal::new(false);
    let profile_name = RwSignal::new("Player".to_string());
    let profile_player = RwSignal::new(String::new());
    let profile = RwSignal::new(None::<api::Profile>);
    let keybinds = RwSignal::new(Keybinds::default());
    let home_recent = RwSignal::new(Vec::<api::GameSummary>::new());
    let games = RwSignal::new(Vec::<api::GameSummary>::new());
    let total = RwSignal::new(0i64);
    let query = RwSignal::new(String::new());
    let status = RwSignal::new(String::new());
    let import_text = RwSignal::new(String::new());
    let selected_pgn = RwSignal::new(String::new());
    let game_detail = RwSignal::new(None::<api::GameDetail>);
    let tree_current = RwSignal::new(String::new());
    let ann_ply = RwSignal::new(0i64);
    let evals = RwSignal::new(Vec::<api::EvalRow>::new());
    let eval_hover = RwSignal::new(None::<usize>);
    let shapes = RwSignal::new(Vec::<brick_chess::Shape>::new());
    let goto_seq = RwSignal::new(0u32);
    let settings_theme = RwSignal::new("green".to_string());
    let settings_pieces = RwSignal::new("cburnett".to_string());
    let settings_threads = RwSignal::new(0u32);
    let settings_hash = RwSignal::new(0u32);
    let settings_syzygy = RwSignal::new(String::new());
    let settings_variant_engine = RwSignal::new(String::new());
    let settings_online = RwSignal::new(false);
    let animate_last_move = RwSignal::new(false);
    let sound_enabled = RwSignal::new(true);
    let sound_theme = RwSignal::new("sfx".to_string());
    let sound_volume = RwSignal::new(70u32);
    let piece_animation_ms = RwSignal::new(0u32);
    let puzzle_theme = RwSignal::new(String::new());
    let puzzle_themes = RwSignal::new(Vec::<api::ThemeCount>::new());
    let puzzle_pgn = RwSignal::new(String::new());
    let puzzle_orientation = RwSignal::new("white".to_string());
    let puzzle_solution = RwSignal::new(Vec::<String>::new());
    let puzzle_san = RwSignal::new(Vec::<String>::new());
    let puzzle_step = RwSignal::new(0usize);
    let puzzle_state = RwSignal::new(String::new());
    let puzzle_recorded = RwSignal::new(false);
    let puzzle_delta = RwSignal::new(None::<(i32, i32)>);
    let collections = RwSignal::new(Vec::<api::Collection>::new());
    let active_collection = RwSignal::new(None::<i64>);
    let new_collection = RwSignal::new(String::new());
    let add_collection = RwSignal::new(0i64);
    let players = RwSignal::new(Vec::<api::PlayerRow>::new());
    let events = RwSignal::new(Vec::<api::EventRow>::new());
    let event_filter = RwSignal::new(None::<String>);
    let engines = RwSignal::new(Vec::<api::EngineEntry>::new());
    let new_engine_name = RwSignal::new(String::new());
    let new_engine_path = RwSignal::new(String::new());
    let show_tags = RwSignal::new(false);
    let tag_event = RwSignal::new(String::new());
    let tag_site = RwSignal::new(String::new());
    let tag_date = RwSignal::new(String::new());
    let tag_round = RwSignal::new(String::new());
    let tag_white = RwSignal::new(String::new());
    let tag_black = RwSignal::new(String::new());
    let tag_result = RwSignal::new(String::new());
    let tag_white_elo = RwSignal::new(String::new());
    let tag_black_elo = RwSignal::new(String::new());
    let tag_time_control = RwSignal::new(String::new());
    let vs_engine = RwSignal::new(false);
    let play_as = RwSignal::new("white".to_string());
    let play_uci = RwSignal::new(None::<(u32, String)>);
    let play_seq = RwSignal::new(0u32);
    let thinking = RwSignal::new(false);
    let editor_open = RwSignal::new(false);
    let editor_pieces = RwSignal::new([None::<char>; 64]);
    let editor_palette = RwSignal::new(Some('P'));
    let editor_turn = RwSignal::new("w".to_string());
    let editor_castling = RwSignal::new("KQkq".to_string());
    let editor_ep = RwSignal::new("-".to_string());
    let load_fen_sig = RwSignal::new(None::<(u32, String, String)>);
    let load_seq = RwSignal::new(0u32);
    let tb_result = RwSignal::new(None::<api::Tablebase>);
    let tb_status = RwSignal::new(String::new());
    let clock = RwSignal::new(None::<(i64, i64)>);
    let clock_inc = RwSignal::new(0i64);
    let clock_epoch = RwSignal::new(0u32);
    let clock_game = RwSignal::new(None::<i64>);
    let clock_active = RwSignal::new(false);
    let clock_base_input = RwSignal::new(180u32);
    let clock_inc_input = RwSignal::new(0u32);
    let variant_only = RwSignal::new(false);
    let board_variant = RwSignal::new("standard".to_string());
    let editor_variant = RwSignal::new("standard".to_string());
    let vision = RwSignal::new(None::<api::VisionStatus>);
    let vision_busy = RwSignal::new(false);
    let vision_msg = RwSignal::new(String::new());
    let sound_msg = RwSignal::new(String::new());
    let record_moves = RwSignal::new(false);
    let db_list = RwSignal::new(Vec::<api::DatabaseInfo>::new());
    let stats = RwSignal::new(api::Stats::default());
    let opening_source = RwSignal::new("local".to_string());
    let attempts = RwSignal::new(Vec::<api::PuzzleAttempt>::new());
    let online_provider = RwSignal::new("lichess".to_string());
    let online_user = RwSignal::new(String::new());
    let online_max = RwSignal::new(30u32);
    let partial_search = RwSignal::new(false);
    let position_variant = RwSignal::new("standard".to_string());
    let engine_results = RwSignal::new(Vec::<api::EngineAnalysis>::new());
    let opening_mode = RwSignal::new("explorer".to_string());
    let reps = RwSignal::new(Vec::<api::Repertoire>::new());
    let rep_name = RwSignal::new(String::new());
    let rep_color = RwSignal::new("white".to_string());
    let rep_selected = RwSignal::new(0i64);
    let rep_pgn_input = RwSignal::new(String::new());
    let rep_id = RwSignal::new(0i64);
    let rep_card = RwSignal::new(None::<api::RepertoireCard>);
    let rep_board = RwSignal::new(String::new());
    let rep_feedback = RwSignal::new(String::new());
    let rep_correct = RwSignal::new(false);
    let analysis_rep = RwSignal::new(0i64);
    let manage_collection = RwSignal::new(0i64);
    let collection_name = RwSignal::new(String::new());
    let edit_moves_open = RwSignal::new(false);
    let moves_text = RwSignal::new(String::new());
    let new_database = RwSignal::new(String::new());
    let position_fen = RwSignal::new(String::new());
    let practice_mode = RwSignal::new("guess".to_string());
    let practice_game = RwSignal::new(None::<api::GameDetail>);
    let practice_sans = RwSignal::new(Vec::<String>::new());
    let practice_step = RwSignal::new(0usize);
    let practice_hits = RwSignal::new(0u32);
    let practice_misses = RwSignal::new(0u32);
    let practice_feedback = RwSignal::new(String::new());
    let practice_pgn = RwSignal::new(String::new());
    let practice_goto = RwSignal::new(None::<(u32, usize)>);
    let practice_seq = RwSignal::new(0u32);
    let practice_pick = RwSignal::new(0i64);
    let practice_query = RwSignal::new(String::new());
    let practice_games = RwSignal::new(Vec::<api::GameSummary>::new());
    let spar_pgn = RwSignal::new(String::new());
    let spar_fen = RwSignal::new(String::new());
    let spar_color = RwSignal::new("white".to_string());
    let spar_play = RwSignal::new(None::<(u32, String)>);
    let spar_seq = RwSignal::new(0u32);
    let spar_thinking = RwSignal::new(false);
    let spar_status = RwSignal::new(String::new());
    let filter_date_from = RwSignal::new(String::new());
    let filter_date_to = RwSignal::new(String::new());
    let filter_result = RwSignal::new(String::new());
    let filter_elo_min = RwSignal::new(String::new());
    let filter_elo_max = RwSignal::new(String::new());
    let filter_variant = RwSignal::new("any".to_string());
    let sort_by = RwSignal::new("newest".to_string());
    let fen = RwSignal::new(String::new());
    let analysis = RwSignal::new(None::<api::Analysis>);
    let analyzing = RwSignal::new(false);
    let selected_game_id = RwSignal::new(None::<i64>);
    let report = RwSignal::new(None::<api::GameReport>);
    let reporting = RwSignal::new(false);
    let opening = RwSignal::new(None::<api::Explorer>);
    let opening_fen = RwSignal::new(
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".to_string(),
    );
    let puzzle = RwSignal::new(None::<api::Puzzle>);
    let show_solution = RwSignal::new(false);
    let puzzle_rating = RwSignal::new(1500i32);
    let puzzle_csv = RwSignal::new(String::new());
    let settings_engine = RwSignal::new(String::new());
    let settings_model = RwSignal::new(String::new());
    let settings_depth = RwSignal::new(15u32);
    let settings_status = RwSignal::new(String::new());
    let selected_moves = RwSignal::new(Vec::<String>::new());
    let selected_title = RwSignal::new(String::new());
    let tree_ply = RwSignal::new(0usize);
    let goto_sig = RwSignal::new(None::<(u32, usize)>);
    let annotations = RwSignal::new(Vec::<api::Annotation>::new());
    let ann_text = RwSignal::new(String::new());
    let ann_nag = RwSignal::new(0i64);
    let lines = RwSignal::new(Vec::<api::EngineLine>::new());
    let chat_text = RwSignal::new(String::new());
    let dock = RwSignal::new(String::new());
    let chat_messages = RwSignal::new(Vec::<(String, String)>::new());
    let chat_tools = RwSignal::new(Vec::<String>::new());
    let ai_shapes = RwSignal::new(Vec::<brick_chess::Shape>::new());
    let console_running = RwSignal::new(false);
    let console_name = RwSignal::new(String::new());
    let console_log = RwSignal::new(Vec::<String>::new());
    let console_cmd = RwSignal::new(String::new());
    let console_path = RwSignal::new(String::new());
    let console_busy = RwSignal::new(false);
    let engine_dialog = RwSignal::new(None::<usize>);
    let engine_test = RwSignal::new(None::<api::EngineInfo>);
    let engine_test_busy = RwSignal::new(false);

    let reload = move || {
        let q = query.get();
        let collection = active_collection.get();
        let event = event_filter.get();
        spawn_local(async move {
            let filters = api::Filters {
                player: q.clone(),
                event: event.clone(),
                collection,
                date_from: filter_date_from.get_untracked(),
                date_to: filter_date_to.get_untracked(),
                result: filter_result.get_untracked(),
                elo_min: filter_elo_min.get_untracked(),
                elo_max: filter_elo_max.get_untracked(),
                variant: filter_variant.get_untracked(),
                sort: sort_by.get_untracked(),
            };
            match api::list_games(&filters).await {
                Ok((g, t)) => {
                    games.set(g);
                    total.set(t);
                }
                Err(e) => status.set(e),
            }
        });
        spawn_local(async move {
            if let Ok(list) = api::databases().await {
                db_list.set(list);
            }
            if let Ok(s) = api::stats().await {
                stats.set(s);
            }
            if let Ok(a) = api::puzzle_attempts(8).await {
                attempts.set(a);
            }
            if let Ok(r) = api::repertoires().await {
                reps.set(r);
            }
            if let Ok(c) = api::collections().await {
                collections.set(c);
            }
            if let Ok(p) = api::players().await {
                players.set(p);
            }
            if let Ok(e) = api::events().await {
                events.set(e);
            }
        });
    };

    Effect::new(move |_| reload());

    let add_collection_clicked = move |_| {
        let name = new_collection.get();
        if name.trim().is_empty() {
            return;
        }
        spawn_local(async move {
            match api::create_collection(name.trim()).await {
                Ok(c) => {
                    new_collection.set(String::new());
                    status.set(format!("collection '{}' created", c.name));
                    reload();
                }
                Err(e) => status.set(e),
            }
        });
    };

    let switch_db = move |ev: leptos::ev::Event| {
        let name = event_target_value(&ev);
        if name.is_empty() {
            return;
        }
        spawn_local(async move {
            match api::switch_database(&name).await {
                Ok(()) => {
                    query.set(String::new());
                    active_collection.set(None);
                    event_filter.set(None);
                    selected_game_id.set(None);
                    selected_pgn.set(String::new());
                    game_detail.set(None);
                    status.set(format!("database: {name}"));
                    reload();
                }
                Err(e) => status.set(e),
            }
        });
    };

    let create_db = move |_: leptos::ev::MouseEvent| {
        let name = new_database.get();
        if name.trim().is_empty() {
            return;
        }
        spawn_local(async move {
            match api::create_database(name.trim()).await {
                Ok(()) => {
                    new_database.set(String::new());
                    query.set(String::new());
                    active_collection.set(None);
                    selected_game_id.set(None);
                    selected_pgn.set(String::new());
                    game_detail.set(None);
                    status.set(format!("database '{}' ready", name.trim()));
                    reload();
                }
                Err(e) => status.set(e),
            }
        });
    };

    let rename_collection = move |_: leptos::ev::MouseEvent| {
        let id = manage_collection.get();
        let name = collection_name.get();
        if id <= 0 || name.trim().is_empty() {
            return;
        }
        spawn_local(async move {
            match api::rename_collection(id, name.trim()).await {
                Ok(()) => {
                    collection_name.set(String::new());
                    status.set("collection renamed".into());
                    reload();
                }
                Err(e) => status.set(e),
            }
        });
    };

    let delete_collection = move |_: leptos::ev::MouseEvent| {
        let id = manage_collection.get();
        if id <= 0 {
            return;
        }
        let confirmed = web_sys::window()
            .and_then(|w| w.confirm_with_message("Delete this collection?").ok())
            .unwrap_or(false);
        if !confirmed {
            return;
        }
        spawn_local(async move {
            match api::delete_collection(id).await {
                Ok(()) => {
                    if active_collection.get_untracked() == Some(id) {
                        active_collection.set(None);
                    }
                    manage_collection.set(0);
                    status.set("collection deleted".into());
                    reload();
                }
                Err(e) => status.set(e),
            }
        });
    };

    let search_position = move |_: leptos::ev::MouseEvent| {
        let f = position_fen.get();
        if f.trim().is_empty() {
            return;
        }
        spawn_local(async move {
            let partial = partial_search.get_untracked();
            let variant = position_variant.get_untracked();
            match api::search_position(&f, partial, &variant).await {
                Ok(found) => {
                    total.set(found.len() as i64);
                    games.set(found);
                    status.set("position search".into());
                }
                Err(e) => status.set(e),
            }
        });
    };

    let add_game_to_collection = move |_: leptos::ev::MouseEvent| {
        let Some(gid) = selected_game_id.get() else { return };
        let cid = add_collection.get();
        if cid <= 0 {
            return;
        }
        spawn_local(async move {
            match api::add_to_collection(cid, gid).await {
                Ok(()) => {
                    status.set("added to collection".into());
                    reload();
                }
                Err(e) => status.set(e),
            }
        });
    };

    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(s) = api::get_settings().await {
                settings_engine.set(s.engine_path.unwrap_or_default());
                settings_model.set(s.model_url.unwrap_or_default());
                settings_depth.set(s.depth);
                settings_theme.set(s.theme);
                settings_pieces.set(s.pieces);
                settings_threads.set(s.threads);
                settings_hash.set(s.hash_mb);
                settings_syzygy.set(s.syzygy_path.unwrap_or_default());
                settings_variant_engine.set(s.variant_engine_path.unwrap_or_default());
                settings_online.set(s.online_explorer);
                animate_last_move.set(s.animate_last_move);
                sound_enabled.set(s.sound_enabled);
                sound_theme.set(s.sound_theme);
                sound_volume.set(s.sound_volume);
                piece_animation_ms.set(s.piece_animation_ms);
                puzzle_rating.set(s.puzzle_rating);
                profile_name.set(s.profile_name);
                profile_player.set(s.profile_player.unwrap_or_default());
                if let Ok(binds) = serde_json::from_str::<Keybinds>(&s.keybinds) {
                    keybinds.set(binds);
                }
                engines.set(s.engines);
            }
        });
    });

    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(status) = api::vision_status().await {
                vision.set(Some(status));
            }
        });
    });

    Effect::new(move |_| {
        sound::configure(
            sound_enabled.get(),
            &sound_theme.get(),
            sound_volume.get(),
        );
    });

    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(t) = api::puzzle_themes().await {
                puzzle_themes.set(t);
            }
        });
    });

    Effect::new(move |_| {
        if tab.get() == Tab::Home {
            spawn_local(async move {
                if let Ok((list, _)) = api::list_games(&api::Filters::default()).await {
                    home_recent.set(list.into_iter().take(6).collect());
                }
            });
        }
    });

    Effect::new(move |_| {
        let query = practice_query.get();
        spawn_local(async move {
            let filters = api::Filters {
                player: query,
                ..api::Filters::default()
            };
            if let Ok((list, _)) = api::list_games(&filters).await {
                practice_games.set(list);
            }
        });
    });

    Effect::new(move |_| {
        let player = profile_player.get();
        if player.trim().is_empty() {
            profile.set(None);
            return;
        }
        spawn_local(async move {
            match api::profile(player.trim()).await {
                Ok(p) => profile.set(Some(p)),
                Err(_) => profile.set(None),
            }
        });
    });

    // --- workspaces: one persisted board per analysed game -----------------
    let save_workspace = move || {
        let key = active_ws.get_untracked();
        if key == 0 {
            return;
        }
        let ply = tree_ply.get_untracked();
        let pgn = selected_pgn.get_untracked();
        workspaces.update(|list| {
            if let Some(ws) = list.iter_mut().find(|w| w.key == key) {
                ws.ply = ply;
                ws.pgn = pgn;
            }
        });
    };

    let remember_workspace = move |game_id: Option<i64>,
                                   title: String,
                                   pgn: String,
                                   moves: Vec<String>,
                                   variant: String,
                                   preview_only: bool| {
        let existing = workspaces.with_untracked(|list| {
            game_id.and_then(|id| list.iter().find(|w| w.game_id == Some(id)).map(|w| w.key))
        });
        let key = match existing {
            Some(key) => key,
            None => {
                let key = ws_seq.get_untracked();
                ws_seq.set(key + 1);
                workspaces.update(|list| {
                    list.push(Workspace {
                        key,
                        game_id,
                        title: title.clone(),
                        pgn: pgn.clone(),
                        ply: 0,
                        variant: variant.clone(),
                        preview_only,
                        moves: moves.clone(),
                    })
                });
                key
            }
        };
        workspaces.update(|list| {
            if let Some(ws) = list.iter_mut().find(|w| w.key == key) {
                ws.title = title;
                ws.pgn = pgn;
                ws.moves = moves;
                ws.variant = variant;
                ws.preview_only = preview_only;
                ws.ply = 0;
            }
        });
        active_ws.set(key);
    };

    let activate_workspace = move |key: u32| {
        save_workspace();
        let Some(ws) = workspaces
            .get_untracked()
            .into_iter()
            .find(|w| w.key == key)
        else {
            return;
        };
        active_ws.set(key);
        selected_pgn.set(ws.pgn.clone());
        selected_title.set(ws.title.clone());
        selected_game_id.set(ws.game_id);
        selected_moves.set(ws.moves.clone());
        board_variant.set(ws.variant.clone());
        editor_variant.set(ws.variant.clone());
        variant_only.set(ws.preview_only);
        tree_ply.set(ws.ply);
        goto_sig.set(Some((goto_seq.get_untracked() + 1, ws.ply)));
        report.set(None);
        evals.set(Vec::new());
        lines.set(Vec::new());
        analysis.set(None);
        annotations.set(Vec::new());
        shapes.set(Vec::new());
        tree_current.set(String::new());
        ann_ply.set(0);
        vs_engine.set(false);
        tb_result.set(None);
        tb_status.set(String::new());
        add_collection.set(0);
        if let Some(id) = ws.game_id {
            spawn_local(async move {
                if let Ok(a) = api::annotations(id).await {
                    annotations.set(a);
                }
                if let Ok(d) = api::get_game(id).await {
                    game_detail.set(Some(d));
                }
                if let Ok(e) = api::evals(id, None).await {
                    evals.set(e);
                }
            });
        } else {
            game_detail.set(None);
        }
        tab.set(Tab::Analysis);
    };

    let close_workspace = move |key: u32| {
        let was_active = active_ws.get_untracked() == key;
        let next = workspaces.with_untracked(|list| {
            let idx = list.iter().position(|w| w.key == key);
            idx.and_then(|i| {
                list.get(i + 1)
                    .or_else(|| i.checked_sub(1).and_then(|j| list.get(j)))
                    .map(|w| w.key)
            })
        });
        workspaces.update(|list| list.retain(|w| w.key != key));
        if was_active {
            match next {
                Some(key) => activate_workspace(key),
                None => {
                    active_ws.set(0);
                    selected_pgn.set(String::new());
                    selected_title.set(String::new());
                    selected_game_id.set(None);
                    selected_moves.set(Vec::new());
                    tree_ply.set(0);
                    tab.set(Tab::Home);
                }
            }
        }
    };

    let new_workspace = move || {
        save_workspace();
        let key = ws_seq.get_untracked();
        ws_seq.set(key + 1);
        let start = line_pgn(
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            &[],
            "standard",
        );
        workspaces.update(|list| {
            list.push(Workspace {
                key,
                game_id: None,
                title: "New analysis".into(),
                pgn: start.clone(),
                ply: 0,
                variant: "standard".into(),
                preview_only: false,
                moves: Vec::new(),
            })
        });
        active_ws.set(key);
        selected_pgn.set(start);
        selected_title.set("New analysis".into());
        selected_game_id.set(None);
        selected_moves.set(Vec::new());
        game_detail.set(None);
        board_variant.set("standard".into());
        variant_only.set(false);
        tree_ply.set(0);
        goto_sig.set(Some((goto_seq.get_untracked() + 1, 0)));
        report.set(None);
        evals.set(Vec::new());
        lines.set(Vec::new());
        analysis.set(None);
        annotations.set(Vec::new());
        shapes.set(Vec::new());
        tree_current.set(String::new());
        vs_engine.set(false);
        tb_result.set(None);
        tb_status.set(String::new());
        tab.set(Tab::Analysis);
    };

    let switch_tab = move |next: Tab| {
        if tab.get_untracked() == Tab::Analysis && next != Tab::Analysis {
            save_workspace();
        }
        if next == Tab::Analysis && tree_ply.get_untracked() > 0 {
            goto_sig.set(Some((goto_seq.get_untracked() + 1, tree_ply.get_untracked())));
        }
        tab.set(next);
    };

    // Persist the workspace session (tabs, active tab, current ply) and restore
    // it on the next load, so reopening the app returns to the same boards.
    Effect::new(move |_| {
        let mut list = workspaces.get();
        let active = active_ws.get();
        let tab_now = tab.get();
        let ply = tree_ply.get();
        if let Some(ws) = list.iter_mut().find(|w| w.key == active) {
            ws.ply = ply;
            let pgn = selected_pgn.get();
            if !pgn.is_empty() {
                ws.pgn = pgn;
            }
        }
        let session = Session {
            workspaces: list,
            active,
            seq: ws_seq.get(),
            tab: tab_name(tab_now).to_owned(),
        };
        spawn_local(async move {
            TimeoutFuture::new(350).await;
            write_session(&session);
        });
    });

    Effect::new(move |_| {
        if session_restored.get_untracked() {
            return;
        }
        session_restored.set(true);
        let Some(session) = read_session() else {
            return;
        };
        if session.workspaces.is_empty() {
            return;
        }
        let seq = session.seq.max(
            session
                .workspaces
                .iter()
                .map(|w| w.key + 1)
                .max()
                .unwrap_or(1),
        );
        ws_seq.set(seq);
        workspaces.set(session.workspaces.clone());
        let active = session
            .workspaces
            .iter()
            .find(|w| w.key == session.active)
            .map(|w| w.key)
            .unwrap_or_else(|| session.workspaces[0].key);
        let resume_analysis = tab_from_name(&session.tab) == Some(Tab::Analysis);
        if resume_analysis {
            activate_workspace(active);
        } else {
            active_ws.set(active);
            if let Some(ws) = session.workspaces.iter().find(|w| w.key == active) {
                selected_pgn.set(ws.pgn.clone());
                selected_title.set(ws.title.clone());
                selected_game_id.set(ws.game_id);
                selected_moves.set(ws.moves.clone());
                board_variant.set(ws.variant.clone());
                variant_only.set(ws.preview_only);
                tree_ply.set(ws.ply);
                goto_sig.set(Some((goto_seq.get_untracked() + 1, ws.ply)));
            }
            if let Some(t) = tab_from_name(&session.tab) {
                tab.set(t);
            }
        }
    });

    // The desktop shell (Tauri) dispatches this after a native File > Open…
    {
        let on_import = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
            let message = event
                .dyn_ref::<web_sys::CustomEvent>()
                .and_then(|custom| custom.detail().as_string())
                .unwrap_or_default();
            reload();
            if !message.is_empty() {
                status.set(message);
            }
        });
        if let Some(window) = web_sys::window() {
            let _ = window.add_event_listener_with_callback(
                "rbc-imported",
                on_import.as_ref().unchecked_ref(),
            );
        }
        on_import.forget();
    }

    let open_game = move |g: api::GameSummary| {
        save_workspace();
        let sans: Vec<String> = g.moves.split_whitespace().map(str::to_owned).collect();
        let preview_only = g
            .variant
            .as_deref()
            .map(|v| {
                !matches!(
                    v,
                    "standard"
                        | "chess960"
                        | "atomic"
                        | "antichess"
                        | "kingofthehill"
                        | "3check"
                        | "crazyhouse"
                        | "racingkings"
                        | "horde"
                )
            })
            .unwrap_or(false);
        let variant_name = g.variant.clone().unwrap_or_else(|| "standard".into());
        board_variant.set(variant_name.clone());
        editor_variant.set(variant_name.clone());
        variant_only.set(preview_only);
        if preview_only {
            selected_pgn.set(line_pgn(&g.fen, &[], &variant_name));
            status.set(format!(
                "variant {}: imported, preview only",
                g.variant.clone().unwrap_or_default()
            ));
        } else {
            selected_pgn.set(line_pgn(&g.fen, &sans, &variant_name));
        }
        selected_game_id.set(Some(g.id));
        selected_moves.set(sans.clone());
        selected_title.set(if let Some(v) = g.variant.as_deref().filter(|v| *v != "standard") {
            format!("{} vs {} · {} · {}", g.white, g.black, g.result, v)
        } else {
            format!("{} vs {} · {}", g.white, g.black, g.result)
        });
        remember_workspace(
            Some(g.id),
            selected_title.get_untracked(),
            selected_pgn.get_untracked(),
            sans.clone(),
            variant_name.clone(),
            preview_only,
        );
        report.set(None);
        evals.set(Vec::new());
        lines.set(Vec::new());
        analysis.set(None);
        tree_current.set(String::new());
        tree_ply.set(0);
        ann_ply.set(0);
        shapes.set(Vec::new());
        vs_engine.set(false);
        tb_result.set(None);
        tb_status.set(String::new());
        goto_sig.set(Some((goto_seq.get_untracked() + 1, 0)));
        let id = g.id;
        add_collection.set(0);
        spawn_local(async move {
            if let Ok(a) = api::annotations(id).await {
                annotations.set(a);
            }
            if let Ok(d) = api::get_game(id).await {
                game_detail.set(Some(d));
            }
            if let Ok(e) = api::evals(id, None).await {
                evals.set(e);
            }
        });
        tab.set(Tab::Analysis);
    };

    let import = move |_| {
        let text = import_text.get();
        if text.trim().is_empty() {
            return;
        }
        spawn_local(async move {
            match api::import_pgn(&text).await {
                Ok(n) => {
                    status.set(format!("imported {n} game(s)"));
                    import_text.set(String::new());
                    reload();
                }
                Err(e) => status.set(e),
            }
        });
    };

    let import_online = move |_: leptos::ev::MouseEvent| {
        let user = online_user.get();
        if user.trim().is_empty() {
            return;
        }
        let provider = online_provider.get_untracked();
        let max = online_max.get_untracked();
        status.set("importing…".into());
        spawn_local(async move {
            let result = match provider.as_str() {
                "chesscom" => api::import_chesscom(user.trim(), max).await,
                _ => api::import_lichess(user.trim(), max).await,
            };
            match result {
                Ok((n, skipped)) => {
                    status.set(if skipped > 0 {
                        format!("imported {n} game(s) from {provider}, skipped {skipped} unsupported")
                    } else {
                        format!("imported {n} game(s) from {provider}")
                    });
                    reload();
                }
                Err(e) => status.set(e),
            }
        });
    };

    let run_analysis = move |_| {
        let f = fen.get();
        if f.is_empty() {
            return;
        }
        analyzing.set(true);
        spawn_local(async move {
            let depth = settings_depth.get_untracked();
            match api::analyze(&f, depth, &board_variant.get_untracked()).await {
                Ok(a) => {
                    let ply = tree_ply.get_untracked() as i64;
                    let row = api::EvalRow {
                        ply,
                        depth: a.depth,
                        cp: a.score_cp,
                        mate: a.mate,
                    };
                    evals.update(|v| {
                        match v.iter_mut().find(|r| r.ply == ply) {
                            Some(slot) => *slot = row,
                            None => {
                                v.push(row);
                                v.sort_by_key(|r| r.ply);
                            }
                        }
                    });
                    analysis.set(Some(a));
                    analyzing.set(false);
                }
                Err(e) => {
                    status.set(e);
                    analyzing.set(false);
                }
            }
        });
    };

    let classify = move |_| {
        let Some(id) = selected_game_id.get() else {
            return;
        };
        reporting.set(true);
        spawn_local(async move {
            let depth = settings_depth.get_untracked();
            match api::game_report(id, depth).await {
                Ok(r) => {
                    report.set(Some(r));
                    reporting.set(false);
                    if let Ok(e) = api::evals(id, None).await {
                        evals.set(e);
                    }
                }
                Err(e) => {
                    status.set(e);
                    reporting.set(false);
                }
            }
        });
    };

    let save_tags = move |_: leptos::ev::MouseEvent| {
        let Some(id) = selected_game_id.get() else {
            return;
        };
        let text = |value: String| {
            let value = value.trim().to_owned();
            if value.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(value) }
        };
        let number = |value: String| {
            value.trim().parse::<i32>().ok().map(|v| serde_json::Value::from(v)).unwrap_or(serde_json::Value::Null)
        };
        let payload = serde_json::json!({
            "event": text(tag_event.get_untracked()),
            "site": text(tag_site.get_untracked()),
            "date": text(tag_date.get_untracked()),
            "round": text(tag_round.get_untracked()),
            "white": text(tag_white.get_untracked()),
            "black": text(tag_black.get_untracked()),
            "result": text(tag_result.get_untracked()),
            "white_elo": number(tag_white_elo.get_untracked()),
            "black_elo": number(tag_black_elo.get_untracked()),
            "time_control": text(tag_time_control.get_untracked()),
        });
        spawn_local(async move {
            match api::save_tags(id, payload).await {
                Ok(d) => {
                    status.set("tags saved".into());
                    selected_title.set(format!("{} vs {} · {}", d.white, d.black, d.result));
                    game_detail.set(Some(d));
                    show_tags.set(false);
                    reload();
                }
                Err(e) => status.set(e),
            }
        });
    };

    let add_line_to_repertoire = move |_: leptos::ev::MouseEvent| {
        let Some(repertoire) = Some(analysis_rep.get()).filter(|id| *id > 0) else {
            status.set("choose a repertoire first".into());
            return;
        };
        let Some(detail) = game_detail.get() else {
            return;
        };
        let path = tree_current.get();
        let items = detail
            .movetext
            .as_deref()
            .map(|json| tree_items(&api::parse_movetext(json)))
            .unwrap_or_default();
        let line = if path.is_empty() {
            Vec::new()
        } else {
            items
                .into_iter()
                .find(|item| item.path == path)
                .map(|item| item.line)
                .unwrap_or_default()
        };
        if line.is_empty() {
            status.set("select a move in the tree first".into());
            return;
        }
        let pgn = line_pgn(&detail.fen, &line, detail.variant.as_deref().unwrap_or("standard"));
        spawn_local(async move {
            match api::import_repertoire_pgn(repertoire, &pgn).await {
                Ok(n) => {
                    status.set(format!("added {n} card(s) to the repertoire"));
                    if let Ok(list) = api::repertoires().await {
                        reps.set(list);
                    }
                }
                Err(e) => status.set(e),
            }
        });
    };

    let adopt_detail = move |d: api::GameDetail| {
        let sans: Vec<String> = d.moves.split_whitespace().map(str::to_owned).collect();
        selected_pgn.set(line_pgn(&d.fen, &sans, d.variant.as_deref().unwrap_or("standard")));
        selected_moves.set(sans);
        selected_title.set(format!("{} vs {} · {}", d.white, d.black, d.result));
        game_detail.set(Some(d));
        evals.set(Vec::new());
        annotations.set(Vec::new());
        tree_current.set(String::new());
        tree_ply.set(0);
        ann_ply.set(0);
        load_fen_sig.set(None);
        play_uci.set(None);
        let seq = goto_seq.get_untracked() + 1;
        goto_seq.set(seq);
        goto_sig.set(Some((seq, 0)));
        edit_moves_open.set(false);
    };

    let apply_moves = move |_: leptos::ev::MouseEvent| {
        let Some(id) = selected_game_id.get() else {
            return;
        };
        let pgn = moves_text.get();
        spawn_local(async move {
            match api::update_tree(id, &pgn).await {
                Ok(d) => {
                    adopt_detail(d);
                    status.set("tree saved".into());
                    reload();
                }
                Err(e) => status.set(e),
            }
        });
    };

    let delete_tree_from_here = move |_: leptos::ev::MouseEvent| {
        let Some(id) = selected_game_id.get() else {
            return;
        };
        let path = tree_current.get();
        if path.is_empty() {
            status.set("select a move first".into());
            return;
        }
        spawn_local(async move {
            match api::delete_tree_node(id, &path).await {
                Ok(d) => {
                    adopt_detail(d);
                    status.set("line deleted".into());
                    reload();
                }
                Err(e) => status.set(e),
            }
        });
    };

    let promote_selected = move |_: leptos::ev::MouseEvent| {
        let Some(id) = selected_game_id.get() else {
            return;
        };
        let path = tree_current.get();
        if !path.contains('~') {
            status.set("select a variation first".into());
            return;
        }
        spawn_local(async move {
            match api::promote_variation(id, &path).await {
                Ok(d) => {
                    adopt_detail(d);
                    status.set("variation promoted".into());
                    reload();
                }
                Err(e) => status.set(e),
            }
        });
    };

    let truncate_moves = move |_: leptos::ev::MouseEvent| {
        let Some(id) = selected_game_id.get() else {
            return;
        };
        let ply = tree_ply.get();
        spawn_local(async move {
            match api::truncate_moves(id, ply).await {
                Ok(d) => {
                    adopt_detail(d);
                    status.set(format!("kept {ply} move(s)"));
                    reload();
                }
                Err(e) => status.set(e),
            }
        });
    };

    let add_game_to_repertoire = move |_: leptos::ev::MouseEvent| {
        let Some(repertoire) = Some(analysis_rep.get()).filter(|id| *id > 0) else {
            status.set("choose a repertoire first".into());
            return;
        };
        let Some(game) = selected_game_id.get() else {
            return;
        };
        spawn_local(async move {
            match api::import_repertoire_game(repertoire, game).await {
                Ok(n) => {
                    status.set(format!("added {n} card(s) from the whole game"));
                    if let Ok(list) = api::repertoires().await {
                        reps.set(list);
                    }
                }
                Err(e) => status.set(e),
            }
        });
    };

    let delete_current = move |_: leptos::ev::MouseEvent| {
        let Some(id) = selected_game_id.get() else {
            return;
        };
        let confirmed = web_sys::window()
            .and_then(|w| w.confirm_with_message("Delete this game?").ok())
            .unwrap_or(false);
        if !confirmed {
            return;
        }
        spawn_local(async move {
            match api::delete_game(id).await {
                Ok(()) => {
                    status.set("game deleted".into());
                    game_detail.set(None);
                    selected_pgn.set(String::new());
                    selected_game_id.set(None);
                    reload();
                    tab.set(Tab::Database);
                }
                Err(e) => status.set(e),
            }
        });
    };

    let fetch_lines = move |_| {
        let f = fen.get();
        if f.is_empty() {
            return;
        }
        spawn_local(async move {
            let depth = settings_depth.get_untracked();
            match api::engine_lines(&f, depth, 3, &board_variant.get_untracked()).await {
                Ok(l) => lines.set(l),
                Err(e) => status.set(e),
            }
        });
    };

    let fetch_engine_lines = move |_| {
        let f = fen.get();
        if f.is_empty() {
            return;
        }
        engine_results.set(Vec::new());
        spawn_local(async move {
            let depth = settings_depth.get_untracked();
            match api::analyze_engines(&f, depth, 3, &board_variant.get_untracked()).await {
                Ok(list) => engine_results.set(list),
                Err(e) => status.set(e),
            }
        });
    };

    let save_annotation = move |_| {
        let Some(id) = selected_game_id.get() else {
            return;
        };
        let path = tree_current.get();
        let ply = ann_ply.get();
        let text = ann_text.get();
        let nag = ann_nag.get();
        let shapes_json = {
            let list = shapes.get();
            if list.is_empty() {
                None
            } else {
                serde_json::to_string(&list).ok()
            }
        };
        spawn_local(async move {
            let comment = if text.trim().is_empty() {
                None
            } else {
                Some(text)
            };
            let nag = if nag == 0 { None } else { Some(nag) };
            match api::add_annotation(id, ply, Some(path), comment, nag, shapes_json).await {
                Ok(()) => {
                    status.set("annotation saved".into());
                    if let Ok(a) = api::annotations(id).await {
                        annotations.set(a);
                    }
                }
                Err(e) => status.set(e),
            }
        });
    };

    Effect::new(move |_| {
        let detail = game_detail.get();
        if let Some(d) = detail {
            tag_event.set(d.event.unwrap_or_default());
            tag_site.set(d.site.unwrap_or_default());
            tag_date.set(d.date.unwrap_or_default());
            tag_round.set(d.round.unwrap_or_default());
            tag_white.set(d.white.clone());
            tag_black.set(d.black.clone());
            tag_result.set(d.result.clone());
            tag_white_elo.set(d.white_elo.map(|v| v.to_string()).unwrap_or_default());
            tag_black_elo.set(d.black_elo.map(|v| v.to_string()).unwrap_or_default());
            let tc = d.time_control.clone().unwrap_or_default();
            tag_time_control.set(tc.clone());
            if clock_game.get_untracked() != Some(d.id) {
                clock_game.set(Some(d.id));
                if let Some((base, inc)) = parse_time_control(&tc) {
                    clock.set(Some((base, base)));
                    clock_inc.set(inc);
                    clock_base_input.set(base.max(0) as u32);
                    clock_inc_input.set(inc.max(0) as u32);
                    clock_active.set(false);
                    record_moves.set(true);
                } else {
                    clock.set(None);
                    clock_inc.set(0);
                    record_moves.set(false);
                }
            }
            selected_title.set(format!("{} vs {} · {}", d.white, d.black, d.result));
        }
    });

    // Keep the annotation editor and the drawing layer in sync with the node.
    Effect::new(move |_| {
        let path = tree_current.get();
        let ply = ann_ply.get();
        let anns = annotations.get();
        let found = find_annotation(&anns, &path, ply).cloned();
        ann_text.set(found.as_ref().and_then(|a| a.comment.clone()).unwrap_or_default());
        ann_nag.set(found.as_ref().and_then(|a| a.nag).unwrap_or(0));
        shapes.set(annotation_shapes(found.as_ref()));
    });

    let select_node = move |item: TreeItem| {
        tree_current.set(item.path.clone());
        ann_ply.set(item.ply as i64);
        tree_ply.set(item.ply);
        if variant_only.get_untracked() {
            status.set("variant preview: board navigation is unavailable".into());
            return;
        }
        clock_active.set(false);
        if item.depth == 0 {
            let full = game_detail
                .get()
                .map(|d| {
                    let sans: Vec<String> =
                        d.moves.split_whitespace().map(str::to_owned).collect();
                    line_pgn(&d.fen, &sans, d.variant.as_deref().unwrap_or("standard"))
                })
                .unwrap_or_default();
            if !full.is_empty() && full != selected_pgn.get_untracked() {
                selected_pgn.set(full);
            }
            let seq = goto_seq.get_untracked() + 1;
            goto_seq.set(seq);
            goto_sig.set(Some((seq, item.ply + 1)));
        } else {
            let fen = game_detail
                .get()
                .map(|d| d.fen)
                .unwrap_or_else(|| "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".into());
            selected_pgn.set(line_pgn(&fen, &item.line, game_detail.get().and_then(|d| d.variant).as_deref().unwrap_or("standard")));
            let seq = goto_seq.get_untracked() + 1;
            goto_seq.set(seq);
            goto_sig.set(Some((seq, item.line.len())));
        }
    };

    let tree_nodes = move || {
        let detail = game_detail.get();
        let mut items = detail
            .as_ref()
            .and_then(|d| d.movetext.as_deref())
            .map(|json| tree_items(&api::parse_movetext(json)))
            .unwrap_or_default();
        if items.is_empty() {
            items = plain_items(&selected_moves.get());
        }
        let current = tree_current.get();
        let labels: std::collections::BTreeMap<usize, String> = report
            .get()
            .map(|r| r.moves.iter().map(|m| (m.ply, m.lichess.clone())).collect())
            .unwrap_or_default();
        let anns = annotations.get();
        let start_cls = if current.is_empty() {
            "tnode current"
        } else {
            "tnode"
        };
        let rows = items
            .into_iter()
            .map(|item| {
                let label = if item.depth == 0 {
                    labels.get(&item.ply).cloned().unwrap_or_default()
                } else {
                    String::new()
                };
                let ann = find_annotation(&anns, &item.path, item.ply as i64);
                let marked = ann
                    .map(|a| {
                        a.comment.is_some()
                            || a.nag.unwrap_or(0) != 0
                            || a.shapes.as_deref().map(|s| s.len() > 2).unwrap_or(false)
                    })
                    .unwrap_or(false);
                let cls = format!(
                    "tnode {}{}{}",
                    if current == item.path { "current " } else { "" },
                    label,
                    if item.depth > 0 { " variation" } else { "" },
                );
                let number = item.ply / 2 + 1;
                let prefix = if item.ply % 2 == 0 {
                    format!("{number}. ")
                } else {
                    format!("{number}… ")
                };
                let text = if marked {
                    format!("{prefix}{} •", item.san)
                } else {
                    format!("{prefix}{}", item.san)
                };
                let clock = item.clock.clone();
                let style = if item.depth > 0 {
                    Some(format!("margin-left:{}px", 10 + item.depth * 14))
                } else {
                    None
                };
                let clicked = item.clone();
                view! {
                    <button class=cls style=style on:click=move |_| select_node(clicked.clone())>
                        {text}
                        {clock.map(|c| view! { <span class="clock">{c}</span> })}
                    </button>
                }
            })
            .collect_view();
        view! {
            <div class="tree">
                <button
                    class=start_cls
                    on:click=move |_| {
                        tree_current.set(String::new());
                        ann_ply.set(0);
                        tree_ply.set(0);
                        if variant_only.get_untracked() {
                            return;
                        }
                        let full = game_detail
                            .get()
                            .map(|d| {
                                let sans: Vec<String> = d
                                    .moves
                                    .split_whitespace()
                                    .map(str::to_owned)
                                    .collect();
                                line_pgn(&d.fen, &sans, d.variant.as_deref().unwrap_or("standard"))
                            })
                            .unwrap_or_default();
                        if !full.is_empty() {
                            selected_pgn.set(full);
                        }
                        let seq = goto_seq.get_untracked() + 1;
                        goto_seq.set(seq);
                        goto_sig.set(Some((seq, 0)));
                    }
                >
                    "Start"
                </button>
                {rows}
            </div>
        }
    };

    let persist_settings = move || {
        let settings = api::Settings {
            engine_path: {
                let v = settings_engine.get_untracked();
                if v.trim().is_empty() { None } else { Some(v) }
            },
            model_url: {
                let v = settings_model.get_untracked();
                if v.trim().is_empty() { None } else { Some(v) }
            },
            depth: settings_depth.get_untracked(),
            theme: settings_theme.get_untracked(),
            pieces: settings_pieces.get_untracked(),
            threads: settings_threads.get_untracked(),
            hash_mb: settings_hash.get_untracked(),
            syzygy_path: {
                let v = settings_syzygy.get_untracked();
                if v.trim().is_empty() { None } else { Some(v) }
            },
            variant_engine_path: {
                let v = settings_variant_engine.get_untracked();
                if v.trim().is_empty() { None } else { Some(v) }
            },
            online_explorer: settings_online.get_untracked(),
            animate_last_move: animate_last_move.get_untracked(),
            sound_enabled: sound_enabled.get_untracked(),
            sound_theme: sound_theme.get_untracked(),
            sound_volume: sound_volume.get_untracked(),
            piece_animation_ms: piece_animation_ms.get_untracked(),
            puzzle_rating: puzzle_rating.get_untracked(),
            profile_name: profile_name.get_untracked(),
            profile_player: {
                let v = profile_player.get_untracked();
                if v.trim().is_empty() { None } else { Some(v) }
            },
            keybinds: serde_json::to_string(&keybinds.get_untracked()).unwrap_or_default(),
            engines: engines.get_untracked(),
        };
        spawn_local(async move {
            match api::save_settings(&settings).await {
                Ok(s) => {
                    settings_status.set("saved".into());
                    puzzle_rating.set(s.puzzle_rating);
                }
                Err(e) => settings_status.set(e),
            }
        });
    };

    let apply_ai_action = move |action: serde_json::Value| {
        match action.get("kind").and_then(|v| v.as_str()) {
            Some("arrow") => {
                let from = action.get("from").and_then(|v| v.as_str()).unwrap_or_default();
                let to = action.get("to").and_then(|v| v.as_str()).unwrap_or_default();
                let color = action
                    .get("color")
                    .and_then(|v| v.as_str())
                    .unwrap_or("green");
                if !from.is_empty() && !to.is_empty() {
                    ai_shapes.update(|list| {
                        list.retain(|shape| {
                            !matches!(shape, brick_chess::Shape::Arrow { from: f, to: t, .. } if f == from && t == to)
                        });
                        list.push(brick_chess::Shape::Arrow {
                            from: from.to_owned(),
                            to: to.to_owned(),
                            color: color.to_owned(),
                        });
                    });
                }
            }
            Some("circle") => {
                let square = action
                    .get("square")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let color = action
                    .get("color")
                    .and_then(|v| v.as_str())
                    .unwrap_or("green");
                if !square.is_empty() {
                    ai_shapes.update(|list| {
                        list.retain(|shape| {
                            !matches!(shape, brick_chess::Shape::Circle { square: s, .. } if s == square)
                        });
                        list.push(brick_chess::Shape::Circle {
                            square: square.to_owned(),
                            color: color.to_owned(),
                        });
                    });
                }
            }
            Some("clear") => ai_shapes.set(Vec::new()),
            Some("move") => {
                if let Some(uci) = action.get("uci").and_then(|v| v.as_str()) {
                    let seq = play_seq.get_untracked() + 1;
                    play_seq.set(seq);
                    play_uci.set(Some((seq, uci.to_owned())));
                }
            }
            _ => {}
        }
    };

    let send_chat = move || {
        let message = chat_text.get_untracked();
        if message.trim().is_empty() {
            return;
        }
        let position = fen.get_untracked();
        chat_messages.update(|list| list.push(("user".into(), message.clone())));
        chat_text.set(String::new());
        spawn_local(async move {
            match api::chat(&position, &message).await {
                Ok(reply) => {
                    chat_messages
                        .update(|list| list.push(("assistant".into(), reply.reply.clone())));
                    chat_tools.set(reply.tools.clone());
                    for action in reply.actions {
                        apply_ai_action(action);
                    }
                }
                Err(error) => {
                    chat_messages.update(|list| list.push(("error".into(), error)));
                }
            }
        });
    };

    let console_start = move |_: leptos::ev::MouseEvent| {
        let path = {
            let selected = console_path.get_untracked();
            if selected.trim().is_empty() {
                engines
                    .get_untracked()
                    .first()
                    .map(|engine| engine.path.clone())
                    .unwrap_or_default()
            } else {
                selected
            }
        };
        console_busy.set(true);
        console_log.update(|log| log.push(format!("> starting {}", if path.is_empty() { "engine" } else { &path })));
        spawn_local(async move {
            match api::console_start((!path.is_empty()).then_some(path.as_str())).await {
                Ok(status) => {
                    console_running.set(true);
                    console_name.set(status.name.clone());
                    console_log.update(|log| log.push(format!("< {}", status.name)));
                }
                Err(error) => console_log.update(|log| log.push(format!("< {error}"))),
            }
            console_busy.set(false);
        });
    };

    let console_stop = move |_: leptos::ev::MouseEvent| {
        console_busy.set(true);
        spawn_local(async move {
            let _ = api::console_stop().await;
            console_running.set(false);
            console_name.set(String::new());
            console_log.update(|log| log.push("< stopped".into()));
            console_busy.set(false);
        });
    };

    let console_send = move || {
        let command = console_cmd.get_untracked();
        if command.trim().is_empty() {
            return;
        }
        console_cmd.set(String::new());
        console_log.update(|log| log.push(format!("> {}", command.trim())));
        console_busy.set(true);
        let wait = if command.trim().starts_with("go") { 8000 } else { 800 };
        spawn_local(async move {
            match api::console_command(command.trim(), wait).await {
                Ok(lines) => {
                    console_log.update(|log| {
                        for line in lines {
                            log.push(line);
                        }
                    });
                }
                Err(error) => console_log.update(|log| log.push(format!("< {error}"))),
            }
            console_busy.set(false);
        });
    };

    let set_engine_option = move |index: usize, key: &'static str, value: String| {
        engines.update(|list| {
            if let Some(entry) = list.get_mut(index) {
                if value.trim().is_empty() {
                    entry.options.remove(key);
                } else {
                    entry.options.insert(key.to_owned(), value);
                }
            }
        });
    };

    let run_engine_test = move |_| {
        let Some(index) = engine_dialog.get_untracked() else {
            return;
        };
        let path = engines
            .get_untracked()
            .get(index)
            .map(|entry| entry.path.clone())
            .unwrap_or_default();
        engine_test_busy.set(true);
        engine_test.set(None);
        spawn_local(async move {
            match api::test_engine(&path).await {
                Ok(info) => engine_test.set(Some(info)),
                Err(error) => engine_test.set(Some(api::EngineInfo {
                    ok: false,
                    name: String::new(),
                    author: String::new(),
                    options: Vec::new(),
                    error: Some(error),
                })),
            }
            engine_test_busy.set(false);
        });
    };

    let add_engine = move |_| {
        let name = new_engine_name.get();
        let path = new_engine_path.get();
        if name.trim().is_empty() || path.trim().is_empty() {
            return;
        }
        let path = path.trim().to_owned();
        engines.update(|v| {
            if !v.iter().any(|e| e.path == path) {
                v.push(api::EngineEntry {
                    name: name.trim().to_owned(),
                    path: path.clone(),
                    options: Default::default(),
                });
            }
        });
        new_engine_name.set(String::new());
        new_engine_path.set(String::new());
        persist_settings();
    };

    let stats_view = move || {
        let card = |label: &'static str, value: i64| {
            view! {
                <div class="stat-card">
                    <div class="stat-label">{label}</div>
                    <div class="stat-value">{value.to_string()}</div>
                </div>
            }
        };
        let s = stats.get();
        view! {
            <section class="panel">
                <div class="panel-head">
                    <h2>"Statistics"</h2>
                    <span class="muted">{move || db_list.get().into_iter().find(|d| d.current).map(|d| d.name).unwrap_or_default()}</span>
                </div>
                <div class="stat-grid">
                    {card("Games", s.games)}
                    {card("White wins", s.white_wins)}
                    {card("Draws", s.draws)}
                    {card("Black wins", s.black_wins)}
                    {card("Unfinished", s.unfinished)}
                    {card("Players", s.players)}
                    {card("Events", s.events)}
                    {card("Collections", s.collections)}
                    {card("Puzzles", s.puzzles)}
                </div>
                <div class="catalog-row">
                    <div class="catalog">
                        <div class="panel-sub">"Most active players"</div>
                        {move || {
                            stats
                                .get()
                                .top_players
                                .into_iter()
                                .map(|p| {
                                    view! {
                                        <span class="muted">
                                            {format!(
                                                "{} — {} games{}",
                                                p.name,
                                                p.games,
                                                p.elo.map(|e| format!(" · {e}")).unwrap_or_default(),
                                            )}
                                        </span>
                                    }
                                })
                                .collect_view()
                        }}
                    </div>
                    <div class="catalog">
                        <div class="panel-sub">"Events"</div>
                        {move || {
                            stats
                                .get()
                                .top_events
                                .into_iter()
                                .map(|e| view! { <span class="muted">{format!("{} — {}", e.name, e.games)}</span> })
                                .collect_view()
                        }}
                    </div>
                </div>
            </section>
        }
        .into_any()
    };

    let download_vision = move |_| {
        vision_busy.set(true);
        vision_msg.set("downloading model (about 100 MB)…".into());
        spawn_local(async move {
            match api::download_vision().await {
                Ok(status) => {
                    vision.set(Some(status));
                    vision_msg.set("vision model installed".into());
                }
                Err(e) => vision_msg.set(e),
            }
            vision_busy.set(false);
        });
    };

    let scan_image = move |ev: leptos::ev::Event| {
        let Some(input) = ev
            .current_target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        else {
            return;
        };
        let Some(file) = input.files().and_then(|files| files.get(0)) else {
            return;
        };
        let Ok(reader) = web_sys::FileReader::new() else {
            return;
        };
        let reader_for_load = reader.clone();
        let onload = Closure::<dyn FnMut()>::new(move || {
            let Ok(result) = reader_for_load.result() else {
                return;
            };
            let Some(url) = result.as_string() else {
                return;
            };
            vision_msg.set("recognizing board…".into());
            spawn_local(async move {
                match api::recognize_image(&url).await {
                    Ok(found) => {
                        let fen = format!("{} {} - - 0 1", found.placement, found.turn);
                        editor_pieces.set(pieces_from_fen(&fen));
                        editor_turn.set(found.turn.clone());
                        editor_castling.set("-".into());
                        editor_ep.set("-".into());
                        editor_variant.set(board_variant.get_untracked());
                        editor_open.set(true);
                        vision_msg.set(format!(
                            "{} pieces detected · confidence {:.0}%",
                            found.detections,
                            found.confidence * 100.0
                        ));
                    }
                    Err(e) => vision_msg.set(e),
                }
            });
        });
        reader.set_onload(Some(onload.as_ref().unchecked_ref()));
        let _ = reader.read_as_data_url(&file);
        onload.forget();
    };

    let upload_sound = move |ev: leptos::ev::Event| {
        let Some(input) = ev
            .current_target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        else {
            return;
        };
        let Some(file) = input.files().and_then(|files| files.get(0)) else {
            return;
        };
        let name = file.name();
        let stem = name.split('.').next().unwrap_or("unknown").to_owned();
        let Ok(reader) = web_sys::FileReader::new() else {
            return;
        };
        let reader_for_load = reader.clone();
        let onload = Closure::<dyn FnMut()>::new(move || {
            let Ok(result) = reader_for_load.result() else {
                return;
            };
            let Some(url) = result.as_string() else {
                return;
            };
            let stem = stem.clone();
            spawn_local(async move {
                match api::upload_sound(&stem, &url).await {
                    Ok(()) => {
                        sound_msg.set(format!("uploaded {stem}"));
                        sound_theme.set("custom".into());
                    }
                    Err(e) => sound_msg.set(e),
                }
            });
        });
        reader.set_onload(Some(onload.as_ref().unchecked_ref()));
        let _ = reader.read_as_data_url(&file);
        onload.forget();
    };

    let settings_view = move || {
        view! {
            <section class="panel">
                <div class="panel-head">
                    <h2>"Settings"</h2>
                    <span class="muted">{move || format!("Puzzle rating {}", puzzle_rating.get())}</span>
                </div>
                <div class="settings-form">
                    <label>
                        "Engine binary (STOCKFISH_PATH)"
                        <input
                            type="text"
                            placeholder="/path/to/stockfish"
                            prop:value=move || settings_engine.get()
                            on:input=move |ev| settings_engine.set(event_target_value(&ev))
                        />
                    </label>
                    <label>
                        "Model URL (chat)"
                        <input
                            type="text"
                            placeholder="http://127.0.0.1:8000/v1/chat"
                            prop:value=move || settings_model.get()
                            on:input=move |ev| settings_model.set(event_target_value(&ev))
                        />
                    </label>
                    <label>
                        "Analysis depth"
                        <input
                            type="number"
                            min="1"
                            max="30"
                            prop:value=move || settings_depth.get().to_string()
                            on:input=move |ev| {
                                settings_depth.set(event_target_value(&ev).parse().unwrap_or(15))
                            }
                        />
                    </label>
                    <div class="settings-row">
                        <label>
                            "Board theme"
                            <select
                                prop:value=move || settings_theme.get()
                                on:change=move |ev| settings_theme.set(event_target_value(&ev))
                            >
                                <option value="green">"Green"</option>
                                <option value="brown">"Brown"</option>
                                <option value="blue">"Blue"</option>
                            </select>
                        </label>
                        <label>
                            "Piece set"
                            <select
                                prop:value=move || settings_pieces.get()
                                on:change=move |ev| settings_pieces.set(event_target_value(&ev))
                            >
                                {brick_chess::pieces::PieceSet::ALL
                                    .into_iter()
                                    .map(|s| view! { <option value=s.slug()>{s.label()}</option> })
                                    .collect_view()}
                            </select>
                        </label>
                    </div>
                    <div class="settings-row">
                        <label>
                            "Engine threads (0 = default)"
                            <input
                                type="number"
                                min="0"
                                max="256"
                                prop:value=move || settings_threads.get().to_string()
                                on:input=move |ev| {
                                    settings_threads.set(event_target_value(&ev).parse().unwrap_or(0))
                                }
                            />
                        </label>
                        <label>
                            "Hash MB (0 = default)"
                            <input
                                type="number"
                                min="0"
                                max="65536"
                                prop:value=move || settings_hash.get().to_string()
                                on:input=move |ev| {
                                    settings_hash.set(event_target_value(&ev).parse().unwrap_or(0))
                                }
                            />
                        </label>
                    </div>
                    <label>
                        "Syzygy tablebase path"
                        <input
                            type="text"
                            placeholder="/path/to/syzygy"
                            prop:value=move || settings_syzygy.get()
                            on:input=move |ev| settings_syzygy.set(event_target_value(&ev))
                        />
                    </label>
                    {move || {
                        let index = engine_dialog.get()?;
                        let entry = engines.get().get(index).cloned()?;
                        let option = |key: &'static str, label: &'static str, numeric: bool| {
                            let stored = entry.options.get(key).cloned().unwrap_or_default();
                            let value = if stored.is_empty() {
                                match key {
                                    "MultiPV" => "1".to_owned(),
                                    "Hash" | "Threads" => "0".to_owned(),
                                    _ => String::new(),
                                }
                            } else {
                                stored
                            };
                            view! {
                                <label>
                                    {label}
                                    <input
                                        type=if numeric { "number" } else { "text" }
                                        prop:value=value
                                        on:change=move |ev| {
                                            set_engine_option(index, key, event_target_value(&ev))
                                        }
                                    />
                                </label>
                            }
                        };
                        let test = engine_test.get();
                        Some(view! {
                            <div class="engine-dialog">
                                <div class="panel-head">
                                    <h2>{format!("Engine: {}", entry.name)}</h2>
                                    <button on:click=move |_| {
                                        engine_dialog.set(None);
                                        engine_test.set(None);
                                    }>"Close"</button>
                                </div>
                                <div class="engine-fields">
                                    <label>
                                        "Name"
                                        <input
                                            type="text"
                                            prop:value=entry.name.clone()
                                            on:change=move |ev| {
                                                let value = event_target_value(&ev);
                                                engines.update(|list| {
                                                    if let Some(item) = list.get_mut(index) {
                                                        item.name = value;
                                                    }
                                                });
                                            }
                                        />
                                    </label>
                                    <label>
                                        "Path"
                                        <input
                                            type="text"
                                            prop:value=entry.path.clone()
                                            on:change=move |ev| {
                                                let value = event_target_value(&ev);
                                                engines.update(|list| {
                                                    if let Some(item) = list.get_mut(index) {
                                                        item.path = value;
                                                    }
                                                });
                                            }
                                        />
                                    </label>
                                </div>
                                <div class="engine-fields">
                                    {option("Hash", "Hash (MB)", true)}
                                    {option("Threads", "Threads", true)}
                                    {option("MultiPV", "MultiPV", true)}
                                    {option("Skill Level", "Skill level", true)}
                                    {option("UCI_Elo", "UCI Elo", true)}
                                    {option("SyzygyPath", "Syzygy path", false)}
                                </div>
                                <div class="muted">
                                    "Empty values use the engine defaults; these override the global Hash/Threads for this engine."
                                </div>
                                <div class="row-btns">
                                    <button on:click=run_engine_test disabled=move || engine_test_busy.get()>
                                        {move || if engine_test_busy.get() { "Testing…" } else { "Test engine" }}
                                    </button>
                                    <button on:click=move |_| {
                                        persist_settings();
                                        settings_status.set("engine saved".into());
                                    }>"Save"</button>
                                    <span class="status">{move || settings_status.get()}</span>
                                </div>
                                {test.map(|info| view! {
                                    <div class="engine-test">
                                        <div class=if info.ok { "best" } else { "error" }>
                                            {if info.ok {
                                                format!("{} — {}", info.name, info.author)
                                            } else {
                                                format!("could not start engine: {}", info.error.unwrap_or_default())
                                            }}
                                        </div>
                                        <div class="muted">
                                            {format!("{} UCI option(s)", info.options.len())}
                                        </div>
                                    </div>
                                })}
                            </div>
                        })
                    }}
                    <label>
                        "Variant engine (Fairy-Stockfish)"
                        <input
                            type="text"
                            placeholder="/path/to/fairy-stockfish"
                            prop:value=move || settings_variant_engine.get()
                            on:input=move |ev| settings_variant_engine.set(event_target_value(&ev))
                        />
                    </label>
                    <div class="panel-sub">"Vision model (optional)"</div>
                    {move || {
                        vision
                            .get()
                            .map(|v| {
                                let size = v.size as f64 / 1_048_576.0;
                                view! {
                                    <div class="engine-body">
                                        <div class="muted">
                                            {format!("{} · {} · {:.0} MB", v.name, v.license, size)}
                                        </div>
                                        <div class="muted">{v.sha256.clone()}</div>
                                        <div class="row-btns">
                                            <button
                                                on:click=download_vision
                                                disabled=move || vision_busy.get()
                                            >
                                                {move || {
                                                    if vision.get().map(|v| v.installed).unwrap_or(false) {
                                                        "Re-download model"
                                                    } else {
                                                        "Download model"
                                                    }
                                                }}
                                            </button>
                                            <span class="status">
                                                {move || {
                                                    if vision.get().map(|v| v.installed).unwrap_or(false) {
                                                        "installed".to_owned()
                                                    } else {
                                                        "not installed".to_owned()
                                                    }
                                                }}
                                            </span>
                                        </div>
                                        <div class="muted">{move || vision_msg.get()}</div>
                                    </div>
                                }
                            })
                    }}
                    <div class="panel-sub">"Engines"</div>
                    <div class="engine-list">
                        {move || {
                            engines
                                .get()
                                .into_iter()
                                .enumerate()
                                .map(|(i, e)| {
                                    let active = settings_engine.get() == e.path;
                                    let path = e.path.clone();
                                    view! {
                                        <div class="engine-row">
                                            <span class=if active { "engine-name active" } else { "engine-name" }>
                                                {format!("{} — {}", e.name, e.path)}
                                            </span>
                                            <button
                                                on:click=move |_| {
                                                    engine_test.set(None);
                                                    engine_dialog.set(Some(i));
                                                }
                                            >
                                                "Configure"
                                            </button>
                                            <button
                                                on:click=move |_| {
                                                    settings_engine.set(path.clone());
                                                    persist_settings();
                                                }
                                            >
                                                {if active { "Active" } else { "Use" }}
                                            </button>
                                            <button
                                                on:click=move |_| {
                                                    engines.update(|v| {
                                                        if i < v.len() {
                                                            v.remove(i);
                                                        }
                                                    });
                                                    persist_settings();
                                                }
                                            >
                                                "Remove"
                                            </button>
                                        </div>
                                    }
                                })
                                .collect_view()
                        }}
                    </div>
                    <div class="row-btns">
                        <input
                            type="text"
                            placeholder="Engine name"
                            style="max-width:9rem"
                            prop:value=move || new_engine_name.get()
                            on:input=move |ev| new_engine_name.set(event_target_value(&ev))
                        />
                        <input
                            type="text"
                            placeholder="/path/to/engine"
                            prop:value=move || new_engine_path.get()
                            on:input=move |ev| new_engine_path.set(event_target_value(&ev))
                        />
                        <button on:click=add_engine>"Add engine"</button>
                    </div>
                    <label class="checkline">
                        <input
                            type="checkbox"
                            prop:checked=move || animate_last_move.get()
                            on:change=move |ev| animate_last_move.set(event_target_checked(&ev))
                        />
                        "Animate the last move"
                    </label>
                    <div class="settings-row">
                        <label class="checkline">
                            <input
                                type="checkbox"
                                prop:checked=move || sound_enabled.get()
                                on:change=move |ev| sound_enabled.set(event_target_checked(&ev))
                            />
                            "Sounds"
                        </label>
                        <label>
                            "Sound theme"
                            <select
                                prop:value=move || sound_theme.get()
                                on:change=move |ev| sound_theme.set(event_target_value(&ev))
                            >
                                <option value="sfx">"SFX"</option>
                                <option value="piano">"Piano"</option>
                                <option value="nes">"NES"</option>
                                <option value="futuristic">"Futuristic"</option>
                                <option value="custom">"Custom"</option>
                            </select>
                        </label>
                        <label>
                            "Volume"
                            <input
                                type="range"
                                min="0"
                                max="100"
                                prop:value=move || sound_volume.get().to_string()
                                on:input=move |ev| sound_volume.set(event_target_value(&ev).parse().unwrap_or(70))
                            />
                        </label>
                        <label>
                            "Move animation (0 = instant)"
                            <input
                                type="range"
                                min="0"
                                max="400"
                                step="10"
                                prop:value=move || piece_animation_ms.get().to_string()
                                on:input=move |ev| piece_animation_ms.set(event_target_value(&ev).parse().unwrap_or(0))
                            />
                        </label>
                    </div>
                    <label class="checkline">
                        "Custom sound file (named Move.ogg, Capture.ogg, …)"
                        <input
                            type="file"
                            accept=".ogg,.mp3,.wav,audio/*"
                            on:change=upload_sound
                        />
                    </label>
                    <div class="muted">{move || sound_msg.get()}</div>
                    <div class="panel-sub">"Keybinds"</div>
                    <div class="keybinds">
                        <label>
                            "Previous move"
                            <input
                                type="text"
                                prop:value=move || keybinds.get().previous
                                on:change=move |ev| {
                                    let v = event_target_value(&ev);
                                    keybinds.update(|k| k.previous = v);
                                }
                            />
                        </label>
                        <label>
                            "Next move"
                            <input
                                type="text"
                                prop:value=move || keybinds.get().next
                                on:change=move |ev| {
                                    let v = event_target_value(&ev);
                                    keybinds.update(|k| k.next = v);
                                }
                            />
                        </label>
                        <label>
                            "Start"
                            <input
                                type="text"
                                prop:value=move || keybinds.get().start
                                on:change=move |ev| {
                                    let v = event_target_value(&ev);
                                    keybinds.update(|k| k.start = v);
                                }
                            />
                        </label>
                        <label>
                            "End"
                            <input
                                type="text"
                                prop:value=move || keybinds.get().end
                                on:change=move |ev| {
                                    let v = event_target_value(&ev);
                                    keybinds.update(|k| k.end = v);
                                }
                            />
                        </label>
                        <label>
                            "Flip board"
                            <input
                                type="text"
                                prop:value=move || keybinds.get().flip
                                on:change=move |ev| {
                                    let v = event_target_value(&ev);
                                    keybinds.update(|k| k.flip = v);
                                }
                            />
                        </label>
                    </div>
                    <div class="muted">
                        "Key names follow the browser: ArrowLeft, Home, End, or a single letter."
                    </div>
                    <div class="row-btns">
                        <button on:click=move |_| persist_settings()>"Save"</button>
                        <button on:click=move |_| {
                            keybinds.set(Keybinds::default());
                        }>"Reset keybinds"</button>
                        <span class="status">{move || settings_status.get()}</span>
                    </div>
                </div>
            </section>
        }
        .into_any()
    };

    let database_view = move || {
        view! {
            <section class="panel">
                <div class="panel-head">
                    <h2>"Database"</h2>
                    <span class="muted">{move || format!("{} games", total.get())}</span>
                </div>
                <div class="dbs">
                    <label>
                        "Database"
                        <select
                            prop:value=move || {
                                db_list
                                    .get()
                                    .into_iter()
                                    .find(|d| d.current)
                                    .map(|d| d.name)
                                    .unwrap_or_default()
                            }
                            on:change=switch_db
                        >
                            {move || {
                                db_list
                                    .get()
                                    .into_iter()
                                    .map(|d| {
                                        let name = d.name;
                                        view! { <option value=name.clone()>{name.clone()}</option> }
                                    })
                                    .collect_view()
                            }}
                        </select>
                    </label>
                    <input
                        type="text"
                        placeholder="New database…"
                        prop:value=move || new_database.get()
                        on:input=move |ev| new_database.set(event_target_value(&ev))
                    />
                    <button on:click=create_db>"Create"</button>
                </div>
                <div class="import">
                    <textarea
                        placeholder="Paste one or more PGN games…"
                        prop:value=move || import_text.get()
                        on:input=move |ev| import_text.set(event_target_value(&ev))
                    ></textarea>
                    <button on:click=import>"Import"</button>
                </div>
                <div class="collections">
                    <button
                        class=move || if active_collection.get().is_none() { "chip active" } else { "chip" }
                        on:click=move |_| {
                            active_collection.set(None);
                            reload();
                        }
                    >
                        "All games"
                    </button>
                    {move || {
                        collections
                            .get()
                            .into_iter()
                            .map(|c| {
                                let id = c.id;
                                view! {
                                    <button
                                        class=move || {
                                            if active_collection.get() == Some(id) { "chip active" } else { "chip" }
                                        }
                                        on:click=move |_| {
                                            active_collection.set(Some(id));
                                            reload();
                                        }
                                    >
                                        {format!("{} ({})", c.name, c.games)}
                                    </button>
                                }
                            })
                            .collect_view()
                    }}
                    <input
                        type="text"
                        placeholder="New collection…"
                        prop:value=move || new_collection.get()
                        on:input=move |ev| new_collection.set(event_target_value(&ev))
                    />
                    <button on:click=add_collection_clicked>"Create"</button>
                </div>
                <div class="dbs">
                    <select
                        prop:value=move || manage_collection.get().to_string()
                        on:change=move |ev| {
                            manage_collection.set(event_target_value(&ev).parse().unwrap_or(0))
                        }
                    >
                        <option value="0">"Manage collection…"</option>
                        {move || {
                            collections
                                .get()
                                .into_iter()
                                .map(|c| {
                                    view! { <option value=c.id.to_string()>{c.name.clone()}</option> }
                                })
                                .collect_view()
                        }}
                    </select>
                    <input
                        type="text"
                        placeholder="New name…"
                        prop:value=move || collection_name.get()
                        on:input=move |ev| collection_name.set(event_target_value(&ev))
                    />
                    <button on:click=rename_collection>"Rename"</button>
                    <button on:click=delete_collection>"Delete"</button>
                </div>
                <div class="load">
                    <input
                        type="text"
                        placeholder="Search by position (FEN)…"
                        prop:value=move || position_fen.get()
                        on:input=move |ev| position_fen.set(event_target_value(&ev))
                    />
                    <select
                        prop:value=move || position_variant.get()
                        on:change=move |ev| position_variant.set(event_target_value(&ev))
                    >
                        <option value="standard">"Standard"</option>
                        <option value="chess960">"Chess960"</option>
                        <option value="atomic">"Atomic"</option>
                        <option value="antichess">"Antichess"</option>
                        <option value="kingofthehill">"King of the Hill"</option>
                        <option value="3check">"Three-check"</option>
                        <option value="crazyhouse">"Crazyhouse"</option>
                        <option value="racingkings">"Racing Kings"</option>
                        <option value="horde">"Horde"</option>
                    </select>
                    <label class="checkline">
                        <input
                            type="checkbox"
                            prop:checked=move || partial_search.get()
                            on:change=move |ev| partial_search.set(event_target_checked(&ev))
                        />
                        "partial (pieces only)"
                    </label>
                    <button on:click=search_position>"Find position"</button>
                    <button on:click=move |_| {
                        position_fen.set(String::new());
                        event_filter.set(None);
                        query.set(String::new());
                        filter_date_from.set(String::new());
                        filter_date_to.set(String::new());
                        filter_result.set(String::new());
                        filter_elo_min.set(String::new());
                        filter_elo_max.set(String::new());
                        filter_variant.set("any".into());
                        sort_by.set("newest".into());
                        reload();
                    }>"Clear filters"</button>
                </div>
                <div class="filters">
                    <label>
                        "From"
                        <input
                            type="date"
                            prop:value=move || filter_date_from.get()
                            on:change=move |ev| filter_date_from.set(event_target_value(&ev))
                        />
                    </label>
                    <label>
                        "To"
                        <input
                            type="date"
                            prop:value=move || filter_date_to.get()
                            on:change=move |ev| filter_date_to.set(event_target_value(&ev))
                        />
                    </label>
                    <label>
                        "Result"
                        <select
                            prop:value=move || filter_result.get()
                            on:change=move |ev| filter_result.set(event_target_value(&ev))
                        >
                            <option value="">"Any"</option>
                            <option value="1-0">"White wins"</option>
                            <option value="0-1">"Black wins"</option>
                            <option value="1/2-1/2">"Draw"</option>
                            <option value="*">"Unfinished"</option>
                        </select>
                    </label>
                    <label>
                        "Elo"
                        <input
                            type="number"
                            placeholder="min"
                            min="0"
                            max="4000"
                            prop:value=move || filter_elo_min.get()
                            on:input=move |ev| filter_elo_min.set(event_target_value(&ev))
                        />
                        <input
                            type="number"
                            placeholder="max"
                            min="0"
                            max="4000"
                            prop:value=move || filter_elo_max.get()
                            on:input=move |ev| filter_elo_max.set(event_target_value(&ev))
                        />
                    </label>
                    <label>
                        "Variant"
                        <select
                            prop:value=move || filter_variant.get()
                            on:change=move |ev| filter_variant.set(event_target_value(&ev))
                        >
                            <option value="any">"Any"</option>
                            <option value="standard">"Standard"</option>
                            <option value="chess960">"Chess960"</option>
                            <option value="atomic">"Atomic"</option>
                            <option value="antichess">"Antichess"</option>
                            <option value="kingofthehill">"King of the Hill"</option>
                            <option value="3check">"Three-check"</option>
                            <option value="crazyhouse">"Crazyhouse"</option>
                            <option value="racingkings">"Racing Kings"</option>
                            <option value="horde">"Horde"</option>
                        </select>
                    </label>
                    <label>
                        "Sort"
                        <select
                            prop:value=move || sort_by.get()
                            on:change=move |ev| sort_by.set(event_target_value(&ev))
                        >
                            <option value="newest">"Newest"</option>
                            <option value="oldest">"Oldest"</option>
                            <option value="date">"Game date"</option>
                            <option value="white">"White"</option>
                            <option value="black">"Black"</option>
                            <option value="length">"Longest"</option>
                        </select>
                    </label>
                    <button on:click=move |_| reload()>"Apply filters"</button>
                </div>
                {move || {
                    event_filter
                        .get()
                        .map(|e| {
                            view! {
                                <div class="muted">
                                    {format!("Event: {e}")}
                                    <button
                                        class="link"
                                        on:click=move |_| {
                                            event_filter.set(None);
                                            reload();
                                        }
                                    >
                                        "clear"
                                    </button>
                                </div>
                            }
                        })
                }}
                <div class="dbs">
                    <select
                        prop:value=move || online_provider.get()
                        on:change=move |ev| online_provider.set(event_target_value(&ev))
                    >
                        <option value="lichess">"Lichess"</option>
                        <option value="chesscom">"Chess.com"</option>
                    </select>
                    <input
                        type="text"
                        placeholder="Username"
                        prop:value=move || online_user.get()
                        on:input=move |ev| online_user.set(event_target_value(&ev))
                    />
                    <input
                        type="number"
                        min="1"
                        max="200"
                        style="max-width:6rem"
                        prop:value=move || online_max.get().to_string()
                        on:input=move |ev| online_max.set(event_target_value(&ev).parse().unwrap_or(30))
                    />
                    <button on:click=import_online>"Import online"</button>
                </div>
                <div class="table">
                    <div class="row head">
                        <span>"#"</span><span>"White"</span><span>"Black"</span>
                        <span>"Result"</span><span>"Event"</span>
                    </div>
                    {move || {
                        games
                            .get()
                            .into_iter()
                            .map(|g| {
                                let clicked = g.clone();
                                view! {
                                    <button class="row game" data-game-id=g.id.to_string() on:click=move |_| open_game(clicked.clone())>
                                        <span>{g.id.to_string()}</span>
                                        <span>{g.white.clone()}</span>
                                        <span>{g.black.clone()}</span>
                                        <span>{g.result.clone()}</span>
                                        <span>{g.event.clone().unwrap_or_default()}</span>
                                    </button>
                                }
                            })
                            .collect_view()
                    }}
                </div>
                <div class="catalog-row">
                    <div class="catalog">
                        <div class="panel-sub">"Players"</div>
                        {move || {
                            players
                                .get()
                                .into_iter()
                                .take(8)
                                .map(|p| {
                                    let name = p.name.clone();
                                    view! {
                                        <button
                                            class="link"
                                            on:click=move |_| {
                                                query.set(name.clone());
                                                reload();
                                            }
                                        >
                                            {format!("{} ({})", p.name, p.games)}
                                        </button>
                                    }
                                })
                                .collect_view()
                        }}
                    </div>
                    <div class="catalog">
                        <div class="panel-sub">"Events"</div>
                        {move || {
                            events
                                .get()
                                .into_iter()
                                .take(8)
                                .map(|e| {
                                    let name = e.name.clone();
                                    let games = e.games;
                                    let clicked = name.clone();
                                    view! {
                                        <button
                                            class="link"
                                            on:click=move |_| {
                                                event_filter.set(Some(clicked.clone()));
                                                reload();
                                            }
                                        >
                                            {format!("{name} ({games})")}
                                        </button>
                                    }
                                })
                                .collect_view()
                        }}
                    </div>
                </div>
            </section>
        }
        .into_any()
    };

    let ask_engine = move |position: String| {
        thinking.set(true);
        let depth = settings_depth.get_untracked();
        spawn_local(async move {
            match api::analyze(&position, depth, &board_variant.get_untracked()).await {
                Ok(a) => {
                    let seq = play_seq.get_untracked() + 1;
                    play_seq.set(seq);
                    play_uci.set(Some((seq, a.best_move)));
                    thinking.set(false);
                }
                Err(e) => {
                    status.set(e);
                    thinking.set(false);
                }
            }
        });
    };

    let record_played_move = move |san: &str, now_black: bool| {
        let Some(game) = selected_game_id.get_untracked() else {
            return;
        };
        if !record_moves.get_untracked() {
            return;
        }
        let clock_text = clock.get_untracked().map(|(white, black)| {
            let mover_clock = if now_black { white } else { black };
            clock_pgn(mover_clock)
        });
        let parent = tree_current.get_untracked();
        let san = san.to_owned();
        spawn_local(async move {
            match api::add_tree_move(game, &parent, &san, clock_text.as_deref()).await {
                Ok((detail, path)) => {
                    let sans: Vec<String> =
                        detail.moves.split_whitespace().map(str::to_owned).collect();
                    selected_moves.set(sans);
                    selected_title.set(format!(
                        "{} vs {} · {}",
                        detail.white, detail.black, detail.result
                    ));
                    game_detail.set(Some(detail));
                    tree_current.set(path);
                }
                Err(e) => status.set(e),
            }
        });
    };

    let on_board_move = move |(_uci, san, fen_after): (String, String, String)| {
        if clock.get_untracked().is_some() {
            let inc = clock_inc.get_untracked();
            let now_black = fen_after.split_whitespace().nth(1) == Some("b");
            clock.update(|c| {
                if let Some((white, black)) = c {
                    if inc > 0 {
                        if now_black {
                            *white += inc;
                        } else {
                            *black += inc;
                        }
                    }
                }
            });
            if !clock_active.get_untracked() {
                clock_epoch.update(|e| *e += 1);
                clock_active.set(true);
            }
        }
        record_played_move(&san, fen_after.split_whitespace().nth(1) == Some("b"));
        if !vs_engine.get_untracked() {
            return;
        }
        let engine_black = play_as.get_untracked() == "white";
        let turn_black = fen_after.split_whitespace().nth(1) == Some("b");
        if turn_black == engine_black {
            ask_engine(fen_after);
        }
    };

    let on_engine_play = move |(_uci, san, fen_after): (String, String, String)| {
        let inc = clock_inc.get_untracked();
        record_played_move(&san, fen_after.split_whitespace().nth(1) == Some("b"));
        if inc > 0 && clock.get_untracked().is_some() {
            let now_black = fen_after.split_whitespace().nth(1) == Some("b");
            clock.update(|c| {
                if let Some((white, black)) = c {
                    if now_black {
                        *white += inc;
                    } else {
                        *black += inc;
                    }
                }
            });
        }
    };

    Effect::new(move |_| {
        let on = clock_active.get();
        let epoch = clock_epoch.get();
        if !on || clock.get_untracked().is_none() {
            return;
        }
        spawn_local(async move {
            loop {
                TimeoutFuture::new(1000).await;
                if clock_epoch.get_untracked() != epoch || !clock_active.get_untracked() {
                    break;
                }
                let position = fen.get_untracked();
                let black = position.split_whitespace().nth(1) == Some("b");
                let mut flagged = false;
                clock.update(|c| {
                    if let Some((white, black_clock)) = c {
                        if black {
                            *black_clock = (*black_clock - 1).max(0);
                            flagged = *black_clock == 0;
                        } else {
                            *white = (*white - 1).max(0);
                            flagged = *white == 0;
                        }
                    }
                });
                if flagged {
                    status.set(if black {
                        "Black flag fell".to_owned()
                    } else {
                        "White flag fell".to_owned()
                    });
                    clock_active.set(false);
                    break;
                }
            }
        });
    });

    let toggle_vs_engine = move |ev: leptos::ev::Event| {
        let on = event_target_checked(&ev);
        vs_engine.set(on);
        if on {
            let engine_black = play_as.get_untracked() == "white";
            let position = fen.get_untracked();
            let turn_black = position.split_whitespace().nth(1) == Some("b");
            if turn_black == engine_black {
                ask_engine(position);
            }
        } else {
            thinking.set(false);
        }
    };

    let choose_side = move |ev: leptos::ev::Event| {
        let side = event_target_value(&ev);
        play_as.set(side.clone());
        if vs_engine.get_untracked() {
            let engine_black = side == "white";
            let position = fen.get_untracked();
            let turn_black = position.split_whitespace().nth(1) == Some("b");
            if turn_black == engine_black {
                ask_engine(position);
            }
        }
    };

    let open_editor = move |_| {
        let position = fen.get_untracked();
        let fields: Vec<&str> = position.split_whitespace().collect();
        editor_pieces.set(pieces_from_fen(&position));
        editor_turn.set(fields.get(1).copied().unwrap_or("w").to_owned());
        editor_castling.set(fields.get(2).copied().unwrap_or("KQkq").to_owned());
        editor_ep.set(fields.get(3).copied().unwrap_or("-").to_owned());
        editor_variant.set(board_variant.get_untracked());
        editor_open.set(true);
    };

    let apply_editor = move |_| {
        let position = fen_from_editor(
            &editor_pieces.get_untracked(),
            &editor_turn.get_untracked(),
            &editor_castling.get_untracked(),
            &editor_ep.get_untracked(),
        );
        let seq = load_seq.get_untracked() + 1;
        load_seq.set(seq);
        load_fen_sig.set(Some((seq, position, editor_variant.get_untracked())));
        vs_engine.set(false);
        editor_open.set(false);
    };

    let probe_tablebase = move |_| {
        let position = fen.get_untracked();
        if position.is_empty() {
            return;
        }
        tb_status.set("probing…".into());
        tb_result.set(None);
        spawn_local(async move {
            match api::tablebase(&position, &board_variant.get_untracked()).await {
                Ok(t) => {
                    tb_status.set(String::new());
                    tb_result.set(Some(t));
                }
                Err(e) => tb_status.set(e),
            }
        });
    };

    let home_view = move || {
        let stat = move |value: i64, label: &'static str| {
            view! {
                <div class="home-stat">
                    <div class="home-stat-value">{value}</div>
                    <div class="muted">{label}</div>
                </div>
            }
        };
        view! {
            <section class="home">
                <div class="panel home-hero">
                    <div>
                        <div class="panel-sub">"Local account"</div>
                        <h2 class="home-greeting">
                            {move || format!("Welcome back, {}", profile_name.get())}
                        </h2>
                        <div class="muted">
                            "Everything stays in this database. A player profile reads your games; nothing is uploaded."
                        </div>
                    </div>
                    <div class="home-hero-actions">
                        <input
                            type="text"
                            placeholder="Display name"
                            prop:value=move || profile_name.get()
                            on:input=move |ev| profile_name.set(event_target_value(&ev))
                        />
                        <button on:click=move |_| {
                            persist_settings();
                            status.set("profile saved".into());
                        }>"Save profile"</button>
                    </div>
                </div>

                <div class="home-stats">
                    {move || stat(stats.get().games, "games")}
                    {move || stat(stats.get().players, "players")}
                    {move || stat(stats.get().events, "events")}
                    {move || stat(stats.get().puzzles, "puzzles")}
                </div>

                <div class="home-grid">
                    <div class="panel">
                        <div class="panel-head"><h2>"Recent games"</h2></div>
                        <div class="table">
                            <div class="row head">
                                <span>"ID"</span><span>"White"</span><span>"Black"</span>
                                <span>"Result"</span><span>"Event"</span>
                            </div>
                            {move || {
                                home_recent
                                    .get()
                                    .into_iter()
                                    .map(|g| {
                                        let summary = g.clone();
                                        let title = format!("{} vs {} · {}", g.white, g.black, g.result);
                                        view! {
                                            <button
                                                class="row game"
                                                data-game-id=g.id.to_string()
                                                on:click=move |_| {
                                                    open_game(summary.clone());
                                                    tab.set(Tab::Analysis);
                                                }
                                            >
                                                <span>{g.id.to_string()}</span>
                                                <span>{g.white}</span>
                                                <span>{g.black}</span>
                                                <span>{g.result}</span>
                                                <span class="muted">{g.event.unwrap_or_default()}</span>
                                                <span class="hidden-title">{title}</span>
                                            </button>
                                        }
                                    })
                                    .collect_view()
                            }}
                        </div>
                        <div class="muted">
                            {move || {
                                if home_recent.get().is_empty() {
                                    "No games yet — import a PGN from the Database tab."
                                } else {
                                    ""
                                }
                            }}
                        </div>
                    </div>

                    <div class="panel">
                        <div class="panel-head"><h2>"Player profile"</h2></div>
                        <select
                            prop:value=move || profile_player.get()
                            on:change=move |ev| profile_player.set(event_target_value(&ev))
                        >
                            <option value="">"Choose a player…"</option>
                            {move || {
                                players
                                    .get()
                                    .into_iter()
                                    .map(|p| {
                                        view! {
                                            <option value=p.name.clone()>
                                                {format!("{} ({} games)", p.name, p.games)}
                                            </option>
                                        }
                                    })
                                    .collect_view()
                            }}
                        </select>
                        {move || {
                            profile
                                .get()
                                .map(|p| {
                                    let decided = p.wins + p.draws + p.losses;
                                    view! {
                                        <div class="profile-card">
                                            <div class="profile-name">{p.name.clone()}</div>
                                            <div class="profile-record">
                                                {format!("{}W · {}D · {}L", p.wins, p.draws, p.losses)}
                                            </div>
                                            <div class="profile-score">
                                                {format!("{:.1}% score · {} games", p.score_pct, decided)}
                                            </div>
                                            <div class="muted">
                                                {format!("{} as White · {} as Black · {} unfinished", p.as_white, p.as_black, p.unfinished)}
                                            </div>
                                        </div>
                                    }
                                })
                        }}
                        <div class="row-btns">
                            <button on:click=move |_| {
                                persist_settings();
                                profile_player.set(profile_player.get_untracked());
                                status.set("player profile saved".into());
                            }>"Use as my account"</button>
                        </div>
                        <div class="muted">
                            {move || {
                                if profile_player.get().trim().is_empty() {
                                    "Pick a player to see the record and save it as your account."
                                } else {
                                    ""
                                }
                            }}
                        </div>
                    </div>
                </div>

                <div class="panel">
                    <div class="panel-head"><h2>"Quick actions"</h2></div>
                    <div class="row-btns">
                        <button on:click=move |_| switch_tab(Tab::Database)>"Import / browse games"</button>
                        <button on:click=move |_| new_workspace()>"New analysis board"</button>
                        <button on:click=move |_| switch_tab(Tab::Puzzles)>"Solve a puzzle"</button>
                        <button on:click=move |_| switch_tab(Tab::Openings)>"Train a repertoire"</button>
                        <button on:click=move |_| switch_tab(Tab::Settings)>"Settings"</button>
                    </div>
                </div>
            </section>
        }
    };

    let analysis_view = move || {
        let score = move || {
            analysis
                .get()
                .map(|a| {
                    if let Some(m) = a.mate {
                        format!("M{m}")
                    } else if let Some(c) = a.score_cp {
                        format!("{:+.2}", c as f64 / 100.0)
                    } else {
                        "—".to_owned()
                    }
                })
                .unwrap_or_else(|| "—".to_owned())
        };
        view! {
            <section class="analysis">
                <div class="board-pane">
                    {move || {
                        clock
                            .get()
                            .map(|(white, black)| {
                                let black_turn = fen.get().split_whitespace().nth(1) == Some("b");
                                let running = clock_active.get();
                                let mut white_cls = String::from("clock-chip");
                                let mut black_cls = String::from("clock-chip");
                                if running && !black_turn {
                                    white_cls.push_str(" active");
                                }
                                if running && black_turn {
                                    black_cls.push_str(" active");
                                }
                                if white < 30 {
                                    white_cls.push_str(" low");
                                }
                                if black < 30 {
                                    black_cls.push_str(" low");
                                }
                                view! {
                                    <div class="clocks">
                                        <div class=white_cls>
                                            {format!("White {}", format_clock(white))}
                                        </div>
                                        <div class=black_cls>
                                            {format!("Black {}", format_clock(black))}
                                        </div>
                                    </div>
                                    <div class="clock-controls">
                                        <button
                                            on:click=move |_| {
                                                if clock_active.get_untracked() {
                                                    clock_active.set(false);
                                                } else if clock.get_untracked().is_some() {
                                                    clock_epoch.update(|e| *e += 1);
                                                    clock_active.set(true);
                                                }
                                            }
                                        >
                                            {move || if clock_active.get() { "Pause" } else { "Start" }}
                                        </button>
                                        <button on:click=move |_| {
                                            let base = clock_base_input.get_untracked() as i64;
                                            let inc = clock_inc_input.get_untracked() as i64;
                                            clock.set(Some((base, base)));
                                            clock_inc.set(inc);
                                            clock_active.set(false);
                                            clock_epoch.update(|e| *e += 1);
                                        }>"Reset"</button>
                                        <input
                                            type="number"
                                            min="0"
                                            max="86400"
                                            title="Base seconds"
                                            prop:value=move || clock_base_input.get().to_string()
                                            on:input=move |ev| {
                                                clock_base_input.set(event_target_value(&ev).parse().unwrap_or(180))
                                            }
                                        />
                                        <input
                                            type="number"
                                            min="0"
                                            max="300"
                                            title="Increment seconds"
                                            prop:value=move || clock_inc_input.get().to_string()
                                            on:input=move |ev| {
                                                clock_inc_input.set(event_target_value(&ev).parse().unwrap_or(0))
                                            }
                                        />
                                    </div>
                                }
                            })
                    }}
                    <Board
                        pgn=selected_pgn
                        title=Signal::derive(move || selected_title.get())
                        on_position=Callback::new(move |f: String| fen.set(f))
                        on_ply=Callback::new(move |p: usize| tree_ply.set(p))
                        goto=Signal::derive(move || goto_sig.get())
                        play=Signal::derive(move || play_uci.get())
                        load_fen=Signal::derive(move || load_fen_sig.get())
                        on_move=Callback::new(on_board_move)
                        on_play=Callback::new(on_engine_play)
                        animate_last=Signal::derive(move || animate_last_move.get())
                        animation_ms=Signal::derive(move || piece_animation_ms.get())
                        keybinds=Signal::derive(move || keybinds.get())
                        orientation=Signal::derive(move || {
                            if vs_engine.get() && play_as.get() == "black" {
                                "black".to_string()
                            } else {
                                "white".to_string()
                            }
                        })
                        shapes=Signal::derive(move || {
                            let mut list = shapes.get();
                            list.extend(ai_shapes.get());
                            list
                        })
                        on_shape=Callback::new(move |s: brick_chess::Shape| {
                            shapes.update(|v| v.push(s));
                        })
                        theme=Signal::derive(move || settings_theme.get())
                        pieces=Signal::derive(move || settings_pieces.get())
                        on_theme=Callback::new(move |v: String| {
                            settings_theme.set(v);
                            persist_settings();
                        })
                        on_pieces=Callback::new(move |v: String| {
                            settings_pieces.set(v);
                            persist_settings();
                        })
                    />
                    <div class="eval-graph">
                        {move || {
                            let rows = evals.get();
                            if rows.is_empty() {
                                return view! {
                                    <div class="muted eval-empty">
                                        "Classify the game to build the evaluation graph."
                                    </div>
                                }
                                .into_any();
                            }
                            let n = rows.len().max(2) as f64;
                            let pts: Vec<(f64, f64)> = rows.iter().map(eval_point).collect();
                            let poly = pts
                                .iter()
                                .map(|(ply, y)| {
                                    format!("{:.2},{:.2}", ply * 100.0 / (n - 1.0), y)
                                })
                                .collect::<Vec<_>>()
                                .join(" ");
                            let hover = eval_hover.get().filter(|h| *h < rows.len());
                            let marker = hover.map(|h| {
                                (h as f64 * 100.0 / (n - 1.0), pts[h].1)
                            });
                            let readout = hover.map(|h| {
                                let san = report
                                    .get()
                                    .and_then(|r| {
                                        r.moves.iter().find(|m| m.ply == h).map(|m| m.san.clone())
                                    })
                                    .unwrap_or_default();
                                format!("ply {} {} · {}", h, san, eval_text(&rows[h]))
                            });
                            view! {
                                <div class="eval-readout muted">
                                    {readout.unwrap_or_else(|| "hover the graph; click to jump".into())}
                                </div>
                                <svg
                                    class="eval-svg"
                                    viewBox="0 0 100 100"
                                    preserveAspectRatio="none"
                                    on:mousemove=move |ev| {
                                        let Some(target) = ev.current_target() else { return };
                                        let Ok(el) = target.dyn_into::<web_sys::Element>() else {
                                            return;
                                        };
                                        let width = el.client_width().max(1) as f64;
                                        let x = ev.offset_x() as f64;
                                        let idx = ((x / width) * (n - 1.0))
                                            .round()
                                            .clamp(0.0, n - 1.0) as usize;
                                        eval_hover.set(Some(idx));
                                    }
                                    on:mouseleave=move |_| eval_hover.set(None)
                                    on:click=move |_| {
                                        let Some(ply) = eval_hover.get() else { return };
                                        if let Some(detail) = game_detail.get() {
                                            let items = detail
                                                .movetext
                                                .as_deref()
                                                .map(|json| tree_items(&api::parse_movetext(json)))
                                                .unwrap_or_default();
                                            if let Some(item) = items
                                                .into_iter()
                                                .find(|i| i.depth == 0 && i.ply == ply)
                                            {
                                                select_node(item);
                                            }
                                        }
                                    }
                                >
                                    <line x1="0" y1="50" x2="100" y2="50" stroke="#31313c" stroke-width="0.5"></line>
                                    <polyline points=poly fill="none" stroke="#7fd18a" stroke-width="1.2"></polyline>
                                    {marker
                                        .map(|(x, y)| {
                                            view! {
                                                <line x1=x y1="0" x2=x y2="100" stroke="#9a9aa8" stroke-width="0.4" stroke-dasharray="2,2"></line>
                                                <circle cx=x cy=y r="1.6" fill="#7fd18a"></circle>
                                            }
                                        })}
                                </svg>
                            }
                            .into_any()
                        }}
                    </div>
                </div>
                <aside class="engine-pane">
                    <div class="panel-head"><h2>"Engine"</h2></div>
                    <div class="row-btns">
                        <label class="checkline">
                            <input
                                type="checkbox"
                                prop:checked=move || vs_engine.get()
                                on:change=toggle_vs_engine
                            />
                            "Play vs engine"
                        </label>
                        <select
                            prop:value=move || play_as.get()
                            on:change=choose_side
                        >
                            <option value="white">"You are White"</option>
                            <option value="black">"You are Black"</option>
                        </select>
                        {move || thinking.get().then(|| view! { <span class="muted">"engine thinking…"</span> })}
                    </div>
                    <div class="eval-card">
                        {move || {
                            analysis
                                .get()
                                .map(|a| {
                                    view! {
                                        <div class="eval-score">{score}</div>
                                        <div class="muted">{format!("depth {}", a.depth)}</div>
                                    }
                                    .into_any()
                                })
                                .unwrap_or_else(|| {
                                    view! { <div class="muted">"no analysis yet"</div> }.into_any()
                                })
                        }}
                    </div>
                    <div class="row-btns">
                        <button on:click=run_analysis disabled=move || analyzing.get() || variant_only.get()>
                            {move || if analyzing.get() { "Analyzing…" } else { "Analyze" }}
                        </button>
                        <button on:click=fetch_lines disabled=move || variant_only.get()>"Lines"</button>
                        <button on:click=fetch_engine_lines disabled=move || variant_only.get()>"All engines"</button>
                    </div>
                    {move || {
                        let list = engine_results.get();
                        if list.is_empty() {
                            return None;
                        }
                        Some(view! {
                            <div class="lines">
                                {list
                                    .into_iter()
                                    .map(|engine| {
                                        view! {
                                            <div class="engine-group">
                                                <div class="panel-sub">{engine.name.clone()}</div>
                                                {match engine.error {
                                                    Some(error) => view! {
                                                        <div class="muted">{error}</div>
                                                    }.into_any(),
                                                    None => view! {
                                                        {engine
                                                            .lines
                                                            .into_iter()
                                                            .map(|l| {
                                                                let score = if let Some(m) = l.mate {
                                                                    format!("M{m}")
                                                                } else if let Some(c) = l.score_cp {
                                                                    format!("{:+.2}", c as f64 / 100.0)
                                                                } else {
                                                                    "—".to_owned()
                                                                };
                                                                view! {
                                                                    <div class="line">
                                                                        <span class="line-score">{score}</span>
                                                                        <span class="pv">{l.pv.join(" ")}</span>
                                                                    </div>
                                                                }
                                                            })
                                                            .collect_view()}
                                                    }.into_any(),
                                                }}
                                            </div>
                                        }
                                    })
                                    .collect_view()}
                            </div>
                        })
                    }}
                    {move || {
                        let ls = lines.get();
                        if ls.is_empty() {
                            return None;
                        }
                        Some(view! {
                            <div class="lines">
                                {ls
                                    .into_iter()
                                    .map(|l| {
                                        let score = if let Some(m) = l.mate {
                                            format!("M{m}")
                                        } else if let Some(c) = l.score_cp {
                                            format!("{:+.2}", c as f64 / 100.0)
                                        } else {
                                            "—".to_owned()
                                        };
                                        view! {
                                            <div class="line">
                                                <span class="line-score">{score}</span>
                                                <span class="pv">{l.pv.join(" ")}</span>
                                            </div>
                                        }
                                    })
                                    .collect_view()}
                            </div>
                        })
                    }}
                    {move || {
                        analysis
                            .get()
                            .map(|a| {
                                view! {
                                    <div class="engine-body">
                                        <div class="best">{format!("Best: {}", a.best_move)}</div>
                                        <div class="pv">{a.pv.join(" ")}</div>
                                        <div class="muted">
                                            {a.nodes.map(|n| format!("{n} nodes")).unwrap_or_default()}
                                        </div>
                                    </div>
                                }
                            })
                    }}
                    <div class="panel-sub">"Tablebase"</div>
                    <div class="row-btns">
                        <button on:click=probe_tablebase>"Probe"</button>
                        <span class="muted">{move || tb_status.get()}</span>
                    </div>
                    {move || {
                        tb_result
                            .get()
                            .map(|t| {
                                let wdl = match t.wdl.as_str() {
                                    "win" => "Win",
                                    "cursed-win" => "Cursed win (50-move)",
                                    "draw" => "Draw",
                                    "blessed-loss" => "Blessed loss (50-move)",
                                    "loss" => "Loss",
                                    other => other,
                                };
                                view! {
                                    <div class="engine-body">
                                        <div class="best">{format!("{wdl} · {} pieces", t.pieces)}</div>
                                        <div class="muted">
                                            {t.best_move.clone().map(|m| format!("best: {m}")).unwrap_or_default()}
                                        </div>
                                        <div class="muted">{format!("tables: {}", t.path)}</div>
                                    </div>
                                }
                            })
                    }}
                    <div class="panel-sub">"Move classification"</div>
                    <div class="row-btns">
                        <button on:click=classify disabled=move || reporting.get() || variant_only.get()>
                            {move || if reporting.get() { "Classifying…" } else { "Classify game" }}
                        </button>
                        <a
                            class="link"
                            href=move || {
                                selected_game_id.get().map(api::game_pgn_url).unwrap_or_default()
                            }
                            target="_blank"
                            rel="noopener"
                        >
                            "Export PGN"
                        </a>
                        <button on:click=delete_current>"Delete"</button>
                    </div>
                    <div class="row-btns">
                        <select
                            prop:value=move || add_collection.get().to_string()
                            on:change=move |ev| {
                                add_collection.set(event_target_value(&ev).parse().unwrap_or(0))
                            }
                        >
                            <option value="0">"Add to collection…"</option>
                            {move || {
                                collections
                                    .get()
                                    .into_iter()
                                    .map(|c| {
                                        view! {
                                            <option value=c.id.to_string()>{c.name.clone()}</option>
                                        }
                                    })
                                    .collect_view()
                            }}
                        </select>
                        <button on:click=add_game_to_collection>"Add"</button>
                    </div>
                    {move || {
                        report
                            .get()
                            .map(|r| {
                                let n = r.moves.len().max(2) as f64;
                                let points = r
                                    .moves
                                    .iter()
                                    .enumerate()
                                    .map(|(i, m)| {
                                        let x = i as f64 * (400.0 / (n - 1.0));
                                        let y = 100.0 - m.win_after;
                                        format!("{x:.1},{y:.1}")
                                    })
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                view! {
                                    <div class="report">
                                        <div class="acc">
                                            {format!(
                                                "White {:.1}% · Black {:.1}%",
                                                r.white_accuracy.unwrap_or(0.0),
                                                r.black_accuracy.unwrap_or(0.0),
                                            )}
                                        </div>
                                        <div class="muted">
                                            {format!(
                                                "Lichess — inacc {} · mistakes {} · blunders {}",
                                                r.lichess_counts.inaccuracy,
                                                r.lichess_counts.mistake,
                                                r.lichess_counts.blunder,
                                            )}
                                        </div>
                                        <div class="muted">
                                            {format!(
                                                "Expected points — inacc {} · mistakes {} · blunders {}",
                                                r.expected_points_counts.inaccuracy,
                                                r.expected_points_counts.mistake,
                                                r.expected_points_counts.blunder,
                                            )}
                                        </div>
                                        <svg class="spark" viewBox="0 0 400 100" preserveAspectRatio="none">
                                            <polyline points=points fill="none" stroke="#7fd18a" stroke-width="2"></polyline>
                                        </svg>
                                        <div class="mv-list">
                                            {r
                                                .moves
                                                .iter()
                                                .map(|m| {
                                                    view! {
                                                        <span class=format!("mv {}", m.lichess)>{m.san.clone()}</span>
                                                    }
                                                })
                                                .collect_view()}
                                        </div>
                                    </div>
                                }
                            })
                    }}
                    <div class="panel-sub">"Analysis tree"</div>
                    {tree_nodes}
                    <div class="row-btns">
                        <button on:click=delete_tree_from_here>"Delete from here"</button>
                        <button on:click=promote_selected>"Promote variation"</button>
                        <label class="checkline">
                            <input
                                type="checkbox"
                                prop:checked=move || record_moves.get()
                                on:change=move |ev| record_moves.set(event_target_checked(&ev))
                            />
                            "Record moves"
                        </label>
                    </div>
                    <div class="panel-sub">"Assistant"</div>
                    <div class="row-btns">
                        <button on:click=move |_| dock.set("ai".into())>"AI chat"</button>
                        <button on:click=move |_| dock.set("console".into())>"Engine console"</button>
                    </div>
                    <div class="muted">
                        "The assistant can analyse, play moves and draw arrows on this board."
                    </div>
                    <div class="panel-sub">"Game tags"</div>
                    <div class="row-btns">
                        <button on:click=move |_| show_tags.update(|v| *v = !*v)>
                            {move || if show_tags.get() { "Hide tags" } else { "Edit tags" }}
                        </button>
                        <span class="muted">{move || game_detail.get().map(|d| d.event.unwrap_or_default()).unwrap_or_default()}</span>
                    </div>
                    {move || {
                        show_tags
                            .get()
                            .then(|| {
                                view! {
                                    <div class="tag-form">
                                        <label>"Event"<input type="text" prop:value=move || tag_event.get() on:input=move |ev| tag_event.set(event_target_value(&ev))/></label>
                                        <label>"Site"<input type="text" prop:value=move || tag_site.get() on:input=move |ev| tag_site.set(event_target_value(&ev))/></label>
                                        <label>"Date"<input type="text" placeholder="YYYY.MM.DD" prop:value=move || tag_date.get() on:input=move |ev| tag_date.set(event_target_value(&ev))/></label>
                                        <label>"Round"<input type="text" prop:value=move || tag_round.get() on:input=move |ev| tag_round.set(event_target_value(&ev))/></label>
                                        <label>"White"<input type="text" prop:value=move || tag_white.get() on:input=move |ev| tag_white.set(event_target_value(&ev))/></label>
                                        <label>"Black"<input type="text" prop:value=move || tag_black.get() on:input=move |ev| tag_black.set(event_target_value(&ev))/></label>
                                        <label>"Result"<input type="text" prop:value=move || tag_result.get() on:input=move |ev| tag_result.set(event_target_value(&ev))/></label>
                                        <label>"WhiteElo"<input type="text" prop:value=move || tag_white_elo.get() on:input=move |ev| tag_white_elo.set(event_target_value(&ev))/></label>
                                        <label>"BlackElo"<input type="text" prop:value=move || tag_black_elo.get() on:input=move |ev| tag_black_elo.set(event_target_value(&ev))/></label>
                                        <label>"TimeControl"<input type="text" prop:value=move || tag_time_control.get() on:input=move |ev| tag_time_control.set(event_target_value(&ev))/></label>
                                        <button on:click=save_tags>"Save tags"</button>
                                    </div>
                                }
                            })
                    }}
                    <div class="panel-sub">"Board editor"</div>
                    <div class="row-btns">
                        <button on:click=open_editor>
                            {move || if editor_open.get() { "Close editor" } else { "Edit position" }}
                        </button>
                    </div>
                    {move || {
                        editor_open
                            .get()
                            .then(|| {
                                let palette = editor_palette.get();
                                let options = ['K', 'Q', 'R', 'B', 'N', 'P', 'k', 'q', 'r', 'b', 'n', 'p'];
                                view! {
                                    <div class="editor">
                                        <div class="palette">
                                            {options
                                                .into_iter()
                                                .map(|ch| {
                                                    let cls = if palette == Some(ch) { "palette-btn active" } else { "palette-btn" };
                                                    view! {
                                                        <button
                                                            class=cls
                                                            on:click=move |_| editor_palette.set(Some(ch))
                                                        >
                                                            {piece_glyph(ch)}
                                                        </button>
                                                    }
                                                })
                                                .collect_view()}
                                            <button
                                                class=if palette.is_none() { "palette-btn active" } else { "palette-btn" }
                                                on:click=move |_| editor_palette.set(None)
                                            >
                                                "⌫"
                                            </button>
                                        </div>
                                        <div class="editor-grid">
                                            {move || {
                                                let pieces = editor_pieces.get();
                                                (0..64)
                                                    .map(|display| {
                                                        let rank = 7 - display / 8;
                                                        let file = display % 8;
                                                        let idx = rank * 8 + file;
                                                        let ch = pieces[idx];
                                                        let cls = format!(
                                                            "edit-sq {} {}",
                                                            if (display / 8 + display % 8) % 2 == 0 { "light" } else { "dark" },
                                                            ch.map(|c| if c.is_ascii_uppercase() { "white" } else { "black" }).unwrap_or(""),
                                                        );
                                                        view! {
                                                            <button
                                                                class=cls
                                                                data-edit-square=index_to_name(idx)
                                                                on:click=move |_| {
                                                                    let piece = editor_palette.get_untracked();
                                                                    editor_pieces.update(|v| v[idx] = piece);
                                                                }
                                                            >
                                                                {ch.map(piece_glyph).unwrap_or("·")}
                                                            </button>
                                                        }
                                                    })
                                                    .collect_view()
                                            }}
                                        </div>
                                        <div class="editor-fields">
                                            <label>"Turn"<input type="text" prop:value=move || editor_turn.get() on:input=move |ev| editor_turn.set(event_target_value(&ev))/></label>
                                            <label>"Castling"<input type="text" prop:value=move || editor_castling.get() on:input=move |ev| editor_castling.set(event_target_value(&ev))/></label>
                                            <label>"En passant"<input type="text" prop:value=move || editor_ep.get() on:input=move |ev| editor_ep.set(event_target_value(&ev))/></label>
                                            <label class="checkline">
                                                "Scan image"
                                                <input
                                                    type="file"
                                                    accept="image/*"
                                                    on:change=scan_image
                                                />
                                            </label>
                                            <label>
                                                "Variant"
                                                <select
                                                    prop:value=move || editor_variant.get()
                                                    on:change=move |ev| editor_variant.set(event_target_value(&ev))
                                                >
                                                    <option value="standard">"Standard"</option>
                                                    <option value="chess960">"Chess960"</option>
                                                    <option value="atomic">"Atomic"</option>
                                                    <option value="antichess">"Antichess"</option>
                                                    <option value="kingofthehill">"King of the Hill"</option>
                                                    <option value="3check">"Three-check"</option>
                                                    <option value="crazyhouse">"Crazyhouse"</option>
                                                    <option value="racingkings">"Racing Kings"</option>
                                                    <option value="horde">"Horde"</option>
                                                </select>
                                            </label>
                                        </div>
                                        <div class="row-btns">
                                            <button on:click=move |_| {
                                                editor_pieces.update(|pieces| {
                                                    for rank in 0..4 {
                                                        for file in 0..8 {
                                                            pieces.swap(rank * 8 + file, (7 - rank) * 8 + file);
                                                        }
                                                    }
                                                });
                                            }>"Flip ranks"</button>
                                            <button on:click=move |_| {
                                                editor_pieces.update(|pieces| {
                                                    for rank in 0..8 {
                                                        for file in 0..4 {
                                                            pieces.swap(rank * 8 + file, rank * 8 + (7 - file));
                                                        }
                                                    }
                                                });
                                            }>"Flip files"</button>
                                            <button on:click=move |_| {
                                                editor_pieces.update(|pieces| {
                                                    pieces.reverse();
                                                });
                                            }>"Rotate 180°"</button>
                                        </div>
                                        <button on:click=apply_editor>"Use position"</button>
                                        <div class="muted">{move || vision_msg.get()}</div>
                                    </div>
                                }
                            })
                    }}
                    <div class="panel-sub">"Edit moves"</div>
                    <div class="row-btns">
                        <button
                            on:click=move |_| {
                                if !edit_moves_open.get_untracked() {
                                    if let Some(id) = selected_game_id.get_untracked() {
                                        spawn_local(async move {
                                            if let Ok(text) = api::game_pgn_text(id).await {
                                                moves_text.set(text);
                                            }
                                        });
                                    }
                                }
                                edit_moves_open.update(|v| *v = !*v);
                            }
                        >
                            {move || if edit_moves_open.get() { "Close editor" } else { "Edit moves" }}
                        </button>
                        <button on:click=truncate_moves>"Keep up to current move"</button>
                    </div>
                    {move || {
                        edit_moves_open
                            .get()
                            .then(|| {
                                view! {
                                    <div class="pgn-box">
                                        <textarea
                                            placeholder="1. e4 e5 2. Nf3 (2. Bc4 Nf6) …"
                                            prop:value=move || moves_text.get()
                                            on:input=move |ev| moves_text.set(event_target_value(&ev))
                                        ></textarea>
                                        <button on:click=apply_moves>"Save moves"</button>
                                    </div>
                                }
                            })
                    }}
                    <div class="panel-sub">"Repertoire"</div>
                    <div class="row-btns">
                        <select
                            prop:value=move || analysis_rep.get().to_string()
                            on:change=move |ev| {
                                analysis_rep.set(event_target_value(&ev).parse().unwrap_or(0))
                            }
                        >
                            <option value="0">"Add line to…"</option>
                            {move || {
                                reps
                                    .get()
                                    .into_iter()
                                    .map(|r| {
                                        view! {
                                            <option value=r.id.to_string()>
                                                {format!("{} ({})", r.name, r.color)}
                                            </option>
                                        }
                                    })
                                    .collect_view()
                            }}
                        </select>
                        <button on:click=add_line_to_repertoire>"Add line"</button>
                        <button on:click=add_game_to_repertoire>"Add whole game"</button>
                    </div>
                    <div class="panel-sub">"Annotation"</div>
                    <div class="ann-editor">
                        <input
                            type="text"
                            placeholder="Comment for this ply…"
                            prop:value=move || ann_text.get()
                            on:input=move |ev| ann_text.set(event_target_value(&ev))
                        />
                        <select
                            prop:value=move || ann_nag.get().to_string()
                            on:change=move |ev| ann_nag.set(event_target_value(&ev).parse().unwrap_or(0))
                        >
                            <option value="0">"—"</option>
                            <option value="1">"!"</option>
                            <option value="2">"?"</option>
                            <option value="3">"!!"</option>
                            <option value="4">"??"</option>
                            <option value="5">"!?"</option>
                            <option value="6">"?!"</option>
                        </select>
                        <button on:click=save_annotation>"Save"</button>
                    </div>
                </aside>
                {move || {
                    let kind = dock.get();
                    if kind.is_empty() {
                        return None;
                    }
                    let ai = view! {
                        <div class="dock-body">
                            <div class="chat-messages">
                                {move || {
                                    chat_messages
                                        .get()
                                        .into_iter()
                                        .map(|(role, text)| {
                                            view! {
                                                <div class=format!("chat-msg {role}")>{text}</div>
                                            }
                                        })
                                        .collect_view()
                                }}
                            </div>
                            {move || {
                                let tools = chat_tools.get();
                                (!tools.is_empty()).then(|| view! {
                                    <div class="chat-tools">
                                        {tools
                                            .into_iter()
                                            .map(|tool| view! { <span class="tool-chip">{tool}</span> })
                                            .collect_view()}
                                    </div>
                                })
                            }}
                            <div class="row-btns">
                                <button on:click=move |_| {
                                    chat_text.set("Analyse this position.".into());
                                    send_chat();
                                }>"Analyse"</button>
                                <button on:click=move |_| {
                                    chat_text.set("Explain the plan and draw the key idea with arrows.".into());
                                    send_chat();
                                }>"Explain plan"</button>
                                <button on:click=move |_| ai_shapes.set(Vec::new())>"Clear arrows"</button>
                            </div>
                            <div class="chat-input">
                                <textarea
                                    placeholder="Ask the assistant…"
                                    prop:value=move || chat_text.get()
                                    on:input=move |ev| chat_text.set(event_target_value(&ev))
                                ></textarea>
                                <button on:click=move |_| send_chat()>"Send"</button>
                            </div>
                        </div>
                    }
                    .into_any();
                    let console = view! {
                        <div class="dock-body">
                            <div class="engine-fields">
                                <label>
                                    "Engine"
                                    <select
                                        prop:value=move || console_path.get()
                                        on:change=move |ev| console_path.set(event_target_value(&ev))
                                    >
                                        {move || {
                                            engines
                                                .get()
                                                .into_iter()
                                                .map(|engine| {
                                                    view! {
                                                        <option value=engine.path.clone()>
                                                            {format!("{} — {}", engine.name, engine.path)}
                                                        </option>
                                                    }
                                                })
                                                .collect_view()
                                        }}
                                    </select>
                                </label>
                                <div class="row-btns">
                                    <button on:click=console_start disabled=move || console_running.get()>
                                        "Start"
                                    </button>
                                    <button on:click=console_stop disabled=move || !console_running.get()>
                                        "Stop"
                                    </button>
                                    <span class="status">
                                        {move || {
                                            if console_running.get() {
                                                console_name.get()
                                            } else {
                                                "stopped".to_owned()
                                            }
                                        }}
                                    </span>
                                </div>
                            </div>
                            <pre class="console-log">{move || console_log.get().join("\n")}</pre>
                            <div class="chat-input">
                                <input
                                    type="text"
                                    placeholder="UCI command, e.g. go depth 12"
                                    prop:value=move || console_cmd.get()
                                    on:input=move |ev| console_cmd.set(event_target_value(&ev))
                                    on:keydown=move |ev| {
                                        if ev.key() == "Enter" {
                                            console_send();
                                        }
                                    }
                                />
                                <button on:click=move |_| console_send() disabled=move || console_busy.get()>
                                    "Send"
                                </button>
                            </div>
                        </div>
                    }
                    .into_any();
                    Some(view! {
                        <div class="engine-dock">
                            <div class="dock-head">
                                <div class="dock-tabs">
                                    <button
                                        class=if kind == "ai" { "dock-tab active" } else { "dock-tab" }
                                        on:click=move |_| dock.set("ai".into())
                                    >
                                        "AI"
                                    </button>
                                    <button
                                        class=if kind == "console" { "dock-tab active" } else { "dock-tab" }
                                        on:click=move |_| dock.set("console".into())
                                    >
                                        "Console"
                                    </button>
                                </div>
                                <button class="dock-close" on:click=move |_| dock.set(String::new())>
                                    "×"
                                </button>
                            </div>
                            {if kind == "ai" { ai } else { console }}
                        </div>
                    })
                }}
            </section>
        }
        .into_any()
    };

    let normalize_san = |san: &str| -> String {
        san.chars()
            .filter(|c| !matches!(c, '+' | '#' | '!' | '?'))
            .collect::<String>()
            .to_lowercase()
    };

    let start_guess = move |id: i64| {
        if id <= 0 {
            practice_feedback.set("choose a game first".into());
            return;
        }
        spawn_local(async move {
            match api::get_game(id).await {
                Ok(detail) => {
                    if detail.moves.trim().is_empty() {
                        practice_feedback.set("that game has no moves".into());
                        return;
                    }
                    let sans: Vec<String> =
                        detail.moves.split_whitespace().map(str::to_owned).collect();
                    let variant = detail
                        .variant
                        .clone()
                        .unwrap_or_else(|| "standard".into());
                    practice_sans.set(sans.clone());
                    practice_step.set(0);
                    practice_hits.set(0);
                    practice_misses.set(0);
                    practice_feedback
                        .set(format!("Play the game's move — {} to go", sans.len()));
                    practice_pgn.set(line_pgn(&detail.fen, &[], &variant));
                    let seq = practice_seq.get_untracked() + 1;
                    practice_seq.set(seq);
                    practice_goto.set(Some((seq, 0)));
                    practice_game.set(Some(detail));
                }
                Err(e) => practice_feedback.set(e),
            }
        });
    };

    let on_practice_move = move |(_uci, san, _fen): (String, String, String)| {
        let sans = practice_sans.get_untracked();
        let step = practice_step.get_untracked();
        if step >= sans.len() {
            return;
        }
        if normalize_san(&san) == normalize_san(&sans[step]) {
            let hits = practice_hits.get_untracked() + 1;
            practice_hits.set(hits);
            practice_step.set(step + 1);
            if step + 1 >= sans.len() {
                practice_feedback.set(format!(
                    "Finished — {} of {} moves matched",
                    hits,
                    sans.len()
                ));
            } else {
                practice_feedback.set(format!(
                    "Correct — {} moves to go",
                    sans.len() - step - 1
                ));
            }
        } else {
            practice_misses.update(|m| *m += 1);
            practice_feedback
                .set(format!("Not the game move — the game played {}", sans[step]));
            let seq = practice_seq.get_untracked() + 1;
            practice_seq.set(seq);
            practice_goto.set(Some((seq, step)));
        }
    };

    let start_spar = move |_| {
        let entered = spar_fen.get_untracked();
        let fen = if entered.trim().is_empty() {
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".to_owned()
        } else {
            entered
        };
        spar_pgn.set(line_pgn(&fen, &[], "standard"));
        spar_play.set(None);
        spar_status.set(format!(
            "You are {}. Play a move.",
            if spar_color.get_untracked() == "black" {
                "Black"
            } else {
                "White"
            }
        ));
    };

    let on_spar_move = move |(_uci, _san, fen_after): (String, String, String)| {
        let engine_white = spar_color.get_untracked() == "black";
        let turn_white = fen_after.split_whitespace().nth(1) != Some("b");
        spar_status.set("Engine thinking…".into());
        if turn_white == engine_white {
            spar_thinking.set(true);
            let depth = settings_depth.get_untracked();
            spawn_local(async move {
                match api::analyze(&fen_after, depth, "standard").await {
                    Ok(a) => {
                        let seq = spar_seq.get_untracked() + 1;
                        spar_seq.set(seq);
                        spar_play.set(Some((seq, a.best_move)));
                    }
                    Err(e) => spar_status.set(e),
                }
                spar_thinking.set(false);
            });
        } else {
            spar_status.set("Your move.".into());
        }
    };

    let practice_view = move || {
        view! {
            <section class="practice">
                <div class="panel practice-controls">
                    <div class="row-btns">
                        <button
                            class=move || {
                                if practice_mode.get() == "guess" { "mode active" } else { "mode" }
                            }
                            on:click=move |_| practice_mode.set("guess".into())
                        >
                            "Guess the move"
                        </button>
                        <button
                            class=move || {
                                if practice_mode.get() == "spar" { "mode active" } else { "mode" }
                            }
                            on:click=move |_| practice_mode.set("spar".into())
                        >
                            "Sparring"
                        </button>
                    </div>
                    {move || {
                        if practice_mode.get() == "guess" {
                            view! {
                                <div class="practice-row">
                                    <input
                                        type="text"
                                        placeholder="Filter games by player or event…"
                                        prop:value=move || practice_query.get()
                                        on:input=move |ev| practice_query.set(event_target_value(&ev))
                                    />
                                    <select
                                        prop:value=move || practice_pick.get().to_string()
                                        on:change=move |ev| {
                                            practice_pick
                                                .set(event_target_value(&ev).parse().unwrap_or(0))
                                        }
                                    >
                                        <option value="0">"Choose a game…"</option>
                                        {move || {
                                            practice_games
                                                .get()
                                                .into_iter()
                                                .map(|g| {
                                                    view! {
                                                        <option value=g.id.to_string()>
                                                            {format!("{} vs {} · {}", g.white, g.black, g.result)}
                                                        </option>
                                                    }
                                                })
                                                .collect_view()
                                        }}
                                    </select>
                                    <button on:click=move |_| start_guess(practice_pick.get_untracked())>
                                        "Practise"
                                    </button>
                                    <button
                                        disabled=move || selected_game_id.get().is_none()
                                        on:click=move |_| {
                                            if let Some(id) = selected_game_id.get_untracked() {
                                                start_guess(id);
                                            }
                                        }
                                    >
                                        "Use the game in Analysis"
                                    </button>
                                </div>
                            }
                                .into_any()
                        } else {
                            view! {
                                <div class="practice-row">
                                    <input
                                        type="text"
                                        placeholder="Starting FEN (empty = start position)"
                                        prop:value=move || spar_fen.get()
                                        on:input=move |ev| spar_fen.set(event_target_value(&ev))
                                    />
                                    <select
                                        prop:value=move || spar_color.get()
                                        on:change=move |ev| spar_color.set(event_target_value(&ev))
                                    >
                                        <option value="white">"Play White"</option>
                                        <option value="black">"Play Black"</option>
                                    </select>
                                    <button on:click=start_spar>"Start"</button>
                                </div>
                            }
                                .into_any()
                        }
                    }}
                </div>

                <div class="analysis practice-board">
                    <div class="board-pane">
                        {move || {
                            if practice_mode.get() == "guess" {
                                let side = practice_game
                                    .get()
                                    .map(|g| format!("{} vs {}", g.white, g.black))
                                    .unwrap_or_else(|| "Guess the move".into());
                                view! {
                                    <Board
                                        pgn=practice_pgn
                                        title=Signal::derive(move || side.clone())
                                        on_move=Callback::new(on_practice_move)
                                        goto=Signal::derive(move || practice_goto.get())
                                        keybinds=Signal::derive(move || keybinds.get())
                                        theme=Signal::derive(move || settings_theme.get())
                                        pieces=Signal::derive(move || settings_pieces.get())
                                    />
                                }
                                    .into_any()
                            } else {
                                view! {
                                    <Board
                                        pgn=spar_pgn
                                        title=Signal::derive(move || "Sparring".to_string())
                                        orientation=Signal::derive(move || spar_color.get())
                                        play=Signal::derive(move || spar_play.get())
                                        on_move=Callback::new(on_spar_move)
                                        on_play=Callback::new(move |_: (String, String, String)| {
                                            spar_status.set("Your move.".into());
                                        })
                                        keybinds=Signal::derive(move || keybinds.get())
                                        theme=Signal::derive(move || settings_theme.get())
                                        pieces=Signal::derive(move || settings_pieces.get())
                                    />
                                }
                                    .into_any()
                            }
                        }}
                    </div>
                    <aside class="engine-pane">
                        <div class="panel-head"><h2>"Practice"</h2></div>
                        {move || {
                            if practice_mode.get() == "guess" {
                                let total = practice_sans.get().len();
                                let step = practice_step.get();
                                view! {
                                    <div class="engine-body">
                                        <div class="practice-stats">
                                            {format!("Move {} / {}", (step + 1).min(total), total)}
                                        </div>
                                        <div class="muted">
                                            {format!(
                                                "{} correct · {} missed",
                                                practice_hits.get(),
                                                practice_misses.get(),
                                            )}
                                        </div>
                                        <div class="best">{move || practice_feedback.get()}</div>
                                        <div class="row-btns">
                                            <button
                                                disabled=move || practice_sans.get().is_empty()
                                                on:click=move |_| {
                                                    practice_step.set(0);
                                                    practice_hits.set(0);
                                                    practice_misses.set(0);
                                                    practice_feedback.set(String::new());
                                                    let seq = practice_seq.get_untracked() + 1;
                                                    practice_seq.set(seq);
                                                    practice_goto.set(Some((seq, 0)));
                                                    if let Some(game) = practice_game.get_untracked() {
                                                        let variant = game
                                                            .variant
                                                            .clone()
                                                            .unwrap_or_else(|| "standard".into());
                                                        practice_pgn.set(line_pgn(&game.fen, &[], &variant));
                                                    }
                                                }
                                            >
                                                "Restart"
                                            </button>
                                            <button on:click=move |_| {
                                                if let Some(next) = practice_sans
                                                    .get_untracked()
                                                    .get(practice_step.get_untracked())
                                                {
                                                    practice_feedback
                                                        .set(format!("The game move is {next}"));
                                                }
                                            }>"Reveal"</button>
                                        </div>
                                    </div>
                                }
                                    .into_any()
                            } else {
                                view! {
                                    <div class="engine-body">
                                        <div class="best">{move || spar_status.get()}</div>
                                        <div class="muted">
                                            {move || {
                                                if spar_thinking.get() {
                                                    "engine is thinking…"
                                                } else {
                                                    ""
                                                }
                                            }}
                                        </div>
                                        <div class="row-btns">
                                            <button on:click=start_spar>"New game"</button>
                                        </div>
                                    </div>
                                }
                                    .into_any()
                            }
                        }}
                    </aside>
                </div>
            </section>
        }
        .into_any()
    };

    let opening_view = move || {
        let explore = move |_| {
            let f = opening_fen.get();
            if f.is_empty() {
                return;
            }
            let src = opening_source.get_untracked();
            let source = (src != "local").then_some(src);
            spawn_local(async move {
                match api::explore(&f, source.as_deref()).await {
                    Ok(e) => opening.set(Some(e)),
                    Err(e) => status.set(e),
                }
            });
        };
        let explorer = move || {
            view! {
                <div class="load">
                    <input
                        type="text"
                        placeholder="FEN…"
                        prop:value=move || opening_fen.get()
                        on:input=move |ev| opening_fen.set(event_target_value(&ev))
                    />
                    <button on:click=explore>"Explore"</button>
                    <button
                        title="Use the position currently on the analysis board"
                        on:click=move |_| {
                            let current = fen.get();
                            if !current.is_empty() {
                                opening_fen.set(current);
                                let src = opening_source.get_untracked();
                                let source = (src != "local").then_some(src);
                                spawn_local(async move {
                                    match api::explore(&opening_fen.get_untracked(), source.as_deref()).await {
                                        Ok(e) => opening.set(Some(e)),
                                        Err(e) => status.set(e),
                                    }
                                });
                            }
                        }
                    >
                        "Current board"
                    </button>
                </div>
                {move || {
                    opening
                        .get()
                        .map(|o| {
                            let title = format!(
                                "{} {}",
                                o.eco.clone().unwrap_or_default(),
                                o.name.clone().unwrap_or_default(),
                            );
                            view! {
                                <div class="opening">
                                    <div class="opening-name">{title}</div>
                                    <div class="table open">
                                        <div class="row head">
                                            <span>"Move"</span><span>"Games"</span>
                                            <span>"White"</span><span>"Draw"</span><span>"Black"</span>
                                        </div>
                                        {o
                                            .continuations
                                            .into_iter()
                                            .map(|c| {
                                                view! {
                                                    <div class="row">
                                                        <span>{c.san}</span>
                                                        <span>{c.games.to_string()}</span>
                                                        <span>{c.white.to_string()}</span>
                                                        <span>{c.draw.to_string()}</span>
                                                        <span>{c.black.to_string()}</span>
                                                    </div>
                                                }
                                            })
                                            .collect_view()}
                                    </div>
                                </div>
                            }
                        })
                }}
            }
            .into_any()
        };

        let load_card = move |id: i64| {
            rep_id.set(id);
            rep_feedback.set(String::new());
            spawn_local(async move {
                match api::next_repertoire_card(id).await {
                    Ok(Some(card)) => {
                        rep_board.set(line_pgn(&card.fen, &[], "standard"));
                        rep_card.set(Some(card));
                    }
                    Ok(None) => {
                        rep_card.set(None);
                        rep_feedback.set("No cards due right now.".into());
                    }
                    Err(e) => rep_feedback.set(e),
                }
            });
        };

        let next_card = move |_| load_card(rep_id.get_untracked());

        let create_rep = move |_: leptos::ev::MouseEvent| {
            let name = rep_name.get();
            if name.trim().is_empty() {
                return;
            }
            let color = rep_color.get_untracked();
            spawn_local(async move {
                match api::create_repertoire(name.trim(), &color).await {
                    Ok(r) => {
                        rep_name.set(String::new());
                        status.set(format!("repertoire '{}' created", r.name));
                        rep_selected.set(r.id);
                        if let Ok(list) = api::repertoires().await {
                            reps.set(list);
                        }
                    }
                    Err(e) => status.set(e),
                }
            });
        };

        let delete_rep = move |id: i64| {
            spawn_local(async move {
                match api::delete_repertoire(id).await {
                    Ok(()) => {
                        if rep_id.get_untracked() == id {
                            rep_card.set(None);
                        }
                        if let Ok(list) = api::repertoires().await {
                            reps.set(list);
                        }
                    }
                    Err(e) => status.set(e),
                }
            });
        };

        let import_rep = move |id: i64| {
            let pgn = rep_pgn_input.get();
            if pgn.trim().is_empty() {
                return;
            }
            spawn_local(async move {
                match api::import_repertoire_pgn(id, &pgn).await {
                    Ok(n) => {
                        rep_pgn_input.set(String::new());
                        status.set(format!("imported {n} repertoire card(s)"));
                        if let Ok(list) = api::repertoires().await {
                            reps.set(list);
                        }
                    }
                    Err(e) => status.set(e),
                }
            });
        };

        let on_rep_move = move |(uci, _san, _fen): (String, String, String)| {
            let Some(card) = rep_card.get_untracked() else {
                return;
            };
            if !rep_feedback.get_untracked().is_empty() {
                return;
            }
            let correct = uci == card.uci;
            let expected = card.san.clone();
            spawn_local(async move {
                match api::review_repertoire_card(card.id, correct).await {
                    Ok(updated) => {
                        rep_correct.set(correct);
                        rep_feedback.set(if correct {
                            format!(
                                "Correct — next review in {} day(s)",
                                updated.interval_days.max(1.0) as i64
                            )
                        } else {
                            format!("Wrong — expected {expected}")
                        });
                        rep_card.set(Some(updated));
                        if let Ok(list) = api::repertoires().await {
                            reps.set(list);
                        }
                    }
                    Err(e) => rep_feedback.set(e),
                }
            });
        };

        view! {
            <div class="opening-shell">
                {move || {
                    rep_card
                        .get()
                        .map(|card| {
                            view! {
                                <section class="analysis">
                                    <div class="board-pane">
                                        <Board
                                            pgn=rep_board
                                            title=Signal::derive(move || "Repertoire training".to_string())
                                            orientation=Signal::derive(move || {
                                                reps
                                                    .get()
                                                    .into_iter()
                                                    .find(|r| r.id == rep_id.get())
                                                    .map(|r| r.color)
                                                    .unwrap_or_else(|| "white".into())
                                            })
                                            on_move=Callback::new(on_rep_move)
                                            theme=Signal::derive(move || settings_theme.get())
                                            pieces=Signal::derive(move || settings_pieces.get())
                                        />
                                    </div>
                                    <aside class="engine-pane">
                                        <div class="panel-head"><h2>"Repertoire"</h2></div>
                                        <div class="best">
                                            {format!(
                                                "Play {}'s move",
                                                if card.fen.split_whitespace().nth(1) == Some("b") {
                                                    "Black"
                                                } else {
                                                    "White"
                                                },
                                            )}
                                        </div>
                                        <div class="muted">{move || rep_feedback.get()}</div>
                                        <div class="row-btns">
                                            <button on:click=next_card>"Next card"</button>
                                        </div>
                                    </aside>
                                </section>
                            }
                        })
                }}
                <section class="panel">
                    <div class="panel-head">
                        <h2>"Openings"</h2>
                        <div class="row-btns">
                            <button
                                class=move || if opening_mode.get() == "explorer" { "chip active" } else { "chip" }
                                on:click=move |_| opening_mode.set("explorer".into())
                            >
                                "Explorer"
                            </button>
                            <button
                                class=move || if opening_mode.get() == "repertoire" { "chip active" } else { "chip" }
                                on:click=move |_| opening_mode.set("repertoire".into())
                            >
                                "Repertoires"
                            </button>
                        </div>
                    </div>
                    {move || {
                        if opening_mode.get() == "explorer" {
                            view! {
                                <div class="load">
                                    <select
                                        prop:value=move || opening_source.get()
                                        on:change=move |ev| opening_source.set(event_target_value(&ev))
                                    >
                                        <option value="local">"Local games"</option>
                                        <option value="lichess">"Lichess (online)"</option>
                                        <option value="masters">"Masters (online)"</option>
                                    </select>
                                </div>
                                {explorer()}
                            }
                            .into_any()
                        } else {
                            view! {
                                <div class="dbs">
                                    <input
                                        type="text"
                                        placeholder="Repertoire name…"
                                        prop:value=move || rep_name.get()
                                        on:input=move |ev| rep_name.set(event_target_value(&ev))
                                    />
                                    <select
                                        prop:value=move || rep_color.get()
                                        on:change=move |ev| rep_color.set(event_target_value(&ev))
                                    >
                                        <option value="white">"White"</option>
                                        <option value="black">"Black"</option>
                                    </select>
                                    <button on:click=create_rep>"Create"</button>
                                </div>
                                <div class="rep-list">
                                    {move || {
                                        reps
                                            .get()
                                            .into_iter()
                                            .map(|r| {
                                                let id = r.id;
                                                let for_import = id;
                                                let for_delete = id;
                                                let for_train = id;
                                                view! {
                                                    <div class="rep-row">
                                                        <span class="rep-name">
                                                            {format!(
                                                                "{} · {} · {} cards · {} due",
                                                                r.name, r.color, r.cards, r.due,
                                                            )}
                                                        </span>
                                                        <button on:click=move |_| load_card(for_train)>
                                                            "Train"
                                                        </button>
                                                        <button on:click=move |_| rep_selected.set(for_import)>
                                                            "Import PGN"
                                                        </button>
                                                        <button on:click=move |_| delete_rep(for_delete)>
                                                            "Delete"
                                                        </button>
                                                    </div>
                                                }
                                            })
                                            .collect_view()
                                    }}
                                </div>
                                {move || {
                                    let id = rep_selected.get();
                                    (id > 0)
                                        .then(|| {
                                            view! {
                                                <div class="pgn-box">
                                                    <textarea
                                                        placeholder="Paste a repertoire PGN (mainline and variations)…"
                                                        prop:value=move || rep_pgn_input.get()
                                                        on:input=move |ev| rep_pgn_input.set(event_target_value(&ev))
                                                    ></textarea>
                                                    <button on:click=move |_| import_rep(id)>
                                                        "Import into selected repertoire"
                                                    </button>
                                                </div>
                                            }
                                        })
                                }}
                                <div class="panel-sub">"Training"</div>
                                <div class="muted">
                                    {move || rep_feedback.get()}
                                </div>
                                <div class="row-btns">
                                    <button on:click=next_card>"Next card"</button>
                                    {move || {
                                        rep_card
                                            .get()
                                            .map(|card| {
                                                view! {
                                                    <span class="muted">
                                                        {format!(
                                                            "ease {:.2} · {} reviews · {} lapses",
                                                            card.ease, card.reviews, card.lapses,
                                                        )}
                                                    </span>
                                                }
                                            })
                                    }}
                                </div>
                            }
                            .into_any()
                        }
                    }}
                </section>
            </div>
        }
        .into_any()
    };

    let puzzle_view = move || {
        let load = move |_| {
            let theme = puzzle_theme.get_untracked();
            let theme = if theme.is_empty() { None } else { Some(theme) };
            let rating = puzzle_rating.get_untracked();
            spawn_local(async move {
                match api::next_puzzle(Some(rating), theme.as_deref()).await {
                    Ok(Some(p)) => {
                        let ucis: Vec<String> =
                            p.moves.split_whitespace().map(str::to_owned).collect();
                        let sans: Vec<String> =
                            p.moves_san.split_whitespace().map(str::to_owned).collect();
                        let has_setup = ucis.len() > 1;
                        let start_ply = usize::from(has_setup);
                        let turn_black = p.fen.split_whitespace().nth(1) == Some("b");
                        let solver_black = if has_setup { !turn_black } else { turn_black };
                        puzzle_orientation
                            .set(if solver_black { "black".into() } else { "white".into() });
                        puzzle_solution.set(ucis);
                        puzzle_san.set(sans.clone());
                        puzzle_step.set(start_ply);
                        puzzle_state.set(String::new());
                        puzzle_recorded.set(false);
                        puzzle_delta.set(None);
                        show_solution.set(false);
                        let upto = if has_setup { 1 } else { 0 };
                        puzzle_pgn.set(line_pgn(&p.fen, &sans[..upto.min(sans.len())], "standard"));
                        puzzle.set(Some(p));
                    }
                    Ok(None) => status.set("no puzzles installed".into()),
                    Err(e) => status.set(e),
                }
            });
        };

        let retry = move |_| {
            let Some(p) = puzzle.get_untracked() else { return };
            let sans = puzzle_san.get_untracked();
            let ucis = puzzle_solution.get_untracked();
            let has_setup = ucis.len() > 1;
            puzzle_step.set(usize::from(has_setup));
            puzzle_state.set(String::new());
            puzzle_recorded.set(false);
            puzzle_delta.set(None);
            let upto = if has_setup { 1 } else { 0 };
            puzzle_pgn.set(line_pgn(&p.fen, &sans[..upto.min(sans.len())], "standard"));
        };

        let on_puzzle_move = move |(uci, _san, _fen): (String, String, String)| {
            let Some(p) = puzzle.get_untracked() else { return };
            let solution = puzzle_solution.get_untracked();
            let step = puzzle_step.get_untracked();
            if step >= solution.len() {
                return;
            }
            if solution[step] == uci {
                let mut next = step + 1;
                if next < solution.len() {
                    // The opponent's reply is part of the solution line.
                    next += 1;
                    let sans = puzzle_san.get_untracked();
                    let upto = next.min(sans.len());
                    puzzle_pgn.set(line_pgn(&p.fen, &sans[..upto], "standard"));
                }
                puzzle_step.set(next);
                if next >= solution.len() {
                    puzzle_state.set("solved".into());
                    let id = p.id.clone();
                    let rating = p.rating;
                    if !puzzle_recorded.get_untracked() {
                        puzzle_recorded.set(true);
                        spawn_local(async move {
                            if let Ok((before, after)) = api::puzzle_attempt(&id, rating, true).await
                            {
                                puzzle_delta.set(Some((before, after)));
                                puzzle_rating.set(after);
                                if let Ok(list) = api::puzzle_attempts(8).await {
                                    attempts.set(list);
                                }
                            }
                        });
                    }
                } else {
                    puzzle_state.set("correct".into());
                }
            } else {
                puzzle_state.set("wrong".into());
                let id = p.id.clone();
                let rating = p.rating;
                if !puzzle_recorded.get_untracked() {
                    puzzle_recorded.set(true);
                    spawn_local(async move {
                        if let Ok((before, after)) = api::puzzle_attempt(&id, rating, false).await {
                            puzzle_delta.set(Some((before, after)));
                            puzzle_rating.set(after);
                            if let Ok(list) = api::puzzle_attempts(8).await {
                                attempts.set(list);
                            }
                        }
                    });
                }
            }
        };

        let feedback = move || match puzzle_state.get().as_str() {
            "correct" => "Correct — keep going.".to_owned(),
            "wrong" => format!(
                "Not the puzzle move. Solution: {}",
                puzzle_san.get().join(" ")
            ),
            "solved" => match puzzle_delta.get() {
                Some((before, after)) => format!("Solved! Rating {before} → {after}"),
                None => "Solved!".to_owned(),
            },
            _ => String::new(),
        };

        view! {
            <section class="analysis">
                <div class="board-pane">
                    <Board
                        pgn=puzzle_pgn
                        title=Signal::derive(move || {
                            puzzle
                                .get()
                                .map(|p| format!("Puzzle {} · {}", p.id, p.rating))
                                .unwrap_or_else(|| "Puzzle".into())
                        })
                        on_move=Callback::new(on_puzzle_move)
                        orientation=Signal::derive(move || puzzle_orientation.get())
                        theme=Signal::derive(move || settings_theme.get())
                        pieces=Signal::derive(move || settings_pieces.get())
                    />
                </div>
                <aside class="engine-pane">
                    <div class="panel-head"><h2>"Puzzles"</h2></div>
                    <div class="row-btns">
                        <select
                            prop:value=move || puzzle_theme.get()
                            on:change=move |ev| puzzle_theme.set(event_target_value(&ev))
                        >
                            <option value="">"Any theme"</option>
                            {move || {
                                puzzle_themes
                                    .get()
                                    .into_iter()
                                    .map(|t| {
                                        view! {
                                            <option value=t.theme.clone()>
                                                {format!("{} ({})", t.theme, t.count)}
                                            </option>
                                        }
                                    })
                                    .collect_view()
                            }}
                        </select>
                        <input
                            type="number"
                            min="400"
                            max="3000"
                            style="width:5.5rem"
                            prop:value=move || puzzle_rating.get().to_string()
                            on:input=move |ev| {
                                puzzle_rating.set(event_target_value(&ev).parse().unwrap_or(1500))
                            }
                        />
                        <button on:click=load>"Next puzzle"</button>
                    </div>
                    {move || {
                        puzzle
                            .get()
                            .map(|p| {
                                let solution = p.moves_san.clone();
                                view! {
                                    <div class="engine-body">
                                        <div class="best">
                                            {format!(
                                                "{} to move",
                                                if puzzle_orientation.get() == "black" { "Black" } else { "White" },
                                            )}
                                        </div>
                                        <div class="muted">{p.themes.clone().unwrap_or_default()}</div>
                                        <div class="muted">{format!("Puzzle {} · Lichess rating {}", p.id, p.rating)}</div>
                                        <div class="row-btns">
                                            <button on:click=move |_| show_solution.update(|v| *v = !*v)>
                                                {move || if show_solution.get() { "Hide solution" } else { "Show solution" }}
                                            </button>
                                            <button on:click=retry>"Retry"</button>
                                        </div>
                                        {move || {
                                            show_solution
                                                .get()
                                                .then(|| view! { <div class="pv">{solution.clone()}</div> })
                                        }}
                                        {move || {
                                            let msg = feedback();
                                            (!msg.is_empty()).then(|| view! {
                                                <div class=format!(
                                                    "puzzle-feedback {}",
                                                    puzzle_state.get(),
                                                )>{msg}</div>
                                            })
                                        }}
                                    </div>
                                }
                            })
                    }}
                    <div class="panel-sub">"Recent attempts"</div>
                    {move || {
                        attempts
                            .get()
                            .into_iter()
                            .map(|a| {
                                view! {
                                    <div class="attempt">
                                        {format!(
                                            "{} · {} · {} → {}",
                                            a.puzzle_id,
                                            if a.solved { "solved" } else { "failed" },
                                            a.rating_before,
                                            a.rating_after,
                                        )}
                                    </div>
                                }
                            })
                            .collect_view()
                    }}
                    <div class="panel-sub">"Import puzzles (Lichess CSV)"</div>
                    <textarea
                        class="import-csv"
                        placeholder="PuzzleId,FEN,Moves,Rating,…"
                        prop:value=move || puzzle_csv.get()
                        on:input=move |ev| puzzle_csv.set(event_target_value(&ev))
                    ></textarea>
                    <button on:click=move |_| {
                        let csv = puzzle_csv.get();
                        if csv.trim().is_empty() {
                            return;
                        }
                        spawn_local(async move {
                            match api::import_puzzles(&csv).await {
                                Ok(n) => {
                                    status.set(format!("imported {n} puzzles"));
                                    puzzle_csv.set(String::new());
                                    if let Ok(t) = api::puzzle_themes().await {
                                        puzzle_themes.set(t);
                                    }
                                }
                                Err(e) => status.set(e),
                            }
                        });
                    }>"Import CSV"</button>
                </aside>
            </section>
        }
        .into_any()
    };

    view! {
        <div class="app">
            <aside class="sidebar">
                <div class="brand">"RustyBrickChess"</div>
                <nav>
                    {[Tab::Home, Tab::Database, Tab::Analysis, Tab::Practice, Tab::Openings, Tab::Puzzles, Tab::Stats, Tab::Settings]
                        .into_iter()
                        .map(|t| {
                            let label = match t {
                                Tab::Home => "Home",
                                Tab::Database => "Database",
                                Tab::Analysis => "Analysis",
                                Tab::Practice => "Practice",
                                Tab::Openings => "Openings",
                                Tab::Puzzles => "Puzzles",
                                Tab::Stats => "Stats",
                                Tab::Settings => "Settings",
                            };
                            view! {
                                <button
                                    class=move || if tab.get() == t { "nav active" } else { "nav" }
                                    on:click=move |_| switch_tab(t)
                                >
                                    {label}
                                </button>
                            }
                        })
                        .collect_view()}
                </nav>
                <div class="side-foot">"AGPL-3.0 · BrickChess"</div>
            </aside>
            <main class="main">
                <header class="topbar">
                    <input
                        placeholder="Search player…"
                        prop:value=move || query.get()
                        on:input=move |ev| query.set(event_target_value(&ev))
                        on:keydown=move |ev| {
                            if ev.key() == "Enter" { reload(); }
                        }
                    />
                    <button on:click=move |_| reload()>"Search"</button>
                    <span class="status">{move || status.get()}</span>
                </header>
                <div class="workspaces">
                    <span class="ws-label">"Workspaces"</span>
                    {move || {
                        let current = active_ws.get();
                        workspaces
                            .get()
                            .into_iter()
                            .map(|ws| {
                                let key = ws.key;
                                let title = ws.title.clone();
                                view! {
                                    <div class=move || {
                                        if current == key { "ws-tab active" } else { "ws-tab" }
                                    }>
                                        <button
                                            class="ws-name"
                                            title=title.clone()
                                            on:click=move |_| activate_workspace(key)
                                        >
                                            {title.clone()}
                                        </button>
                                        <button
                                            class="ws-close"
                                            title="Close workspace"
                                            on:click=move |_| close_workspace(key)
                                        >
                                            "×"
                                        </button>
                                    </div>
                                }
                            })
                            .collect_view()
                    }}
                    <button class="ws-new" on:click=move |_| new_workspace()>
                        "+ New analysis"
                    </button>
                </div>
                {move || match tab.get() {
                    Tab::Home => home_view().into_any(),
                    Tab::Database => database_view().into_any(),
                    Tab::Analysis => analysis_view().into_any(),
                    Tab::Practice => practice_view().into_any(),
                    Tab::Openings => opening_view().into_any(),
                    Tab::Puzzles => puzzle_view().into_any(),
                    Tab::Stats => stats_view().into_any(),
                    Tab::Settings => settings_view().into_any(),
                }}
            </main>
        </div>
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    mount_to_body(|| view! { <App/> });
}
