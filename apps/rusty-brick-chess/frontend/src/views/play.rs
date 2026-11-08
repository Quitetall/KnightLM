//! The Play tab: matches against a model, and model-versus-model games.
//!
//! The opponent selector and active board come first. Saved games,
//! leaderboards, and tournaments sit below them.

use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;

use crate::ai_api;
use crate::api as db_api;
use crate::board::Board;
use crate::i18n;
use crate::views::AppCtx;

pub(crate) const KNIGHTLM_MODEL: &str = "knightlm-smolvlm-500m";

const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

fn seat_label(seat: &serde_json::Value) -> String {
    if seat["provider"] == "local:LAMU" && seat["model"] == KNIGHTLM_MODEL {
        return "KnightLM".into();
    }
    match seat.get("kind").and_then(|kind| kind.as_str()) {
        Some("model") => format!(
            "{} · {}",
            seat.get("provider")
                .and_then(|p| p.as_str())
                .unwrap_or("model"),
            seat.get("model").and_then(|m| m.as_str()).unwrap_or("")
        ),
        Some("human") => seat
            .get("name")
            .and_then(|name| name.as_str())
            .unwrap_or("You")
            .to_string(),
        _ => "—".to_string(),
    }
}

fn clock_text(milliseconds: Option<i64>) -> String {
    let Some(ms) = milliseconds else {
        return "—".to_string();
    };
    let total = (ms / 1000).max(0);
    format!("{:02}:{:02}", total / 60, total % 60)
}

/// The clock pair from a match payload.
fn clocks(payload: &serde_json::Value) -> (Option<i64>, Option<i64>) {
    match payload.get("clock").and_then(|clock| clock.as_array()) {
        Some(pair) if pair.len() == 2 => (pair[0].as_i64(), pair[1].as_i64()),
        _ => (None, None),
    }
}

fn state_text(payload: &serde_json::Value) -> String {
    payload
        .get("state")
        .and_then(|state| state.as_str())
        .unwrap_or("")
        .replace('_', " ")
}

fn result_text(payload: &serde_json::Value, language: &str) -> Option<String> {
    let result = payload.get("result")?;
    if result.is_null() {
        return None;
    }
    let label = i18n::t(
        language,
        match result["score"].as_str() {
            Some("1-0") => "db.white_wins",
            Some("0-1") => "db.black_wins",
            Some("1/2-1/2") => "db.draw",
            _ => "play.ended",
        },
    );
    let termination = result["termination"].as_str().unwrap_or_default();
    if termination.is_empty() {
        Some(label.to_owned())
    } else {
        Some(format!("{label} · {}", termination.replace('_', " ")))
    }
}

/// Save the PGN as a file the browser downloads.
fn download_pgn(pgn: &str) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let parts = js_sys::Array::of1(&wasm_bindgen::JsValue::from_str(pgn));
    let Ok(blob) = web_sys::Blob::new_with_str_sequence(&parts) else {
        return;
    };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else {
        return;
    };
    if let Ok(anchor) = document.create_element("a")
        && let Ok(anchor) = anchor.dyn_into::<web_sys::HtmlAnchorElement>()
    {
        anchor.set_href(&url);
        anchor.set_download("match.pgn");
        anchor.click();
    }
    let _ = web_sys::Url::revoke_object_url(&url);
}

pub fn play(ctx: AppCtx) -> impl IntoView {
    let AppCtx {
        language,
        open_game_at,
        play_match_id: selected,
        settings_model,
        ..
    } = ctx;

    let matches = RwSignal::new(Vec::<serde_json::Value>::new());
    let replay_ply = RwSignal::new(None::<usize>);
    let detail = RwSignal::new(None::<serde_json::Value>);
    let busy = RwSignal::new(false);
    let thinking = RwSignal::new(false);
    let knightlm_samples = RwSignal::new(5u8);
    let knightlm_temperature = RwSignal::new(0.6f32);
    let knightlm_vote = RwSignal::new(None::<serde_json::Value>);
    let model_status = RwSignal::new(String::new());
    let message = RwSignal::new(String::new());
    let local_models = RwSignal::new(Vec::<(String, String)>::new());
    let opponent = RwSignal::new(1usize);
    let scanning = RwSignal::new(false);
    let scan_local = move || {
        message.set(String::new());
        scanning.set(true);
        spawn_local(async move {
            match db_api::scan_local_providers().await {
                Ok(endpoints) => {
                    let models = endpoints
                        .into_iter()
                        .filter(|endpoint| endpoint.responding)
                        .flat_map(|endpoint| {
                            let label = endpoint.label;
                            endpoint
                                .models
                                .into_iter()
                                .map(move |model| (label.clone(), model))
                        })
                        .filter(|(label, model)| label != "LAMU" || model != KNIGHTLM_MODEL)
                        .collect::<Vec<_>>();
                    let selected = opponent.get_untracked();
                    let next = selected.checked_sub(2).map(|index| {
                        let previous = local_models.with_untracked(|old| old.get(index).cloned());
                        previous
                            .and_then(|seat| models.iter().position(|model| *model == seat))
                            .map_or(1, |index| index + 2)
                    });
                    local_models.set(models);
                    if let Some(next) = next {
                        opponent.set(next);
                    }
                }
                Err(error) => message.set(error),
            }
            scanning.set(false);
        });
    };
    let reload = move || {
        spawn_local(async move {
            match ai_api::list_ai_matches().await {
                Ok(list) => matches.set(list),
                Err(error) => message.set(error),
            }
        });
    };

    let load_detail = move |id: String| {
        spawn_local(async move {
            match ai_api::get_ai_match(&id).await {
                Ok(value) if selected.get_untracked().as_deref() == Some(&id) => {
                    detail.set(Some(value))
                }
                Ok(_) => {}
                Err(error) => message.set(error),
            }
        });
    };

    // Load once, and whenever the list is asked for again.
    Effect::new(move |_| scan_local());
    Effect::new(move |_| reload());
    // Follow the selection.
    Effect::new(move |_| {
        replay_ply.set(None);
        if let Some(id) = selected.get() {
            let already_loaded = detail.with_untracked(|payload| {
                payload.as_ref().and_then(|value| value["id"].as_str()) == Some(id.as_str())
            });
            if !already_loaded {
                detail.set(None);
                load_detail(id);
            }
        } else {
            detail.set(None);
        }
    });

    let lamu_model = move || match opponent.get() {
        1 => Some(KNIGHTLM_MODEL.to_owned()),
        index => index.checked_sub(2).and_then(|index| {
            local_models.with(|models| {
                models
                    .get(index)
                    .filter(|(label, _)| label == "LAMU")
                    .map(|(_, model)| model.clone())
            })
        }),
    };

    let manage_model = move |action: &'static str| {
        let Some(model) = lamu_model() else { return };
        busy.set(true);
        message.set(String::new());
        model_status.set(if action == "load" {
            format!("Loading {model}…")
        } else {
            format!("Unloading {model}…")
        });
        spawn_local(async move {
            match ai_api::manage_lamu_model(action, &model).await {
                Ok(()) if action == "load" => {
                    model_status.set(format!("{model} is loaded"));
                    scan_local();
                }
                Ok(()) => model_status.set(format!("{model} is unloaded")),
                Err(error) => {
                    model_status.set(String::new());
                    message.set(error);
                }
            }
            busy.set(false);
        });
    };

    let create_game = move |config: serde_json::Value| {
        knightlm_vote.set(None);
        busy.set(true);
        message.set(String::new());
        model_status.set(String::new());
        spawn_local(async move {
            let result = async {
                if let Some(model) = ["white", "black"].into_iter().find_map(|color| {
                    let seat = &config[color];
                    (seat["provider"] == "local:LAMU")
                        .then(|| seat["model"].as_str())
                        .flatten()
                }) {
                    model_status.set(format!("Preparing {model}…"));
                    ai_api::manage_lamu_model("load", model).await?;
                    if local_models
                        .with_untracked(|models| !models.iter().any(|(label, _)| label == "LAMU"))
                    {
                        scan_local();
                    }
                }
                ai_api::create_ai_match(config).await
            }
            .await;
            model_status.set(String::new());
            match result {
                Ok(value) => {
                    if let Some(id) = value.get("id").and_then(|id| id.as_str()) {
                        let id = id.to_owned();
                        detail.set(Some(value));
                        selected.set(Some(id));
                    }
                    reload();
                }
                Err(error) => message.set(error),
            }
            busy.set(false);
        });
    };

    let start_match = move |white_model: bool, black_model: bool| {
        let model_seat = match opponent.get_untracked() {
            0 if !settings_model.get_untracked().trim().is_empty() => {
                serde_json::json!({"kind": "model", "provider": "local", "model": "assistant"})
            }
            1 => {
                serde_json::json!({"kind": "model", "provider": "local:LAMU", "model": KNIGHTLM_MODEL})
            }
            choice => {
                let Some((label, model)) = choice.checked_sub(2).and_then(|index| {
                    local_models.with_untracked(|models| models.get(index).cloned())
                }) else {
                    return;
                };
                serde_json::json!({"kind": "model", "provider": format!("local:{label}"), "model": model})
            }
        };
        message.set(String::new());
        let config = serde_json::json!({
            "start_fen": START_FEN,
            "white": if white_model {
                model_seat.clone()
            } else {
                serde_json::json!({"kind": "human", "name": "You"})
            },
            "black": if black_model {
                model_seat
            } else {
                serde_json::json!({"kind": "human", "name": "You"})
            },
            "max_plies": 300,
            "rated": false,
            "watch": {"white_reasoning": true, "black_reasoning": true},
        });
        create_game(config);
    };

    // An action that changes the match, then refreshes the list and the detail.
    let act = move |action: &'static str, body: serde_json::Value| {
        if busy.get_untracked() {
            return;
        }
        let Some(id) = selected.get_untracked() else {
            return;
        };
        busy.set(true);
        message.set(String::new());
        knightlm_vote.set(None);
        let model_options = serde_json::json!({
            "thinking": thinking.get_untracked(),
            "knightlm_samples": knightlm_samples.get_untracked(),
            "knightlm_temperature": knightlm_temperature.get_untracked(),
        });
        let body = if action == "model-move" {
            model_options.clone()
        } else {
            body
        };
        spawn_local(async move {
            match ai_api::match_action(&id, action, body).await {
                Ok(value) => {
                    // A route may answer with the match itself or wrap it.
                    let vote = value
                        .get("knightlm_vote")
                        .filter(|vote| !vote.is_null())
                        .cloned();
                    let next = value.get("match").cloned().unwrap_or(value);
                    if next.get("id").is_some() {
                        let model_reply = action == "move"
                            && next["result"].is_null()
                            && next["turn"]
                                .as_str()
                                .is_some_and(|turn| next[turn]["kind"] == "model");
                        if selected.get_untracked().as_deref() == Some(&id) {
                            knightlm_vote.set(vote);
                            detail.set(Some(next));
                        }
                        if model_reply {
                            match ai_api::match_action(&id, "model-move", model_options).await {
                                Ok(reply) if selected.get_untracked().as_deref() == Some(&id) => {
                                    knightlm_vote.set(
                                        reply
                                            .get("knightlm_vote")
                                            .filter(|vote| !vote.is_null())
                                            .cloned(),
                                    );
                                    detail.set(reply.get("match").cloned());
                                }
                                Err(error) if selected.get_untracked().as_deref() == Some(&id) => {
                                    message.set(error);
                                }
                                _ => {}
                            }
                        }
                    } else if selected.get_untracked().as_deref() == Some(&id) {
                        load_detail(id);
                    }
                    reload();
                }
                Err(error) => {
                    message.set(error);
                    if action == "move" && selected.get_untracked().as_deref() == Some(&id) {
                        load_detail(id);
                    }
                }
            }
            busy.set(false);
        });
    };

    // Rematch: the same seats in a new game. The old one keeps its result.
    let rematch = move || {
        let Some(id) = selected.get_untracked() else {
            return;
        };
        let Some(row) = matches
            .get_untracked()
            .into_iter()
            .find(|row| row.get("id").and_then(|row_id| row_id.as_str()) == Some(id.as_str()))
        else {
            message.set("the match is no longer in the list".to_string());
            return;
        };
        let config = row
            .get("config_json")
            .and_then(|json| json.as_str())
            .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok());
        let Some(config) = config else {
            message.set("this match has no stored config".to_string());
            return;
        };
        create_game(config);
    };

    let save_to_db = move |open: bool| {
        let Some(payload) = detail.get_untracked() else {
            return;
        };
        let pgn = payload
            .get("pgn")
            .and_then(|pgn| pgn.as_str())
            .unwrap_or("")
            .to_string();
        let ply = replay_ply
            .get_untracked()
            .unwrap_or_else(|| payload["moves"].as_array().map_or(0, Vec::len));
        spawn_local(async move {
            match db_api::import_pgn_ids(&pgn).await {
                Ok(ids) if ids.is_empty() => message.set("nothing was imported".to_string()),
                Ok(ids) if open => match db_api::get_game_summary(ids[0]).await {
                    Ok(game) => open_game_at.run((game, ply)),
                    Err(error) => message.set(error),
                },
                Ok(ids) => message.set(format!("saved {} game(s)", ids.len())),
                Err(error) => message.set(error),
            }
        });
    };

    view! {
        <section class="play">
            <header class="play-head">
                <h2>{move || i18n::t(&language.get(), "nav.play")}</h2>
                <div class="play-new">
                    <label>
                        {move || i18n::t(&language.get(), "play.opponent")}
                        <select class="play-opponent" disabled=move || busy.get()
                            prop:value=move || opponent.get().to_string()
                            on:change=move |event| {
                                opponent.set(event_target_value(&event).parse().unwrap_or(1));
                                model_status.set(String::new());
                            }>
                            <option value="1">"KnightLM (LAMU)"</option>
                            {move || (!settings_model.get().trim().is_empty()).then(|| view! {
                                <option value="0">{move || i18n::t(&language.get(), "play.default_model")}</option>
                            })}
                            {move || local_models.get().into_iter().enumerate().map(|(index, (label, model))| view! {
                                <option value=index + 2 selected=move || opponent.get() == index + 2>{format!("{label} · {model}")}</option>
                            }).collect_view()}
                        </select>
                    </label>
                    <button
                        class="play-start"
                        disabled=move || busy.get()
                        on:click=move |_| start_match(false, true)
                    >
                        {move || i18n::t(&language.get(), "play.new_casual")}
                    </button>
                    <button class="play-friend" disabled=move || busy.get() on:click=move |_| {
                        create_game(serde_json::json!({
                            "start_fen": START_FEN,
                            "white": {"kind": "human", "name": "White"},
                            "black": {"kind": "human", "name": "Black"},
                            "max_plies": 300,
                        }));
                    }>{move || i18n::t(&language.get(), "play.friend")}</button>
                    <button on:click=move |_| {
                        ctx.practice_mode.set("spar".into());
                        ctx.switch_tab.run(crate::model::Tab::Practice);
                    }>{move || i18n::t(&language.get(), "play.engine")}</button>
                </div>
                <div class="play-new">
                    <label title=move || i18n::t(&language.get(), "play.thinking_hint")>
                        <input type="checkbox"
                            prop:checked=move || thinking.get() && lamu_model().is_some()
                            disabled=move || busy.get() || lamu_model().is_none()
                            on:change=move |event| thinking.set(event_target_checked(&event)) />
                        {move || i18n::t(&language.get(), "play.longer_thinking")}
                    </label>
                    <button type="button" disabled=move || busy.get() || lamu_model().is_none()
                        on:click=move |_| manage_model("load")>{move || i18n::t(&language.get(), "play.load_model")}</button>
                    <button type="button" disabled=move || busy.get() || lamu_model().is_none()
                        on:click=move |_| manage_model("unload")>{move || i18n::t(&language.get(), "play.unload_model")}</button>
                </div>
                {move || (thinking.get() && lamu_model().as_deref() == Some(KNIGHTLM_MODEL)).then(|| view! {
                    <div class="play-new">
                        <label>
                            {move || i18n::t(&language.get(), "play.knightlm_samples")}
                            <select prop:value=move || knightlm_samples.get().to_string()
                                disabled=move || busy.get()
                                on:change=move |event| knightlm_samples.set(event_target_value(&event).parse().unwrap_or(5))>
                                {(2..=9).map(|count| view! { <option value=count>{count}</option> }).collect_view()}
                            </select>
                        </label>
                        <label>
                            {move || i18n::t(&language.get(), "play.knightlm_temperature")}
                            <input type="range" min="0.1" max="1.5" step="0.1"
                                prop:value=move || knightlm_temperature.get().to_string()
                                disabled=move || busy.get()
                                on:input=move |event| knightlm_temperature.set(event_target_value(&event).parse().unwrap_or(0.6)) />
                            <output>{move || format!("{:.1}", knightlm_temperature.get())}</output>
                        </label>
                    </div>
                    <small class="muted">{move || i18n::t(&language.get(), "play.knightlm_hint")}</small>
                })}
                {move || lamu_model().is_some().then(|| view! {
                    <small class="muted">{move || i18n::t(&language.get(), "play.model_load_hint")}</small>
                })}
                <div role="status">{move || model_status.get()}</div>
            </header>

            <div class="play-body">
                <div class="active-match">
                    {move || {
                        let Some(payload) = detail.get() else {
                            return view! {
                                <p class="muted">{move || i18n::t(&language.get(), "play.ready")}</p>
                            }.into_any();
                        };
                        let (white_clock, black_clock) = clocks(&payload);
                        let moves = payload
                            .get("moves")
                            .and_then(|moves| moves.as_array())
                            .cloned()
                            .unwrap_or_default();
                        let result = result_text(&payload, &language.get());
                        let finished = result.is_some();
                        let result_for_line = result.clone();
                        let state = state_text(&payload);
                        let turn = payload
                            .get("turn")
                            .and_then(|turn| turn.as_str())
                            .unwrap_or("")
                            .to_string();
                        let white = seat_label(&payload["white"]);
                        let black = seat_label(&payload["black"]);
                        let watching = payload["white"]["kind"] == "model" && payload["black"]["kind"] == "model";
                        let human_turn = payload[turn.as_str()]["kind"] == "human";
                        let human_color = ["white", "black"].into_iter().find(|color| payload[*color]["kind"] == "human");
                        let pgn = payload
                            .get("pgn")
                            .and_then(|pgn| pgn.as_str())
                            .unwrap_or("")
                            .to_string();
                        let pgn_for_download = pgn.clone();
                        let live_fen = payload["fen"].as_str().unwrap_or(START_FEN).to_owned();
                        let replay = replay_ply.get().filter(|ply| *ply < moves.len());
                        let fen = replay.and_then(|ply| {
                            let mut board = brick_chess::Widget::new();
                            board.apply(brick_chess::Command::LoadPgn { pgn: pgn.clone() });
                            if ply >= board.history_len() {
                                return None;
                            }
                            board.apply(brick_chess::Command::Goto { index: ply });
                            Some(board.fen())
                        }).unwrap_or(live_fen);
                        let orientation = human_color.unwrap_or("white").to_owned();
                        let local_game = payload["white"]["kind"] == "human" && payload["black"]["kind"] == "human";
                        let resign_color = if local_game {
                            if turn == "black" { Some("black") } else { Some("white") }
                        } else {
                            human_color
                        };
                        view! {
                            <p class="match-cards">
                                <strong>{white}" vs "{black}</strong>
                                {white_clock.zip(black_clock).map(|(white, black)| {
                                    format!(" · {} / {}", clock_text(Some(white)), clock_text(Some(black)))
                                })}
                            </p>

                            <div class="match-workspace">
                            <Board
                                pgn=Signal::derive(String::new)
                                load_fen=Signal::derive(move || Some((0, fen.clone(), "standard".into())))
                                orientation=Signal::derive(move || orientation.clone())
                                board_only=true
                                read_only=finished || !human_turn || replay.is_some()
                                interaction_disabled=busy.into()
                                lang=language.into()
                                on_move=Callback::new(move |(uci, _, _): (String, String, String)| {
                                    act("move", serde_json::json!({"mv": uci}));
                                })
                            />

                            <aside class="match-inspector">
                            <div class="match-status" role="status">
                                {move || if let Some(result) = &result_for_line {
                                    result.clone()
                                } else if watching {
                                    format!("{state} · {turn} to move")
                                } else if local_game {
                                    i18n::tf(&language.get(), "play.side_turn", &[(
                                        "side", i18n::t(&language.get(), if turn == "black" { "play.black" } else { "play.white" }).to_owned()
                                    )])
                                } else if human_turn {
                                    i18n::t(&language.get(), "play.your_turn").to_owned()
                                } else if busy.get() {
                                    i18n::t(&language.get(), "play.thinking").to_owned()
                                } else {
                                    i18n::t(&language.get(), "play.opponent_turn").to_owned()
                                }}
                            </div>
                            {move || knightlm_vote.get().map(|vote| {
                                let summary = i18n::t(&language.get(), "play.knightlm_vote")
                                    .replace("{votes}", &vote["winning_votes"].to_string())
                                    .replace("{legal}", &vote["legal_samples"].to_string())
                                    .replace("{samples}", &vote["samples"].to_string())
                                    .replace("{direct}", vote["direct_move"].as_str().unwrap_or(""));
                                view! { <p class="muted">{summary}</p> }
                            })}
                            <p class="match-error" role="alert">{move || message.get()}</p>

                            {replay.map(|ply| view! {
                                <p role="status">{move || i18n::tf(&language.get(), "play.reviewing_ply", &[("ply", ply.to_string())])}</p>
                                <button on:click=move |_| replay_ply.set(None)>
                                    {move || i18n::t(&language.get(), "play.return_live")}
                                </button>
                            })}
                            <ol class="match-moves">
                                {moves.iter().enumerate().map(|(index, mv)| {
                                    let san = mv.get("san").and_then(|san| san.as_str()).unwrap_or("").to_string();
                                    let color = mv.get("color").and_then(|c| c.as_str()).unwrap_or("").to_string();
                                    let number = index / 2 + 1;
                                    let prefix = if color == "white" { format!("{number}.") } else { String::new() };
                                    let ply = index + 1;
                                    view! {
                                        <li class=format!("ply {color}")>
                                            <button data-match-ply=ply
                                                aria-pressed=if replay.unwrap_or(moves.len()) == ply { "true" } else { "false" }
                                                on:click=move |_| replay_ply.set(Some(ply))>
                                                {format!("{prefix} {san}")}
                                            </button>
                                        </li>
                                    }
                                }).collect_view()}
                            </ol>

                            <div class="match-actions">
                                {watching.then(|| view! {
                                <button
                                    class="match-step"
                                    disabled=move || busy.get() || finished
                                    on:click=move |_| act("watch/step", serde_json::json!({"plies": 1}))
                                >
                                    {move || i18n::t(&language.get(), "play.step")}
                                </button>
                                <button
                                    class="match-step-5"
                                    disabled=move || busy.get() || finished
                                    on:click=move |_| act("watch/step", serde_json::json!({"plies": 5}))
                                >
                                    {move || i18n::t(&language.get(), "play.step5")}
                                </button>
                                <button
                                    class="match-pause"
                                    on:click=move |_| act("watch/pause", serde_json::json!({}))
                                >
                                    {move || i18n::t(&language.get(), "play.pause")}
                                </button>
                                <button
                                    class="match-resume"
                                    on:click=move |_| act("watch/resume", serde_json::json!({}))
                                >
                                    {move || i18n::t(&language.get(), "play.resume")}
                                </button>
                                })}
                                {(!finished && !watching && !human_turn).then(|| view! {
                                    <button class="match-model-move" hidden=move || busy.get()
                                        on:click=move |_| act("model-move", serde_json::json!({}))>
                                        {move || i18n::t(&language.get(), "play.model_move")}
                                    </button>
                                })}
                                {resign_color.filter(|_| !finished).map(|color| view! {
                                    <button class="match-resign" disabled=move || busy.get()
                                        on:click=move |_| act("resign", serde_json::json!({"color": color}))>
                                        {move || i18n::t(&language.get(), "play.resign")}
                                    </button>
                                })}
                                <button
                                    class="match-takeback"
                                    on:click=move |_| act("takeback", serde_json::json!({}))
                                >
                                    {move || i18n::t(&language.get(), "play.takeback")}
                                </button>
                                <button
                                    class="match-rematch"
                                    disabled=move || busy.get()
                                    on:click=move |_| rematch()
                                >
                                    {move || i18n::t(&language.get(), "play.rematch")}
                                </button>
                                <button
                                    class="match-reset"
                                    on:click=move |_| act("watch/reset", serde_json::json!({}))
                                >
                                    {move || i18n::t(&language.get(), "play.reset")}
                                </button>
                            </div>

                            <div class="match-handoff">
                                <button class="match-save" on:click=move |_| save_to_db(false)>
                                    {move || i18n::t(&language.get(), "play.save")}
                                </button>
                                <button class="match-open" on:click=move |_| save_to_db(true)>
                                    {move || i18n::t(&language.get(), "play.open")}
                                </button>
                                <button
                                    class="match-export"
                                    on:click=move |_| {
                                        download_pgn(&pgn_for_download);
                                    }
                                >
                                    {move || i18n::t(&language.get(), "play.export")}
                                </button>
                                <details>
                                    <summary>"PGN"</summary>
                                    <pre class="match-pgn">{pgn}</pre>
                                </details>
                            </div>
                            </aside>
                            </div>
                        }.into_any()
                    }}
                    {move || detail.get().is_none().then(|| view! {
                        <p class="match-error" role="alert">{move || message.get()}</p>
                    })}
                </div>
                <details class="play-more">
                    <summary>{move || i18n::t(&language.get(), "play.more")}</summary>
                    <button class="play-scan-models" disabled=move || scanning.get() || busy.get() on:click=move |_| scan_local()>
                        {move || i18n::t(&language.get(), "play.scan_models")}
                    </button>
                    <button class="play-watch" disabled=move || busy.get() on:click=move |_| start_match(true, true)>
                        {move || i18n::t(&language.get(), "play.new_watch")}
                    </button>
                    <button class="play-refresh" on:click=move |_| reload()>
                        {move || i18n::t(&language.get(), "approvals.refresh")}
                    </button>
                <ul class="match-list">
                    {move || {
                        let list = matches.get();
                        if list.is_empty() {
                            return view! {
                                <li class="muted">{move || i18n::t(&language.get(), "play.empty")}</li>
                            }.into_any();
                        }
                        list.into_iter().map(|row| {
                            let id = row.get("id").and_then(|id| id.as_str()).unwrap_or("").to_string();
                            let id_click = id.clone();
                            let data_id = id.clone();
                            let white = row
                                .get("white_json")
                                .and_then(|json| json.as_str())
                                .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
                                .map(|seat| seat_label(&seat))
                                .unwrap_or_default();
                            let black = row
                                .get("black_json")
                                .and_then(|json| json.as_str())
                                .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
                                .map(|seat| seat_label(&seat))
                                .unwrap_or_default();
                            let ended = row.get("ended").map(|ended| !ended.is_null()).unwrap_or(false);
                            view! {
                                <li>
                                    <button
                                        data-match-id=data_id
                                        class=move || if selected.get().as_deref() == Some(id.as_str()) {
                                            "match-card active"
                                        } else {
                                            "match-card"
                                        }
                                        disabled=move || busy.get()
                                        on:click=move |_| {
                                            knightlm_vote.set(None);
                                            selected.set(Some(id_click.clone()));
                                        }
                                    >
                                        <span class="match-card-seats">{format!("{white} vs {black}")}</span>
                                        <span class="muted">
                                            {if ended {
                                                i18n::t(&language.get(), "play.ended").to_string()
                                            } else {
                                                i18n::t(&language.get(), "play.live").to_string()
                                            }}
                                        </span>
                                    </button>
                                </li>
                            }
                        }).collect_view().into_any()
                    }}
                </ul>

                </details>
            </div>
        </section>
    }
}

pub fn tournaments(ctx: AppCtx) -> impl IntoView {
    let language = ctx.language;
    let message = ctx.status;
    let board_rows = RwSignal::new(Vec::<ai_api::LeaderboardRow>::new());
    let board_metrics = RwSignal::new(Vec::<ai_api::ModelMetrics>::new());
    let board_filter = RwSignal::new(String::new());
    let tournaments = RwSignal::new(Vec::<serde_json::Value>::new());
    let tournament_id = RwSignal::new(None::<String>);
    let tournament_detail = RwSignal::new(None::<serde_json::Value>);
    let tour_name = RwSignal::new(String::new());
    let tour_format = RwSignal::new("double-round-robin".to_string());
    let tour_competitors = RwSignal::new(String::new());
    let tour_log = RwSignal::new(Vec::<String>::new());

    let reload_board = move || {
        spawn_local(async move {
            if let Ok(rows) = ai_api::leaderboard().await {
                board_rows.set(rows);
            }
            if let Ok(rows) = ai_api::metrics().await {
                board_metrics.set(rows);
            }
        });
    };
    let reload_tournaments = move || {
        spawn_local(async move {
            if let Ok(list) = ai_api::list_tournaments().await {
                tournaments.set(list);
            }
        });
    };

    Effect::new(move |_| reload_board());
    Effect::new(move |_| reload_tournaments());
    Effect::new(move |_| {
        if let Some(id) = tournament_id.get() {
            spawn_local(async move {
                if let Ok(detail) = ai_api::tournament_detail(&id).await {
                    tournament_detail.set(Some(detail));
                }
            });
        }
    });

    view! {
        <section class="panel competition-view">
            <div class="panel-head">
                <h2>{move || i18n::t(&language.get(), "play.tournaments")}</h2>
                <button on:click=move |_| {
                    reload_board();
                    reload_tournaments();
                }>{move || i18n::t(&language.get(), "approvals.refresh")}</button>
            </div>
                <div class="match-panel">
                    <section class="leaderboard">
                        <h3>{move || i18n::t(&language.get(), "play.leaderboard")}</h3>
                        <input
                            class="board-filter"
                            placeholder=move || i18n::t(&language.get(), "play.filter")
                            prop:value=move || board_filter.get()
                            on:input=move |event| board_filter.set(event_target_value(&event))
                        />
                        <table class="leaderboard-table">
                            <thead>
                                <tr>
                                    <th>{move || i18n::t(&language.get(), "play.competitor")}</th>
                                    <th>{move || i18n::t(&language.get(), "play.rating")}</th>
                                    <th>"RD"</th>
                                    <th>{move || i18n::t(&language.get(), "play.conservative")}</th>
                                    <th>{move || i18n::t(&language.get(), "play.games")}</th>
                                    <th>{move || i18n::t(&language.get(), "play.legal")}</th>
                                    <th>{move || i18n::t(&language.get(), "play.turns")}</th>
                                    <th>{move || i18n::t(&language.get(), "play.tokens")}</th>
                                </tr>
                            </thead>
                            <tbody>
                                {move || {
                                    let filter = board_filter.get().to_lowercase();
                                    let metrics = board_metrics.get();
                                    board_rows.get()
                                        .into_iter()
                                        .filter(|row| filter.is_empty() || row.competitor.to_lowercase().contains(&filter))
                                        .map(|row| {
                                            let legal = metrics
                                                .iter()
                                                .find(|m| row.competitor.contains(&m.model))
                                                .and_then(|m| m.legal_rate)
                                                .map(|rate| format!("{:.0}%", rate * 100.0))
                                                .unwrap_or_else(|| "—".to_string());
                                            let matched = metrics.iter().find(|m| row.competitor.contains(&m.model));
                                            let turns = matched.map(|m| m.turns).unwrap_or(0);
                                            let tokens = matched
                                                .map(|m| m.tokens_in + m.tokens_out)
                                                .unwrap_or(0);
                                            view! {
                                                <tr>
                                                    <td>{row.competitor.clone()}</td>
                                                    <td>{format!("{:.0}", row.rating)}</td>
                                                    <td>{format!("{:.0}", row.deviation)}</td>
                                                    <td>{format!("{:.0}", row.conservative)}</td>
                                                    <td>{row.games}</td>
                                                    <td>{legal}</td>
                                                    <td>{turns}</td>
                                                    <td>{tokens}</td>
                                                </tr>
                                            }
                                        })
                                        .collect_view()
                                }}
                            </tbody>
                        </table>
                    </section>

                    <section class="tournaments">
                        <h3>{move || i18n::t(&language.get(), "play.tournaments")}</h3>
                        <div class="tournament-form">
                            <input
                                class="tournament-name"
                                placeholder=move || i18n::t(&language.get(), "play.tournament_name")
                                prop:value=move || tour_name.get()
                                on:input=move |event| tour_name.set(event_target_value(&event))
                            />
                            <select
                                class="tournament-format"
                                aria-label=move || i18n::t(&language.get(), "play.format")
                                on:change=move |event| tour_format.set(event_target_value(&event))
                            >
                                <option value="double-round-robin">{move || i18n::t(&language.get(), "play.format_rr")}</option>
                                <option value="swiss">{move || i18n::t(&language.get(), "play.format_swiss")}</option>
                                <option value="knockout">{move || i18n::t(&language.get(), "play.format_knockout")}</option>
                            </select>
                            <input
                                class="tournament-competitors"
                                placeholder=move || i18n::t(&language.get(), "play.competitors_hint")
                                prop:value=move || tour_competitors.get()
                                on:input=move |event| tour_competitors.set(event_target_value(&event))
                            />
                        </div>
                        <div class="tournament-actions">
                            <button
                                class="tournament-create"
                                on:click=move |_| {
                                    let name = tour_name.get_untracked();
                                    let name = if name.trim().is_empty() { "Tournament".to_string() } else { name };
                                    let format = tour_format.get_untracked();
                                    let competitors: Vec<String> = tour_competitors
                                        .get_untracked()
                                        .split(',')
                                        .map(str::trim)
                                        .filter(|c| !c.is_empty())
                                        .map(str::to_string)
                                        .collect();
                                    let log = tour_log;
                                    spawn_local(async move {
                                        match ai_api::create_tournament(&name, &format, &competitors).await {
                                            Ok(value) => {
                                                reload_tournaments();
                                                if let Some(id) = value.get("id").and_then(|id| id.as_str()) {
                                                    tournament_id.set(Some(id.to_string()));
                                                    log.update(|lines| lines.push(format!("created {name} ({format})")));
                                                }
                                            }
                                            Err(error) => message.set(error),
                                        }
                                    });
                                }
                            >
                                {move || i18n::t(&language.get(), "play.new_tournament")}
                            </button>
                            <button class="tournament-step" on:click=move |_| {
                                if let Some(id) = tournament_id.get_untracked() {
                                    spawn_local(async move {
                                        if let Ok(value) = ai_api::tournament_action(&id, "step").await {
                                            let started = value.get("started").and_then(|s| s.as_array()).map(|a| a.len()).unwrap_or(0);
                                            message.set(format!("started {started} game(s)"));
                                            tour_log.update(|lines| lines.push(format!("step: started {started} game(s)")));
                                            if let Ok(detail) = ai_api::tournament_detail(&id).await {
                                                tournament_detail.set(Some(detail));
                                            }
                                        }
                                        reload_tournaments();
                                    });
                                }
                            }>
                                {move || i18n::t(&language.get(), "play.step")}
                            </button>
                            <button class="tournament-pause" on:click=move |_| {
                                if let Some(id) = tournament_id.get_untracked() {
                                    spawn_local(async move {
                                        if let Ok(value) = ai_api::tournament_action(&id, "pause").await {
                                            let status = value.get("status").and_then(|s| s.as_str()).unwrap_or("").to_string();
                                            message.set(format!("tournament {status}"));
                                            tour_log.update(|lines| lines.push(format!("pause: {status}")));
                                            if let Ok(detail) = ai_api::tournament_detail(&id).await {
                                                tournament_detail.set(Some(detail));
                                            }
                                        }
                                    });
                                }
                            }>
                                {move || i18n::t(&language.get(), "play.pause")}
                            </button>
                            <button class="tournament-export" on:click=move |_| {
                                if let Some(id) = tournament_id.get_untracked() {
                                    spawn_local(async move {
                                        match ai_api::tournament_pgn(&id).await {
                                            Ok(pgn) => download_pgn(&pgn),
                                            Err(error) => message.set(error),
                                        }
                                    });
                                }
                            }>
                                {move || i18n::t(&language.get(), "play.export")}
                            </button>
                            <button class="tournament-resume" on:click=move |_| {
                                if let Some(id) = tournament_id.get_untracked() {
                                    spawn_local(async move {
                                        if let Ok(value) = ai_api::tournament_action(&id, "resume").await {
                                            let status = value.get("status").and_then(|s| s.as_str()).unwrap_or("").to_string();
                                            message.set(format!("tournament {status}"));
                                            tour_log.update(|lines| lines.push(format!("resume: {status}")));
                                            if let Ok(detail) = ai_api::tournament_detail(&id).await {
                                                tournament_detail.set(Some(detail));
                                            }
                                        }
                                    });
                                }
                            }>
                                {move || i18n::t(&language.get(), "play.resume")}
                            </button>
                        </div>
                        <ul class="tournament-list">
                            {move || {
                                let list = tournaments.get();
                                if list.is_empty() {
                                    return view! { <li class="muted">{move || i18n::t(&language.get(), "play.no_tournaments")}</li> }.into_any();
                                }
                                list.into_iter().map(|row| {
                                    let id = row.get("id").and_then(|id| id.as_str()).unwrap_or("").to_string();
                                    let id_click = id.clone();
                                    let name = row.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                                    let status = row.get("status").and_then(|s| s.as_str()).unwrap_or("").to_string();
                                    view! {
                                        <li>
                                            <button
                                                class=move || if tournament_id.get().as_deref() == Some(id.as_str()) {
                                                    "tournament-row active"
                                                } else {
                                                    "tournament-row"
                                                }
                                                on:click=move |_| tournament_id.set(Some(id_click.clone()))
                                            >
                                                <span>{name}</span>
                                                <span class="muted">{status}</span>
                                            </button>
                                        </li>
                                    }
                                }).collect_view().into_any()
                            }}
                        </ul>
                        {move || tournament_detail.get().map(|detail| {
                            let rounds = detail
                                .get("tournament")
                                .and_then(|t| t.get("schedule"))
                                .and_then(|s| s.get("rounds"))
                                .and_then(|r| r.as_array())
                                .cloned()
                                .unwrap_or_default();
                            let last = rounds.last().cloned().unwrap_or(serde_json::Value::Null);
                            let pairings = last
                                .get("games")
                                .and_then(|g| g.as_array())
                                .cloned()
                                .unwrap_or_default();
                            view! {
                                <ol class="pairings">
                                    {pairings.into_iter().map(|game| {
                                        let white = game.get("white").and_then(|w| w.as_str()).unwrap_or("").to_string();
                                        let black = game.get("black").and_then(|b| b.as_str()).unwrap_or("").to_string();
                                        let outcome = match game.get("outcome").and_then(|o| o.as_str()) {
                                            Some("White") => "1-0",
                                            Some("Black") => "0-1",
                                            Some("Draw") => "½-½",
                                            _ => "·",
                                        };
                                        view! { <li class="pairing">{format!("{white} — {black}  {outcome}")}</li> }
                                    }).collect_view()}
                                </ol>
                            }.into_any()
                        })}
                        {move || {
                            let lines = tour_log.get();
                            (!lines.is_empty()).then(|| view! {
                                <ul class="tournament-log">
                                    {lines.into_iter().map(|line| view! { <li>{line}</li> }).collect_view()}
                                </ul>
                            })
                        }}
                        {move || tournament_detail.get().map(|detail| {
                            let rows = detail.get("standings").and_then(|s| s.as_array()).cloned().unwrap_or_default();
                            view! {
                                <table class="standings-table">
                                    <thead>
                                        <tr>
                                            <th>{move || i18n::t(&language.get(), "play.competitor")}</th>
                                            <th>"P"</th>
                                            <th>{move || i18n::t(&language.get(), "play.wins")}</th>
                                            <th>"Buchholz"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {rows.into_iter().map(|row| {
                                            let name = row.get("competitor").and_then(|c| c.as_str()).unwrap_or("").to_string();
                                            let points = row.get("points").and_then(|p| p.as_f64()).unwrap_or(0.0);
                                            let wins = row.get("wins").and_then(|w| w.as_i64()).unwrap_or(0);
                                            let buchholz = row.get("buchholz").and_then(|b| b.as_f64()).unwrap_or(0.0);
                                            view! {
                                                <tr>
                                                    <td>{name}</td>
                                                    <td>{format!("{points}")}</td>
                                                    <td>{wins}</td>
                                                    <td>{format!("{buchholz:.1}")}</td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            }.into_any()
                        })}
                    </section>
                </div>
        </section>
    }
}
