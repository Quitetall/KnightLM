//! RustyBrickChess frontend — a Leptos, En-Croissant-shaped app.

pub mod api;
pub mod board;
pub mod i18n;
mod views;
pub mod sound;

use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

use board::Keybinds;

mod model;
use model::*;

#[component]
pub fn App() -> impl IntoView {
    let tab = RwSignal::new(Tab::Home);
    let workspaces = RwSignal::new(Vec::<Workspace>::new());
    let active_ws = RwSignal::new(0u32);
    let ws_seq = RwSignal::new(1u32);
    let renaming_ws = RwSignal::new(None::<u32>);
    let rename_text = RwSignal::new(String::new());
    let new_tab_kind = RwSignal::new(String::new());
    let session_restored = RwSignal::new(false);
    let profile_name = RwSignal::new("Player".to_string());
    let profile_player = RwSignal::new(String::new());
    let profile = RwSignal::new(None::<api::Profile>);
    let keybinds = RwSignal::new(Keybinds::default());
    let home_recent = RwSignal::new(Vec::<api::GameSummary>::new());
    let accounts = RwSignal::new(Vec::<api::AccountState>::new());
    let lichess_user_input = RwSignal::new(String::new());
    let lichess_token_input = RwSignal::new(String::new());
    let chesscom_user_input = RwSignal::new(String::new());
    let account_busy = RwSignal::new(false);
    let account_msg = RwSignal::new(String::new());
    let account_import_max = RwSignal::new(20u32);
    let games = RwSignal::new(Vec::<api::GameSummary>::new());
    let total = RwSignal::new(0i64);
    let query = RwSignal::new(String::new());
    let status = RwSignal::new(String::new());
    let import_text = RwSignal::new(String::new());
    let recent_files = RwSignal::new(read_recent_files());
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
    let settings_light = RwSignal::new("#eeeed2".to_string());
    let settings_dark = RwSignal::new("#769656".to_string());
    let settings_coords = RwSignal::new(true);
    let language = RwSignal::new("en".to_string());
    let custom_sets = RwSignal::new(Vec::<api::PieceSetInfo>::new());
    let cloud = RwSignal::new(None::<api::CloudEval>);
    let annotating = RwSignal::new(false);
    let piece_set_name = RwSignal::new(String::new());
    let piece_set_msg = RwSignal::new(String::new());
    let piece_upload_busy = RwSignal::new(false);
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
    let match_white = RwSignal::new(0usize);
    let match_black = RwSignal::new(1usize);
    let match_depth = RwSignal::new(10u32);
    let match_start_fen = RwSignal::new(String::new());
    let match_pgn = RwSignal::new(String::new());
    let match_play = RwSignal::new(None::<(u32, String)>);
    let match_seq = RwSignal::new(0u32);
    let match_running = RwSignal::new(false);
    let match_status = RwSignal::new(String::new());
    let match_moves = RwSignal::new(Vec::<String>::new());
    let match_result = RwSignal::new(String::new());
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
                    status.set(i18n::tf(&language.get(), "status.d01", &[("name", c.name.clone())]));
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
                    status.set(i18n::tf(&language.get(), "status.d02", &[("name", name.clone())]));
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
                    status.set(i18n::tf(&language.get(), "status.d03", &[("name", name.trim().to_string())]));
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
                    status.set(i18n::t(&language.get(), "status.m07").into());
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
            .and_then(|w| w.confirm_with_message(i18n::t(&language.get(), "db.delete_collection")).ok())
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
                    status.set(i18n::t(&language.get(), "status.m06").into());
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
                    status.set(i18n::t(&language.get(), "status.m19").into());
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
                    status.set(i18n::t(&language.get(), "status.m00").into());
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
                settings_light.set(s.board_light);
                settings_dark.set(s.board_dark);
                settings_coords.set(s.show_coords);
                language.set(s.language);
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
            if let Ok(list) = api::piece_sets().await {
                custom_sets.set(list);
            }
        });
    });

    Effect::new(move |_| {
        if tab.get() == Tab::Home {
            spawn_local(async move {
                if let Ok((list, _)) = api::list_games(&api::Filters::default()).await {
                    home_recent.set(list.into_iter().take(6).collect());
                }
                if let Ok(list) = api::accounts().await {
                    accounts.set(list);
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
                if ws.kind == "analysis" {
                    ws.ply = ply;
                    ws.pgn = pgn;
                }
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
                        kind: "analysis".into(),
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
        if ws.kind != "analysis" {
            tab.set(match ws.kind.as_str() {
                "database" => Tab::Database,
                "openings" => Tab::Openings,
                "puzzles" => Tab::Puzzles,
                _ => Tab::Analysis,
            });
            return;
        }
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
                kind: "analysis".into(),
                game_id: None,
                title: i18n::t(&language.get(), "practice.new_analysis_tab").into(),
                pgn: start.clone(),
                ply: 0,
                variant: "standard".into(),
                preview_only: false,
                moves: Vec::new(),
            })
        });
        active_ws.set(key);
        selected_pgn.set(start);
        selected_title.set(i18n::t(&language.get(), "practice.new_analysis_tab").into());
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

    let open_page_tab = move |kind: String| {
        save_workspace();
        let existing = workspaces.with_untracked(|list| {
            list.iter().find(|w| w.kind == kind).map(|w| w.key)
        });
        let key = match existing {
            Some(key) => key,
            None => {
                let key = ws_seq.get_untracked();
                ws_seq.set(key + 1);
                let title = match kind.as_str() {
                    "database" => i18n::t(&language.get(), "db.database"),
                    "openings" => i18n::t(&language.get(), "nav.openings"),
                    "puzzles" => i18n::t(&language.get(), "nav.puzzles"),
                    _ => i18n::t(&language.get(), "nav.analysis"),
                };
                workspaces.update(|list| {
                    list.push(Workspace {
                        key,
                        kind: kind.clone(),
                        game_id: None,
                        title: title.into(),
                        pgn: String::new(),
                        ply: 0,
                        variant: "standard".into(),
                        preview_only: false,
                        moves: Vec::new(),
                    })
                });
                key
            }
        };
        active_ws.set(key);
        tab.set(match kind.as_str() {
            "database" => Tab::Database,
            "openings" => Tab::Openings,
            "puzzles" => Tab::Puzzles,
            _ => Tab::Analysis,
        });
    };

    let move_workspace = move |key: u32, delta: i32| {
        workspaces.update(|list| {
            let Some(index) = list.iter().position(|w| w.key == key) else {
                return;
            };
            let target = index as i32 + delta;
            if target >= 0 && (target as usize) < list.len() {
                list.swap(index, target as usize);
            }
        });
    };

    let commit_rename = move || {
        let Some(key) = renaming_ws.get_untracked() else {
            return;
        };
        let text = rename_text.get_untracked().trim().to_owned();
        if !text.is_empty() {
            workspaces.update(|list| {
                if let Some(ws) = list.iter_mut().find(|w| w.key == key) {
                    ws.title = text.clone();
                }
            });
            if active_ws.get_untracked() == key
                && workspaces
                    .with_untracked(|list| list.iter().find(|w| w.key == key).map(|w| w.kind.clone()))
                    .as_deref()
                    == Some("analysis")
            {
                selected_title.set(text);
            }
        }
        renaming_ws.set(None);
    };

    let switch_tab = move |next: Tab| {
        if tab.get_untracked() == Tab::Analysis && next != Tab::Analysis {
            save_workspace();
        }
        let kind = match next {
            Tab::Database => "database",
            Tab::Openings => "openings",
            Tab::Puzzles => "puzzles",
            Tab::Analysis => "",
            _ => "",
        };
        if !kind.is_empty() {
            if let Some(key) = workspaces.with_untracked(|list| {
                list.iter().find(|w| w.kind == kind).map(|w| w.key)
            }) {
                active_ws.set(key);
            }
            tab.set(next);
            return;
        }
        if next == Tab::Analysis {
            let active_kind = workspaces.with_untracked(|list| {
                list.iter()
                    .find(|w| w.key == active_ws.get_untracked())
                    .map(|w| w.kind.clone())
            });
            if active_kind.as_deref() != Some("analysis") {
                if let Some(key) = workspaces.with_untracked(|list| {
                    list.iter().rev().find(|w| w.kind == "analysis").map(|w| w.key)
                }) {
                    activate_workspace(key);
                    return;
                }
            }
            if tree_ply.get_untracked() > 0 {
                goto_sig.set(Some((goto_seq.get_untracked() + 1, tree_ply.get_untracked())));
            }
        }
        tab.set(next);
    };

    // Persist the workspace session (tabs, active tab, current ply) and restore
    // it on the next load, so reopening the app returns to the same boards.
    let client = client_id();
    let client_for_restore = client.clone();
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
        let raw = serde_json::to_string(&session).unwrap_or_default();
        let client = client.clone();
        spawn_local(async move {
            TimeoutFuture::new(350).await;
            write_session(&session);
            let _ = api::save_session(&client, &raw).await;
        });
    });

    let apply_session = move |session: Session| {
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
        let restored_tab = tab_from_name(&session.tab);
        if restored_tab == Some(Tab::Analysis) {
            activate_workspace(active);
        } else {
            let page_kind = match restored_tab {
                Some(Tab::Database) => Some("database"),
                Some(Tab::Openings) => Some("openings"),
                Some(Tab::Puzzles) => Some("puzzles"),
                _ => None,
            };
            if let Some(kind) = page_kind {
                if let Some(key) = session
                    .workspaces
                    .iter()
                    .find(|w| w.kind == kind)
                    .map(|w| w.key)
                {
                    active_ws.set(key);
                } else {
                    active_ws.set(active);
                }
            } else {
                active_ws.set(active);
            }
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
            if let Some(t) = restored_tab {
                tab.set(t);
            }
        }
    };

    Effect::new(move |_| {
        if session_restored.get_untracked() {
            return;
        }
        session_restored.set(true);
        if let Some(session) = read_session() {
            apply_session(session);
            return;
        }
        let client = client_for_restore.clone();
        spawn_local(async move {
            if let Ok(Some(raw)) = api::load_session(&client).await {
                if let Ok(session) = serde_json::from_str::<Session>(&raw) {
                    apply_session(session);
                }
            }
        });
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
            status.set(i18n::tf(
                &language.get(),
                "status.d04",
                &[("variant", g.variant.clone().unwrap_or_default())],
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
                    status.set(i18n::tf(&language.get(), "status.d05", &[("n", n.to_string())]));
                    import_text.set(String::new());
                    reload();
                }
                Err(e) => status.set(e),
            }
        });
    };

    let import_file = move |name: String, text: String| {
        if text.trim().is_empty() {
            status.set(i18n::tf(&language.get(), "status.d06", &[("name", name.clone())]));
            return;
        }
        spawn_local(async move {
            match api::import_pgn(&text).await {
                Ok(n) => {
                    status.set(i18n::tf(&language.get(), "status.d07", &[("n", n.to_string()), ("name", name.clone())]));
                    recent_files.update(|list| {
                        list.retain(|(existing, _)| existing != &name);
                        list.insert(0, (name.clone(), n));
                        list.truncate(8);
                    });
                    write_recent_files(&recent_files.get_untracked());
                    reload();
                }
                Err(error) => status.set(i18n::tf(&language.get(), "status.d08", &[("name", name.clone()), ("error", error.to_string())])),
            }
        });
    };

    let read_file = move |file: web_sys::File| {
        let name = file.name();
        let Ok(reader) = web_sys::FileReader::new() else {
            return;
        };
        let reader_for_load = reader.clone();
        let onload = Closure::<dyn FnMut()>::new(move || {
            let Ok(result) = reader_for_load.result() else {
                return;
            };
            if let Some(text) = result.as_string() {
                import_file(name.clone(), text);
            }
        });
        reader.set_onload(Some(onload.as_ref().unchecked_ref()));
        let _ = reader.read_as_text(&file);
        onload.forget();
    };

    let on_file_pick = move |ev: leptos::ev::Event| {
        let Some(input) = ev
            .current_target()
            .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
        else {
            return;
        };
        if let Some(files) = input.files() {
            for index in 0..files.length() {
                if let Some(file) = files.get(index) {
                    read_file(file);
                }
            }
        }
        input.set_value("");
    };

    // Drop a PGN anywhere on the window to import it.
    {
        let prevent = Closure::<dyn FnMut(web_sys::DragEvent)>::new(
            move |event: web_sys::DragEvent| event.prevent_default(),
        );
        let on_drop = Closure::<dyn FnMut(web_sys::DragEvent)>::new(
            move |event: web_sys::DragEvent| {
                event.prevent_default();
                let Some(data) = event.data_transfer() else {
                    return;
                };
                let Some(files) = data.files() else {
                    return;
                };
                for index in 0..files.length() {
                    if let Some(file) = files.get(index) {
                        read_file(file);
                    }
                }
            },
        );
        if let Some(window) = web_sys::window() {
            let _ = window.add_event_listener_with_callback(
                "dragover",
                prevent.as_ref().unchecked_ref(),
            );
            let _ = window
                .add_event_listener_with_callback("drop", on_drop.as_ref().unchecked_ref());
        }
        prevent.forget();
        on_drop.forget();
    }

    let import_online = move |_: leptos::ev::MouseEvent| {
        let user = online_user.get();
        if user.trim().is_empty() {
            return;
        }
        let provider = online_provider.get_untracked();
        let max = online_max.get_untracked();
        status.set(i18n::t(&language.get(), "status.m11").into());
        spawn_local(async move {
            let result = match provider.as_str() {
                "chesscom" => api::import_chesscom(user.trim(), max).await,
                _ => api::import_lichess(user.trim(), max).await,
            };
            match result {
                Ok((n, skipped)) => {
                    status.set(if skipped > 0 {
                        i18n::tf(
                            &language.get(),
                            "status.d09",
                            &[
                                ("n", n.to_string()),
                                ("provider", provider.clone()),
                                ("skipped", skipped.to_string()),
                            ],
                        )
                    } else {
                        i18n::tf(
                            &language.get(),
                            "status.d10",
                            &[("n", n.to_string()), ("provider", provider.clone())],
                        )
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
                    status.set(i18n::t(&language.get(), "status.m28").into());
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
            status.set(i18n::t(&language.get(), "status.m03").into());
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
            status.set(i18n::t(&language.get(), "status.m25").into());
            return;
        }
        let pgn = line_pgn(&detail.fen, &line, detail.variant.as_deref().unwrap_or("standard"));
        spawn_local(async move {
            match api::import_repertoire_pgn(repertoire, &pgn).await {
                Ok(n) => {
                    status.set(i18n::tf(&language.get(), "status.d11", &[("n", n.to_string())]));
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
                    status.set(i18n::t(&language.get(), "status.m29").into());
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
            status.set(i18n::t(&language.get(), "status.m24").into());
            return;
        }
        spawn_local(async move {
            match api::delete_tree_node(id, &path).await {
                Ok(d) => {
                    adopt_detail(d);
                    status.set(i18n::t(&language.get(), "status.m12").into());
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
            status.set(i18n::t(&language.get(), "status.m26").into());
            return;
        }
        spawn_local(async move {
            match api::promote_variation(id, &path).await {
                Ok(d) => {
                    adopt_detail(d);
                    status.set(i18n::t(&language.get(), "status.m31").into());
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
                    status.set(i18n::tf(&language.get(), "status.d12", &[("ply", ply.to_string())]));
                    reload();
                }
                Err(e) => status.set(e),
            }
        });
    };

    let add_game_to_repertoire = move |_: leptos::ev::MouseEvent| {
        let Some(repertoire) = Some(analysis_rep.get()).filter(|id| *id > 0) else {
            status.set(i18n::t(&language.get(), "status.m03").into());
            return;
        };
        let Some(game) = selected_game_id.get() else {
            return;
        };
        spawn_local(async move {
            match api::import_repertoire_game(repertoire, game).await {
                Ok(n) => {
                    status.set(i18n::tf(&language.get(), "status.d13", &[("n", n.to_string())]));
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
            .and_then(|w| w.confirm_with_message(i18n::t(&language.get(), "db.delete_game")).ok())
            .unwrap_or(false);
        if !confirmed {
            return;
        }
        spawn_local(async move {
            match api::delete_game(id).await {
                Ok(()) => {
                    status.set(i18n::t(&language.get(), "status.m10").into());
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
                    status.set(i18n::t(&language.get(), "status.m01").into());
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
            status.set(i18n::t(&language.get(), "status.m30").into());
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
                    {move || i18n::t(&language.get(), "common.start")}
                </button>
                {rows}
            </div>
        }
        .into_any()
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
            board_light: settings_light.get_untracked(),
            board_dark: settings_dark.get_untracked(),
            show_coords: settings_coords.get_untracked(),
            language: language.get_untracked(),
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
                    settings_status.set(i18n::t(&language.get(), "status.m23").into());
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

    let upload_piece_set = move |ev: leptos::ev::Event| {
        let Some(input) = ev
            .current_target()
            .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
        else {
            return;
        };
        let name = piece_set_name.get_untracked().trim().to_lowercase();
        if name.is_empty() {
            piece_set_msg.set(i18n::t(&language.get(), "status.d36").into());
            return;
        }
        let Some(files) = input.files() else {
            return;
        };
        let total = files.length();
        if total == 0 {
            return;
        }
        let collected = std::rc::Rc::new(std::cell::RefCell::new(
            std::collections::BTreeMap::<String, String>::new(),
        ));
        let pending = std::rc::Rc::new(std::cell::RefCell::new(total));
        piece_upload_busy.set(true);
        piece_set_msg.set(i18n::tf(&language.get(), "status.d37", &[("total", total.to_string())]));
        for index in 0..total {
            let Some(file) = files.get(index) else {
                continue;
            };
            let filename = file.name();
            let stem = filename
                .rsplit_once('.')
                .map(|(stem, _)| stem)
                .unwrap_or(&filename)
                .to_owned();
            let code = match stem.as_str() {
                "wK" => "K",
                "wQ" => "Q",
                "wR" => "R",
                "wB" => "B",
                "wN" => "N",
                "wP" => "P",
                "bK" => "k",
                "bQ" => "q",
                "bR" => "r",
                "bB" => "b",
                "bN" => "n",
                "bP" => "p",
                other => other,
            }
            .to_owned();
            let Ok(reader) = web_sys::FileReader::new() else {
                continue;
            };
            let reader_for_load = reader.clone();
            let collected = collected.clone();
            let pending = pending.clone();
            let name = name.clone();
            let code = code.clone();
            let onload = Closure::<dyn FnMut()>::new(move || {
                if let Ok(result) = reader_for_load.result() {
                    if let Some(text) = result.as_string() {
                        collected.borrow_mut().insert(code.clone(), text);
                    }
                }
                let ready = {
                    let mut left = pending.borrow_mut();
                    *left = left.saturating_sub(1);
                    *left == 0
                };
                if !ready {
                    return;
                }
                let files = collected.borrow().clone();
                let missing: Vec<&str> = ["K", "Q", "R", "B", "N", "P", "k", "q", "r", "b", "n", "p"]
                    .into_iter()
                    .filter(|code| !files.contains_key(*code))
                    .collect();
                if !missing.is_empty() {
                    piece_set_msg.set(i18n::tf(&language.get(), "status.d38", &[("list", missing.join(", "))]));
                    piece_upload_busy.set(false);
                    return;
                }
                let upload_name = name.clone();
                spawn_local(async move {
                    match api::upload_piece_set(&upload_name, &files).await {
                        Ok(list) => {
                            custom_sets.set(list);
                            settings_pieces.set(format!("custom:{upload_name}"));
                            persist_settings();
                            piece_set_msg.set(i18n::tf(&language.get(), "status.d31", &[("name", upload_name.clone())]));
                        }
                        Err(error) => piece_set_msg.set(error),
                    }
                    piece_upload_busy.set(false);
                });
            });
            reader.set_onload(Some(onload.as_ref().unchecked_ref()));
            let _ = reader.read_as_text(&file);
            onload.forget();
        }
        input.set_value("");
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
        views::stats::stats_view(views::stats::StatsCtx {
            language,
            stats,
            db_list,
        })
    };

    let download_vision = move |_| {
        vision_busy.set(true);
        vision_msg.set(i18n::t(&language.get(), "status.d32").into());
        spawn_local(async move {
            match api::download_vision().await {
                Ok(status) => {
                    vision.set(Some(status));
                    vision_msg.set(i18n::t(&language.get(), "status.d33").into());
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
            vision_msg.set(i18n::t(&language.get(), "status.d34").into());
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
                        vision_msg.set(i18n::tf(
                            &language.get(),
                            "status.d35",
                            &[
                                ("n", found.detections.to_string()),
                                ("pct", format!("{:.0}", found.confidence * 100.0)),
                            ],
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
                        sound_msg.set(i18n::tf(&language.get(), "status.d31", &[("name", stem.clone())]));
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
                        i18n::t(&language.get(), "clock.black_flag").to_owned()
                    } else {
                        i18n::t(&language.get(), "clock.white_flag").to_owned()
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
        tb_status.set(i18n::t(&language.get(), "status.m20").into());
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

    let link_account = move |provider: &'static str| {
        let (username, token) = match provider {
            "lichess" => (
                lichess_user_input.get_untracked(),
                Some(lichess_token_input.get_untracked()),
            ),
            _ => (chesscom_user_input.get_untracked(), None),
        };
        if username.trim().is_empty() {
            account_msg.set(i18n::t(&language.get(), "account.enter_user").into());
            return;
        }
        account_busy.set(true);
        account_msg.set(i18n::tf(&language.get(), "status.d19", &[("provider", provider.to_string())]));
        spawn_local(async move {
            match api::link_account(provider, username.trim(), token.as_deref()).await {
                Ok(list) => {
                    accounts.set(list);
                    account_msg.set(i18n::tf(&language.get(), "status.d20", &[("provider", provider.to_string())]));
                    lichess_user_input.set(String::new());
                    lichess_token_input.set(String::new());
                    chesscom_user_input.set(String::new());
                }
                Err(error) => account_msg.set(error),
            }
            account_busy.set(false);
        });
    };

    let unlink_account = move |provider: &'static str| {
        account_busy.set(true);
        spawn_local(async move {
            match api::unlink_account(provider).await {
                Ok(list) => {
                    accounts.set(list);
                    account_msg.set(i18n::tf(&language.get(), "status.d21", &[("provider", provider.to_string())]));
                }
                Err(error) => account_msg.set(error),
            }
            account_busy.set(false);
        });
    };

    let import_account = move |provider: &'static str| {
        let max = account_import_max.get_untracked().clamp(1, 500);
        account_busy.set(true);
        account_msg.set(i18n::tf(&language.get(), "status.d22", &[("provider", provider.to_string())]));
        spawn_local(async move {
            match api::import_account(provider, max).await {
                Ok(count) => {
                    account_msg.set(i18n::tf(&language.get(), "status.d23", &[("count", count.to_string()), ("provider", provider.to_string())]));
                    reload();
                }
                Err(error) => account_msg.set(error),
            }
            account_busy.set(false);
        });
    };

    let fetch_cloud = move |_: leptos::ev::MouseEvent| {
        let position = fen.get_untracked();
        if position.trim().is_empty() {
            status.set(i18n::t(&language.get(), "status.m14").into());
            return;
        }
        status.set(i18n::t(&language.get(), "status.m02").into());
        spawn_local(async move {
            match api::cloud_eval(&position).await {
                Ok(result) => {
                    if result.available {
                        status.set(i18n::tf(
                            &language.get(),
                            "status.d14",
                            &[("depth", result.depth.unwrap_or(0).to_string())],
                        ));
                    } else {
                        status.set(i18n::t(&language.get(), "status.m13").into());
                    }
                    cloud.set(Some(result));
                }
                Err(error) => status.set(error),
            }
        });
    };

    let export_report = move |_: leptos::ev::MouseEvent| {
        let Some(report) = report.get_untracked() else {
            status.set(i18n::t(&language.get(), "status.m05").into());
            return;
        };
        let title = selected_title.get_untracked();
        let mut text = format!("# {title}

");
        if let (Some(white), Some(black)) = (report.white_accuracy, report.black_accuracy) {
            text.push_str(&format!(
                "Accuracy: White {:.1}% · Black {:.1}%

",
                white * 100.0,
                black * 100.0
            ));
        }
        text.push_str("| Ply | Move | Label | Accuracy |\n|---|---|---|---|\n");
        for m in &report.moves {
            text.push_str(&format!(
                "| {} | {} | {} | {:.0}% |\n",
                m.ply,
                m.san,
                m.lichess,
                m.accuracy * 100.0
            ));
        }
        let name: String = title
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        download_text(&format!("{name}-report.md"), &text);
        status.set(i18n::t(&language.get(), "status.m22").into());
    };

    let annotate_game = move |_: leptos::ev::MouseEvent| {
        let Some(game_id) = selected_game_id.get_untracked() else {
            status.set(i18n::t(&language.get(), "status.m16").into());
            return;
        };
        let Some(report) = report.get_untracked() else {
            status.set(i18n::t(&language.get(), "status.m05").into());
            return;
        };
        annotating.set(true);
        status.set(i18n::t(&language.get(), "status.m32").into());
        spawn_local(async move {
            let mut written = 0usize;
            for m in &report.moves {
                let nag = match m.lichess.as_str() {
                    "brilliant" => 3,
                    "great" => 1,
                    "inaccuracy" => 5,
                    "mistake" => 2,
                    "blunder" => 4,
                    _ => 0,
                };
                if nag == 0 {
                    continue;
                }
                let comment = format!("{} · {:.0}% accuracy", m.lichess, m.accuracy * 100.0);
                if api::add_annotation(
                    game_id,
                    m.ply as i64,
                    Some(m.ply.to_string()),
                    Some(comment),
                    Some(nag),
                    None,
                )
                .await
                .is_ok()
                {
                    written += 1;
                }
            }
            if let Ok(list) = api::annotations(game_id).await {
                annotations.set(list);
            }
            status.set(i18n::tf(&language.get(), "status.d15", &[("written", written.to_string())]));
            annotating.set(false);
        });
    };



    let normalize_san = |san: &str| -> String {
        san.chars()
            .filter(|c| !matches!(c, '+' | '#' | '!' | '?'))
            .collect::<String>()
            .to_lowercase()
    };

    let start_guess = move |id: i64| {
        if id <= 0 {
            practice_feedback.set(i18n::t(&language.get(), "practice.choose_game_first").into());
            return;
        }
        spawn_local(async move {
            match api::get_game(id).await {
                Ok(detail) => {
                    if detail.moves.trim().is_empty() {
                        practice_feedback.set(i18n::t(&language.get(), "practice.no_moves").into());
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
                    practice_feedback.set(i18n::tf(
                        &language.get(),
                        "status.d40",
                        &[("n", sans.len().to_string())],
                    ));
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
                practice_feedback.set(i18n::tf(
                    &language.get(),
                    "status.d27",
                    &[("hits", hits.to_string()), ("total", sans.len().to_string())],
                ));
            } else {
                practice_feedback.set(i18n::tf(
                    &language.get(),
                    "status.d28",
                    &[("remaining", (sans.len() - step - 1).to_string())],
                ));
            }
        } else {
            practice_misses.update(|m| *m += 1);
            practice_feedback.set(i18n::tf(
                &language.get(),
                "status.d41",
                &[("mv", sans[step].clone())],
            ));
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
        let color = if spar_color.get_untracked() == "black" {
            i18n::t(&language.get(), "common.black")
        } else {
            i18n::t(&language.get(), "common.white")
        };
        spar_status.set(i18n::tf(
            &language.get(),
            "status.d30",
            &[("color", color.to_string())],
        ));
    };

    let on_spar_move = move |(_uci, _san, fen_after): (String, String, String)| {
        let engine_white = spar_color.get_untracked() == "black";
        let turn_white = fen_after.split_whitespace().nth(1) != Some("b");
        spar_status.set(i18n::t(&language.get(), "analysis.engine_thinking_caps").into());
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
            spar_status.set(i18n::t(&language.get(), "practice.your_move").into());
        }
    };

    let request_match_move = move || {

        if !match_running.get_untracked() {
            return;
        }
        let board_fen = fen.get_untracked();
        let turn_black = board_fen.split_whitespace().nth(1) == Some("b");
        let list = engines.get_untracked();
        let index = if turn_black {
            match_black.get_untracked()
        } else {
            match_white.get_untracked()
        };
        let Some(entry) = list.get(index).cloned() else {
            match_status.set(i18n::t(&language.get(), "status.m04").into());
            match_running.set(false);
            return;
        };
        let depth = match_depth.get_untracked().clamp(1, 30);

        match_status.set(i18n::tf(&language.get(), "status.d24", &[("name", entry.name.clone())]));
        spawn_local(async move {
            match api::analyze_engine(&board_fen, depth, &entry.path).await {
                Ok(mv) => {
                    if let Some(over) = mv.game_over {
                        let white_to_move = board_fen.split_whitespace().nth(1) != Some("b");
                        let result = if over == "checkmate" {
                            if white_to_move { "0-1" } else { "1-0" }
                        } else {
                            "1/2-1/2"
                        };
                        match_result.set(result.to_owned());
                        match_status.set(i18n::tf(&language.get(), "status.d25", &[("result", result.to_string()), ("over", over.clone())]));
                        match_running.set(false);
                        return;
                    }
                    if mv.best_move.is_empty() || mv.best_move == "(none)" {
                        match_result.set("1/2-1/2".into());
                        match_status.set(i18n::t(&language.get(), "status.m09").into());
                        match_running.set(false);
                        return;
                    }
                    let seq = match_seq.get_untracked() + 1;
                    match_seq.set(seq);
                    match_play.set(Some((seq, mv.best_move)));
                }
                Err(error) => {
                    match_status.set(error);
                    match_running.set(false);
                }
            }
        });
    };

    let on_match_play = move |(_uci, san, fen_after): (String, String, String)| {

        match_moves.update(|list| list.push(san));
        if !match_running.get_untracked() {
            return;
        }
        let turn = if fen_after.split_whitespace().nth(1) == Some("b") {
            i18n::t(&language.get(), "common.black")
        } else {
            i18n::t(&language.get(), "common.white")
        };
        match_status.set(i18n::tf(
            &language.get(),
            "status.d26",
            &[("turn", turn.to_string())],
        ));
        spawn_local(async move {
            TimeoutFuture::new(150).await;
            request_match_move();
        });
    };

    let start_match = move |_: leptos::ev::MouseEvent| {
        let entered = match_start_fen.get_untracked();
        let start = if entered.trim().is_empty() {
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".to_owned()
        } else {
            entered
        };
        match_pgn.set(line_pgn(&start, &[], "standard"));
        match_moves.set(Vec::new());
        match_result.set(String::new());
        match_status.set(i18n::t(&language.get(), "status.m27").into());
        match_play.set(None);
        let seq = match_seq.get_untracked() + 1;
        match_seq.set(seq);
        match_running.set(true);
        let expected = start.clone();
        spawn_local(async move {
            for _ in 0..20 {
                TimeoutFuture::new(100).await;
                if fen.get_untracked() == expected {
                    break;
                }
            }
            request_match_move();
        });
    };

    let pause_match = move |_: leptos::ev::MouseEvent| {
        match_running.set(false);
        if match_result.get_untracked().is_empty() {
            match_status.set(i18n::t(&language.get(), "status.m17").into());
        }
    };

    let reset_match = move |_: leptos::ev::MouseEvent| {
        match_running.set(false);
        match_play.set(None);
        match_moves.set(Vec::new());
        match_result.set(String::new());
        match_status.set(String::new());
        let seq = match_seq.get_untracked() + 1;
        match_seq.set(seq);
        let start = if match_start_fen.get_untracked().trim().is_empty() {
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".to_owned()
        } else {
            match_start_fen.get_untracked()
        };
        match_pgn.set(line_pgn(&start, &[], "standard"));
    };




    let app_ctx = views::AppCtx {
        account_busy,
        account_import_max,
        account_msg,
        accounts,
        active_collection,
        add_collection,
        ai_shapes,
        analysis,
        analysis_rep,
        analyzing,
        animate_last_move,
        ann_nag,
        ann_text,
        annotating,
        attempts,
        chat_messages,
        chat_text,
        chat_tools,
        chesscom_user_input,
        clock,
        clock_active,
        clock_base_input,
        clock_epoch,
        clock_inc,
        clock_inc_input,
        cloud,
        collection_name,
        collections,
        console_busy,
        console_cmd,
        console_log,
        console_name,
        console_path,
        console_running,
        custom_sets,
        db_list,
        dock,
        edit_moves_open,
        editor_castling,
        editor_ep,
        editor_open,
        editor_palette,
        editor_pieces,
        editor_turn,
        editor_variant,
        engine_dialog,
        engine_results,
        engine_test,
        engine_test_busy,
        engines,
        eval_hover,
        evals,
        event_filter,
        events,
        fen,
        filter_date_from,
        filter_date_to,
        filter_elo_max,
        filter_elo_min,
        filter_result,
        filter_variant,
        game_detail,
        games,
        goto_sig,
        home_recent,
        import_text,
        keybinds,
        language,
        lichess_token_input,
        lichess_user_input,
        lines,
        load_fen_sig,
        manage_collection,
        match_black,
        match_depth,
        match_moves,
        match_pgn,
        match_play,
        match_result,
        match_running,
        match_start_fen,
        match_status,
        match_white,
        moves_text,
        new_collection,
        new_database,
        new_engine_name,
        new_engine_path,
        online_max,
        online_provider,
        online_user,
        opening,
        opening_fen,
        opening_mode,
        opening_source,
        partial_search,
        piece_animation_ms,
        piece_set_msg,
        piece_set_name,
        piece_upload_busy,
        play_as,
        play_uci,
        players,
        position_fen,
        position_variant,
        practice_feedback,
        practice_game,
        practice_games,
        practice_goto,
        practice_hits,
        practice_misses,
        practice_mode,
        practice_pgn,
        practice_pick,
        practice_query,
        practice_sans,
        practice_seq,
        practice_step,
        profile,
        profile_name,
        profile_player,
        puzzle,
        puzzle_csv,
        puzzle_delta,
        puzzle_orientation,
        puzzle_pgn,
        puzzle_rating,
        puzzle_recorded,
        puzzle_san,
        puzzle_solution,
        puzzle_state,
        puzzle_step,
        puzzle_theme,
        puzzle_themes,
        query,
        recent_files,
        record_moves,
        rep_board,
        rep_card,
        rep_color,
        rep_correct,
        rep_feedback,
        rep_id,
        rep_name,
        rep_pgn_input,
        rep_selected,
        report,
        reporting,
        reps,
        selected_game_id,
        selected_pgn,
        selected_title,
        settings_coords,
        settings_dark,
        settings_depth,
        settings_engine,
        settings_hash,
        settings_light,
        settings_model,
        settings_pieces,
        settings_status,
        settings_syzygy,
        settings_theme,
        settings_threads,
        settings_variant_engine,
        shapes,
        show_solution,
        show_tags,
        sort_by,
        sound_enabled,
        sound_msg,
        sound_theme,
        sound_volume,
        spar_color,
        spar_fen,
        spar_pgn,
        spar_play,
        spar_status,
        spar_thinking,
        stats,
        status,
        tab,
        tag_black,
        tag_black_elo,
        tag_date,
        tag_event,
        tag_result,
        tag_round,
        tag_site,
        tag_time_control,
        tag_white,
        tag_white_elo,
        tb_result,
        tb_status,
        thinking,
        total,
        tree_ply,
        variant_only,
        vision,
        vision_busy,
        vision_msg,
        vs_engine,
        workspaces,
        active_ws,
        renaming_ws,
        rename_text,
        new_tab_kind,
        activate_workspace: views::app_ctx::view_callback(activate_workspace),
        close_workspace: views::app_ctx::view_callback(close_workspace),
        commit_rename: views::app_ctx::view_callback(move |_: ()| commit_rename()),
        move_workspace: views::app_ctx::view_callback(move |(a, b): (u32, i32)| move_workspace(a, b)),
        open_page_tab: views::app_ctx::view_callback(open_page_tab),
        add_collection_clicked: views::app_ctx::view_callback(add_collection_clicked),
        add_engine: views::app_ctx::view_callback(add_engine),
        add_game_to_collection: views::app_ctx::view_callback(add_game_to_collection),
        add_game_to_repertoire: views::app_ctx::view_callback(add_game_to_repertoire),
        add_line_to_repertoire: views::app_ctx::view_callback(add_line_to_repertoire),
        annotate_game: views::app_ctx::view_callback(annotate_game),
        apply_editor: views::app_ctx::view_callback(apply_editor),
        apply_moves: views::app_ctx::view_callback(apply_moves),
        choose_side: views::app_ctx::view_callback(choose_side),
        classify: views::app_ctx::view_callback(classify),
        console_send: views::app_ctx::view_callback(move |_: ()| console_send()),
        console_start: views::app_ctx::view_callback(console_start),
        console_stop: views::app_ctx::view_callback(console_stop),
        create_db: views::app_ctx::view_callback(create_db),
        delete_collection: views::app_ctx::view_callback(delete_collection),
        delete_current: views::app_ctx::view_callback(delete_current),
        delete_tree_from_here: views::app_ctx::view_callback(delete_tree_from_here),
        download_vision: views::app_ctx::view_callback(download_vision),
        export_report: views::app_ctx::view_callback(export_report),
        fetch_cloud: views::app_ctx::view_callback(fetch_cloud),
        fetch_engine_lines: views::app_ctx::view_callback(fetch_engine_lines),
        fetch_lines: views::app_ctx::view_callback(fetch_lines),
        import: views::app_ctx::view_callback(import),
        import_account: views::app_ctx::view_callback(import_account),
        import_online: views::app_ctx::view_callback(import_online),
        link_account: views::app_ctx::view_callback(link_account),
        new_workspace: views::app_ctx::view_callback(move |_: ()| new_workspace()),
        on_board_move: views::app_ctx::view_callback(move |args: (String, String, String)| on_board_move(args)),
        on_engine_play: views::app_ctx::view_callback(move |args: (String, String, String)| on_engine_play(args)),
        on_file_pick: views::app_ctx::view_callback(on_file_pick),
        on_match_play: views::app_ctx::view_callback(move |args: (String, String, String)| on_match_play(args)),
        on_practice_move: views::app_ctx::view_callback(move |args: (String, String, String)| on_practice_move(args)),
        on_spar_move: views::app_ctx::view_callback(move |args: (String, String, String)| on_spar_move(args)),
        open_editor: views::app_ctx::view_callback(open_editor),
        open_game: views::app_ctx::view_callback(open_game),
        pause_match: views::app_ctx::view_callback(pause_match),
        persist_settings: views::app_ctx::view_callback(move |_: ()| persist_settings()),
        probe_tablebase: views::app_ctx::view_callback(probe_tablebase),
        promote_selected: views::app_ctx::view_callback(promote_selected),
        reload: views::app_ctx::view_callback(move |_: ()| reload()),
        rename_collection: views::app_ctx::view_callback(rename_collection),
        reset_match: views::app_ctx::view_callback(reset_match),
        run_analysis: views::app_ctx::view_callback(run_analysis),
        run_engine_test: views::app_ctx::view_callback(run_engine_test),
        save_annotation: views::app_ctx::view_callback(save_annotation),
        save_tags: views::app_ctx::view_callback(save_tags),
        scan_image: views::app_ctx::view_callback(scan_image),
        search_position: views::app_ctx::view_callback(search_position),
        select_node: views::app_ctx::view_callback(select_node),
        send_chat: views::app_ctx::view_callback(move |_: ()| send_chat()),
        set_engine_option: views::app_ctx::view_callback(move |(a, b, c): (usize, &'static str, String)| set_engine_option(a, b, c)),
        start_guess: views::app_ctx::view_callback(start_guess),
        start_match: views::app_ctx::view_callback(start_match),
        start_spar: views::app_ctx::view_callback(start_spar),
        switch_db: views::app_ctx::view_callback(switch_db),
        switch_tab: views::app_ctx::view_callback(switch_tab),
        toggle_vs_engine: views::app_ctx::view_callback(toggle_vs_engine),
        tree_nodes: views::app_ctx::view_callback(move |_: ()| tree_nodes()),
        truncate_moves: views::app_ctx::view_callback(truncate_moves),
        unlink_account: views::app_ctx::view_callback(unlink_account),
        upload_piece_set: views::app_ctx::view_callback(upload_piece_set),
        upload_sound: views::app_ctx::view_callback(upload_sound),
    };
    let home_view = move || views::home::home(app_ctx);

    let database_view = move || views::database::database(app_ctx);

    let analysis_view = move || views::analysis::analysis(app_ctx);

    let practice_view = move || views::practice::practice(app_ctx);

    let opening_view = move || views::opening::opening(app_ctx);

    let puzzle_view = move || views::puzzle::puzzle(app_ctx);

    let settings_view = move || views::settings::settings(app_ctx);

    view! {
        <a class="skip-link" href="#app-main">{move || i18n::t(&language.get(), "a11y.skip")}</a>
        <div class="app">
            <aside class="sidebar">
                <div class="brand">"RustyBrickChess"</div>
                <nav>
                    {[Tab::Home, Tab::Database, Tab::Analysis, Tab::Practice, Tab::Openings, Tab::Puzzles, Tab::Stats, Tab::Settings]
                        .into_iter()
                        .map(|t| {
                            let key = match t {
                                Tab::Home => "nav.home",
                                Tab::Database => "nav.database",
                                Tab::Analysis => "nav.analysis",
                                Tab::Practice => "nav.practice",
                                Tab::Openings => "nav.openings",
                                Tab::Puzzles => "nav.puzzles",
                                Tab::Stats => "nav.stats",
                                Tab::Settings => "nav.settings",
                            };
                            view! {
                                <button
                                    class=move || if tab.get() == t { "nav active" } else { "nav" }
                                    on:click=move |_| switch_tab(t)
                                >
                                    {move || i18n::t(&language.get(), key)}
                                </button>
                            }
                        })
                        .collect_view()}
                </nav>
                <div class="side-foot">"AGPL-3.0 · BrickChess"</div>
            </aside>
            <main class="main" id="app-main">
                <header class="topbar">
                    <input
                        placeholder=move || i18n::t(&language.get(), "top.search_player")
                        prop:value=move || query.get()
                        on:input=move |ev| query.set(event_target_value(&ev))
                        on:keydown=move |ev| {
                            if ev.key() == i18n::t(&language.get(), "common.enter") { reload(); }
                        }
                    />
                    <button on:click=move |_| reload()>
                        {move || i18n::t(&language.get(), "top.search")}
                    </button>
                    <span class="status" aria-live="polite">{move || status.get()}</span>
                </header>
                {views::workspaces::workspaces_bar(app_ctx)}
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
