use leptos::prelude::*;
use crate::board::{Board};
use crate::i18n;
use crate::model::*;
use crate::views::AppCtx;

pub fn practice(ctx: AppCtx) -> impl IntoView {
    let AppCtx {
        engines,
        fen,
        keybinds,
        language,
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
        selected_game_id,
        settings_coords,
        settings_dark,
        settings_light,
        settings_pieces,
        settings_theme,
        spar_color,
        spar_fen,
        spar_pgn,
        spar_play,
        spar_status,
        spar_thinking,
        total: _,
        ..
    } = ctx;
    let on_match_play = move |args| ctx.on_match_play.run(args);
    let on_practice_move = move |args| ctx.on_practice_move.run(args);
    let on_spar_move = move |args| ctx.on_spar_move.run(args);
    let pause_match = move |a| ctx.pause_match.run(a);
    let reset_match = move |a| ctx.reset_match.run(a);
    let start_guess = move |a| ctx.start_guess.run(a);
    let start_match = move |a| ctx.start_match.run(a);
    let start_spar = move |a| ctx.start_spar.run(a);
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
                        {move || i18n::t(&language.get(), "practice.guess")}
                    </button>
                    <button
                        class=move || {
                            if practice_mode.get() == "spar" { "mode active" } else { "mode" }
                        }
                        on:click=move |_| practice_mode.set("spar".into())
                    >
                        {move || i18n::t(&language.get(), "practice.spar")}
                    </button>
                    <button
                        class=move || {
                            if practice_mode.get() == "engine" { "mode active" } else { "mode" }
                        }
                        on:click=move |_| practice_mode.set("engine".into())
                    >
                        {move || i18n::t(&language.get(), "practice.match")}
                    </button>
                </div>
                {move || {
                    if practice_mode.get() == "guess" {
                        view! {
                            <div class="practice-row">
                                <input
                                    type="text"
                                    placeholder=move || i18n::t(&language.get(), "practice.filter")
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
                                    <option value="0">{move || i18n::t(&language.get(), "practice.choose_game")}</option>
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
                                    {move || i18n::t(&language.get(), "practice.practise")}
                                </button>
                                <button
                                    disabled=move || selected_game_id.get().is_none()
                                    on:click=move |_| {
                                        if let Some(id) = selected_game_id.get_untracked() {
                                            start_guess(id);
                                        }
                                    }
                                >
                                    {move || i18n::t(&language.get(), "practice.use_analysis")}
                                </button>
                            </div>
                        }
                            .into_any()
                    } else if practice_mode.get() == "spar" {
                        view! {
                            <div class="practice-row">
                                <input
                                    type="text"
                                    placeholder=move || i18n::t(&language.get(), "practice.start_fen_long")
                                    prop:value=move || spar_fen.get()
                                    on:input=move |ev| spar_fen.set(event_target_value(&ev))
                                />
                                <select
                                    prop:value=move || spar_color.get()
                                    on:change=move |ev| spar_color.set(event_target_value(&ev))
                                >
                                    <option value="white">{move || i18n::t(&language.get(), "practice.play_white")}</option>
                                    <option value="black">{move || i18n::t(&language.get(), "practice.play_black")}</option>
                                </select>
                                <button on:click=start_spar>{move || i18n::t(&language.get(), "common.start")}</button>
                            </div>
                        }
                            .into_any()
                    } else {
                        view! {
                            <div class="practice-row">
                                <select
                                    prop:value=move || match_white.get().to_string()
                                    on:change=move |ev| {
                                        match_white.set(event_target_value(&ev).parse().unwrap_or(0))
                                    }
                                >
                                    {move || {
                                        engines
                                            .get()
                                            .into_iter()
                                            .enumerate()
                                            .map(|(i, engine)| {
                                                view! {
                                                    <option value=i.to_string()>
                                                        {format!("White: {}", engine.name)}
                                                    </option>
                                                }
                                            })
                                            .collect_view()
                                    }}
                                </select>
                                <select
                                    prop:value=move || match_black.get().to_string()
                                    on:change=move |ev| {
                                        match_black.set(event_target_value(&ev).parse().unwrap_or(0))
                                    }
                                >
                                    {move || {
                                        engines
                                            .get()
                                            .into_iter()
                                            .enumerate()
                                            .map(|(i, engine)| {
                                                view! {
                                                    <option value=i.to_string()>
                                                        {format!("Black: {}", engine.name)}
                                                    </option>
                                                }
                                            })
                                            .collect_view()
                                    }}
                                </select>
                                <input
                                    type="number"
                                    min="1"
                                    max="30"
                                    title=move || i18n::t(&language.get(), "settings.depth_label")
                                    style="max-width:6rem"
                                    prop:value=move || match_depth.get().to_string()
                                    on:input=move |ev| {
                                        match_depth
                                            .set(event_target_value(&ev).parse().unwrap_or(10))
                                    }
                                />
                                <input
                                    type="text"
                                    placeholder=move || i18n::t(&language.get(), "practice.start_fen")
                                    prop:value=move || match_start_fen.get()
                                    on:input=move |ev| match_start_fen.set(event_target_value(&ev))
                                />
                                <button on:click=start_match>{move || i18n::t(&language.get(), "common.start")}</button>
                                <button on:click=pause_match>{move || i18n::t(&language.get(), "common.pause")}</button>
                                <button on:click=reset_match>{move || i18n::t(&language.get(), "common.reset")}</button>
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
                                .unwrap_or_else(|| i18n::t(&language.get(), "practice.guess").into());
                            view! {
                                <Board
                                    pgn=practice_pgn
                                    title=Signal::derive(move || side.clone())
                                    on_move=Callback::new(on_practice_move)
                                    on_position=Callback::new(move |f: String| fen.set(f))
                                    goto=Signal::derive(move || practice_goto.get())
                                    keybinds=Signal::derive(move || keybinds.get())
                                    theme=Signal::derive(move || settings_theme.get())
                                    pieces=Signal::derive(move || settings_pieces.get())
                                    light_color=Signal::derive(move || settings_light.get())
                                    dark_color=Signal::derive(move || settings_dark.get())
                                    coords=Signal::derive(move || settings_coords.get())
                                    lang=Signal::derive(move || language.get())
                                />
                            }
                                .into_any()
                        } else if practice_mode.get() == "spar" {
                            view! {
                                <Board
                                    pgn=spar_pgn
                                    title=Signal::derive(move || i18n::t(&language.get(), "practice.spar").to_string())
                                    orientation=Signal::derive(move || spar_color.get())
                                    play=Signal::derive(move || spar_play.get())
                                    on_move=Callback::new(on_spar_move)
                                    on_position=Callback::new(move |f: String| fen.set(f))
                                    on_play=Callback::new(move |_: (String, String, String)| {
                                        spar_status.set(i18n::t(&language.get(), "practice.your_move").into());
                                    })
                                    keybinds=Signal::derive(move || keybinds.get())
                                    theme=Signal::derive(move || settings_theme.get())
                                    pieces=Signal::derive(move || settings_pieces.get())
                                    light_color=Signal::derive(move || settings_light.get())
                                    dark_color=Signal::derive(move || settings_dark.get())
                                    coords=Signal::derive(move || settings_coords.get())
                                    lang=Signal::derive(move || language.get())
                                />
                            }
                                .into_any()
                        } else {
                            view! {
                                <Board
                                    pgn=match_pgn
                                    title=Signal::derive(move || i18n::t(&language.get(), "practice.match").to_string())
                                    play=Signal::derive(move || match_play.get())
                                    on_play=Callback::new(on_match_play)
                                    on_position=Callback::new(move |f: String| fen.set(f))
                                    read_only=true
                                    keybinds=Signal::derive(move || keybinds.get())
                                    theme=Signal::derive(move || settings_theme.get())
                                    pieces=Signal::derive(move || settings_pieces.get())
                                    light_color=Signal::derive(move || settings_light.get())
                                    dark_color=Signal::derive(move || settings_dark.get())
                                    coords=Signal::derive(move || settings_coords.get())
                                    lang=Signal::derive(move || language.get())
                                />
                            }
                                .into_any()
                        }
                    }}
                </div>
                <aside class="engine-pane">
                    <div class="panel-head"><h2>{move || i18n::t(&language.get(), "nav.practice")}</h2></div>
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
                                            {move || i18n::t(&language.get(), "practice.restart")}
                                        </button>
                                        <button on:click=move |_| {
                                            if let Some(next) = practice_sans
                                                .get_untracked()
                                                .get(practice_step.get_untracked())
                                            {
                                                practice_feedback.set(i18n::tf(
                                                    &language.get(),
                                                    "status.d29",
                                                    &[("next", next.clone())],
                                                ));
                                            }
                                        }>{move || i18n::t(&language.get(), "practice.reveal")}</button>
                                    </div>
                                </div>
                            }
                                .into_any()
                        } else if practice_mode.get() == "spar" {
                            view! {
                                <div class="engine-body">
                                    <div class="best">{move || spar_status.get()}</div>
                                    <div class="muted">
                                        {move || {
                                            if spar_thinking.get() {
                                                i18n::t(&language.get(), "practice.engine_thinking")
                                            } else {
                                                ""
                                            }
                                        }}
                                    </div>
                                    <div class="row-btns">
                                        <button on:click=start_spar>{move || i18n::t(&language.get(), "practice.new_game")}</button>
                                    </div>
                                </div>
                            }
                                .into_any()
                        } else {
                            view! {
                                <div class="engine-body">
                                    <div class="practice-stats">{move || match_status.get()}</div>
                                    <div class="muted">
                                        {move || {
                                            if match_result.get().is_empty() {
                                                format!("{} moves", match_moves.get().len())
                                            } else {
                                                format!("result {}", match_result.get())
                                            }
                                        }}
                                    </div>
                                    <div class="match-moves">
                                        {move || {
                                            match_moves
                                                .get()
                                                .chunks(2)
                                                .enumerate()
                                                .map(|(i, pair)| {
                                                    view! {
                                                        <div class="match-move">
                                                            <span class="muted">{format!("{}.", i + 1)}</span>
                                                            <span>{pair[0].clone()}</span>
                                                            <span>{pair.get(1).cloned().unwrap_or_default()}</span>
                                                        </div>
                                                    }
                                                })
                                                .collect_view()
                                        }}
                                    </div>
                                    <div class="row-btns">
                                        <button on:click=start_match disabled=move || match_running.get()>
                                            {move || i18n::t(&language.get(), "common.start")}
                                        </button>
                                        <button on:click=pause_match>{move || i18n::t(&language.get(), "common.pause")}</button>
                                        <button on:click=reset_match>{move || i18n::t(&language.get(), "common.reset")}</button>
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
}
