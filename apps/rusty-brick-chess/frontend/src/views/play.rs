//! The Play tab: matches against a model, and model-versus-model games.
//!
//! The view is a list of matches on the left and the chosen one on the right:
//! player cards with their clocks, the move list, the controls that suit that
//! kind of game (a move picker for a human seat, step/pause for two models),
//! and the three ways a finished game leaves this screen — into the database,
//! into the Analysis tab, or out as a PGN file.

use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;

use crate::ai_api;
use crate::api as db_api;
use crate::i18n;
use crate::views::AppCtx;

const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

fn seat_label(seat: &serde_json::Value) -> String {
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

fn result_text(payload: &serde_json::Value) -> Option<String> {
    let result = payload.get("result")?;
    if result.is_null() {
        return None;
    }
    let winner = result
        .get("winner")
        .and_then(|w| w.as_str())
        .unwrap_or("draw");
    let termination = result
        .get("termination")
        .and_then(|t| t.as_str())
        .unwrap_or("");
    Some(format!("{winner} · {termination}"))
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
    if let Ok(anchor) = document.create_element("a") {
        if let Ok(anchor) = anchor.dyn_into::<web_sys::HtmlAnchorElement>() {
            anchor.set_href(&url);
            anchor.set_download("match.pgn");
            anchor.click();
        }
    }
    let _ = web_sys::Url::revoke_object_url(&url);
}

pub fn play(ctx: AppCtx) -> impl IntoView {
    let AppCtx {
        language,
        open_game,
        ..
    } = ctx;

    let matches = RwSignal::new(Vec::<serde_json::Value>::new());
    let selected = RwSignal::new(None::<String>);
    let detail = RwSignal::new(None::<serde_json::Value>);
    let busy = RwSignal::new(false);
    let message = RwSignal::new(String::new());
    // The leaderboard and the tournaments live beside the matches: they are the
    // same question — who is playing well — asked over a longer window.
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
                Ok(value) => detail.set(Some(value)),
                Err(error) => message.set(error),
            }
        });
    };

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

    // Load once, and whenever the list is asked for again.
    Effect::new(move |_| reload());
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

    // Follow the selection.
    Effect::new(move |_| {
        if let Some(id) = selected.get() {
            load_detail(id);
        } else {
            detail.set(None);
        }
    });

    let start_match = move |white_model: bool, black_model: bool| {
        let config = serde_json::json!({
            "start_fen": START_FEN,
            "white": if white_model {
                serde_json::json!({"kind": "model", "provider": "local", "model": "assistant"})
            } else {
                serde_json::json!({"kind": "human", "name": "You"})
            },
            "black": if black_model {
                serde_json::json!({"kind": "model", "provider": "local", "model": "assistant"})
            } else {
                serde_json::json!({"kind": "human", "name": "You"})
            },
            "max_plies": 300,
            "rated": false,
            "watch": {"white_reasoning": true, "black_reasoning": true},
        });
        busy.set(true);
        spawn_local(async move {
            match ai_api::create_ai_match(config).await {
                Ok(value) => {
                    if let Some(id) = value.get("id").and_then(|id| id.as_str()) {
                        selected.set(Some(id.to_string()));
                        detail.set(Some(value));
                    }
                    reload();
                }
                Err(error) => message.set(error),
            }
            busy.set(false);
        });
    };

    // An action that changes the match, then refreshes the list and the detail.
    let act = move |action: &'static str, body: serde_json::Value| {
        let Some(id) = selected.get_untracked() else {
            return;
        };
        busy.set(true);
        spawn_local(async move {
            match ai_api::match_action(&id, action, body).await {
                Ok(value) => {
                    // A route may answer with the match itself or wrap it.
                    let next = value.get("match").cloned().unwrap_or(value);
                    if next.get("id").is_some() {
                        detail.set(Some(next));
                    } else {
                        load_detail(id);
                    }
                    reload();
                }
                Err(error) => message.set(error),
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
        busy.set(true);
        spawn_local(async move {
            match ai_api::create_ai_match(config).await {
                Ok(value) => {
                    if let Some(id) = value.get("id").and_then(|id| id.as_str()) {
                        selected.set(Some(id.to_string()));
                        detail.set(Some(value));
                    }
                    reload();
                }
                Err(error) => message.set(error),
            }
            busy.set(false);
        });
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
        spawn_local(async move {
            match db_api::import_pgn(&pgn).await {
                Ok(0) => message.set("nothing was imported".to_string()),
                Ok(count) => {
                    if open {
                        // Opening means putting it in front of the board, and a
                        // game lives in the database once it leaves this screen.
                        let filters = db_api::Filters::default();
                        match db_api::list_games(&filters).await {
                            Ok((list, _)) => match list.into_iter().next() {
                                Some(summary) => open_game.run(summary),
                                None => message.set("the game was saved but not found".to_string()),
                            },
                            Err(error) => message.set(error),
                        }
                    } else {
                        message.set(format!("saved {count} game(s)"));
                    }
                }
                Err(error) => message.set(error),
            }
        });
    };

    view! {
        <section class="play">
            <header class="play-head">
                <h2>{move || i18n::t(&language.get(), "nav.play")}</h2>
                <div class="play-new">
                    <button
                        class="play-start"
                        disabled=move || busy.get()
                        on:click=move |_| start_match(false, true)
                    >
                        {move || i18n::t(&language.get(), "play.new_casual")}
                    </button>
                    <button
                        class="play-start"
                        disabled=move || busy.get()
                        on:click=move |_| start_match(true, true)
                    >
                        {move || i18n::t(&language.get(), "play.new_watch")}
                    </button>
                    <button class="play-refresh" on:click=move |_| reload()>
                        {move || i18n::t(&language.get(), "approvals.refresh")}
                    </button>
                </div>
            </header>

            <div class="play-body">
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
                                        class=move || if selected.get().as_deref() == Some(id.as_str()) {
                                            "match-card active"
                                        } else {
                                            "match-card"
                                        }
                                        on:click=move |_| selected.set(Some(id_click.clone()))
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
                    {move || {
                        let Some(payload) = detail.get() else {
                            return view! {
                                <p class="muted">{move || i18n::t(&language.get(), "play.choose")}</p>
                            }.into_any();
                        };
                        let (white_clock, black_clock) = clocks(&payload);
                        let moves = payload
                            .get("moves")
                            .and_then(|moves| moves.as_array())
                            .cloned()
                            .unwrap_or_default();
                        let legal = payload
                            .get("legal_moves")
                            .and_then(|moves| moves.as_array())
                            .map(|moves| {
                                moves
                                    .iter()
                                    .filter_map(|mv| mv.as_str().map(str::to_string))
                                    .take(16)
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        let result = result_text(&payload);
                        let finished = result.is_some();
                        let result_for_line = result.clone();
                        let state = state_text(&payload);
                        let turn = payload
                            .get("turn")
                            .and_then(|turn| turn.as_str())
                            .unwrap_or("")
                            .to_string();
                        let pgn = payload
                            .get("pgn")
                            .and_then(|pgn| pgn.as_str())
                            .unwrap_or("")
                            .to_string();
                        let pgn_for_download = pgn.clone();
                        view! {
                            <div class="match-cards">
                                <div class="player-card">
                                    <span class="player-name">{move || i18n::t(&language.get(), "play.white")}</span>
                                    <span class="player-clock">{clock_text(white_clock)}</span>
                                </div>
                                <div class="player-card">
                                    <span class="player-name">{move || i18n::t(&language.get(), "play.black")}</span>
                                    <span class="player-clock">{clock_text(black_clock)}</span>
                                </div>
                            </div>

                            <div class="match-status">
                                <span class="match-state">{state.clone()}</span>
                                <span class="muted">{format!("{} to move", turn)}</span>
                                {result_for_line.map(|result| view! {
                                    <span class="match-result">{result}</span>
                                })}
                            </div>

                            <ol class="match-moves">
                                {moves.iter().enumerate().map(|(index, mv)| {
                                    let san = mv.get("san").and_then(|san| san.as_str()).unwrap_or("").to_string();
                                    let color = mv.get("color").and_then(|c| c.as_str()).unwrap_or("").to_string();
                                    let number = index / 2 + 1;
                                    let prefix = if color == "white" { format!("{number}.") } else { String::new() };
                                    view! { <li class=format!("ply {color}")>{format!("{prefix} {san}")}</li> }
                                }).collect_view()}
                            </ol>

                            <div class="match-actions">
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

                            {(!legal.is_empty()).then(|| view! {
                                <div class="match-legal">
                                    {legal.into_iter().map(|mv| {
                                        let mv_click = mv.clone();
                                        view! {
                                            <button
                                                class="legal-move"
                                                on:click=move |_| act("move", serde_json::json!({"mv": mv_click.clone()}))
                                            >
                                                {mv}
                                            </button>
                                        }
                                    }).collect_view()}
                                </div>
                            })}

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
                                <pre class="match-pgn">{pgn}</pre>
                            </div>
                        }.into_any()
                    }}
                    </section>
                    <div class="muted">{move || message.get()}</div>
                </div>
            </div>
        </section>
    }
}
