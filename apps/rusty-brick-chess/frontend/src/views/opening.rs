use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::board::Board;
use crate::i18n;
use crate::model::*;
use crate::views::AppCtx;

pub fn opening(ctx: AppCtx) -> impl IntoView {
    let AppCtx {
        analysis: _, fen, games: _, language, opening, opening_fen, opening_mode, opening_source, rep_board, rep_card, rep_color, rep_correct, rep_feedback, rep_id, rep_name, rep_pgn_input, rep_selected, reps, settings_coords, settings_dark, settings_light, settings_pieces, settings_theme, status, ..
    } = ctx;
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
                    placeholder=move || i18n::t(&language.get(), "openings.fen_ph")
                    prop:value=move || opening_fen.get()
                    on:input=move |ev| opening_fen.set(event_target_value(&ev))
                />
                <button on:click=explore>{move || i18n::t(&language.get(), "openings.explore")}</button>
                <button
                    title=move || i18n::t(&language.get(), "openings.use_current")
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
                    {move || i18n::t(&language.get(), "analysis.current_board")}
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
                                        <span>{move || i18n::t(&language.get(), "db.move")}</span><span>{move || i18n::t(&language.get(), "db.games")}</span>
                                        <span>"White"</span><span>{move || i18n::t(&language.get(), "db.draw")}</span><span>"Black"</span>
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
                    rep_feedback.set(i18n::t(&language.get(), "openings.no_due").into());
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
                    status.set(i18n::tf(&language.get(), "status.d16", &[("name", r.name.clone())]));
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
                    status.set(i18n::tf(&language.get(), "status.d17", &[("n", n.to_string())]));
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
                                        title=Signal::derive(move || i18n::t(&language.get(), "openings.rep_training").to_string())
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
                                    light_color=Signal::derive(move || settings_light.get())
                                    dark_color=Signal::derive(move || settings_dark.get())
                                    coords=Signal::derive(move || settings_coords.get())
                                    lang=Signal::derive(move || language.get())
                                    />
                                </div>
                                <aside class="engine-pane">
                                    <div class="panel-head"><h2>{move || i18n::t(&language.get(), "openings.repertoire")}</h2></div>
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
                                        <button on:click=next_card>{move || i18n::t(&language.get(), "practice.next_card")}</button>
                                    </div>
                                </aside>
                            </section>
                        }
                    })
            }}
            <section class="panel">
                <div class="panel-head">
                    <h2>{move || i18n::t(&language.get(), "nav.openings")}</h2>
                    <div class="row-btns">
                        <button
                            class=move || if opening_mode.get() == "explorer" { "chip active" } else { "chip" }
                            on:click=move |_| opening_mode.set("explorer".into())
                        >
                            {move || i18n::t(&language.get(), "openings.explorer")}
                        </button>
                        <button
                            class=move || if opening_mode.get() == "repertoire" { "chip active" } else { "chip" }
                            on:click=move |_| opening_mode.set("repertoire".into())
                        >
                            {move || i18n::t(&language.get(), "openings.repertoires")}
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
                                    <option value="local">{move || i18n::t(&language.get(), "openings.local")}</option>
                                    <option value="lichess">{move || i18n::t(&language.get(), "openings.lichess")}</option>
                                    <option value="masters">{move || i18n::t(&language.get(), "openings.masters")}</option>
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
                                    placeholder=move || i18n::t(&language.get(), "openings.rep_name")
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
                                <button on:click=create_rep>{move || i18n::t(&language.get(), "common.create")}</button>
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
                                                        {move || i18n::t(&language.get(), "openings.train")}
                                                    </button>
                                                    <button on:click=move |_| rep_selected.set(for_import)>
                                                        {move || i18n::t(&language.get(), "db.import_pgn")}
                                                    </button>
                                                    <button on:click=move |_| delete_rep(for_delete)>
                                                        {move || i18n::t(&language.get(), "db.delete")}
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
                                                    placeholder=move || i18n::t(&language.get(), "openings.rep_paste")
                                                    prop:value=move || rep_pgn_input.get()
                                                    on:input=move |ev| rep_pgn_input.set(event_target_value(&ev))
                                                ></textarea>
                                                <button on:click=move |_| import_rep(id)>
                                                    {move || i18n::t(&language.get(), "openings.import_into")}
                                                </button>
                                            </div>
                                        }
                                    })
                            }}
                            <div class="panel-sub">{move || i18n::t(&language.get(), "openings.training")}</div>
                            <div class="muted">
                                {move || rep_feedback.get()}
                            </div>
                            <div class="row-btns">
                                <button on:click=next_card>{move || i18n::t(&language.get(), "practice.next_card")}</button>
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
}
