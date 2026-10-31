use leptos::prelude::*;
use leptos::task::spawn_local;
use crate::board::{Board};
use crate::api;
use crate::i18n;
use crate::model::*;
use crate::views::AppCtx;

pub fn puzzle(ctx: AppCtx) -> impl IntoView {
    let AppCtx {
        attempts,
        fen: _,
        language,
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
        settings_coords,
        settings_dark,
        settings_light,
        settings_pieces,
        settings_theme,
        show_solution,
        status,
        ..
    } = ctx;
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
                Ok(None) => status.set(i18n::t(&language.get(), "status.m15").into()),
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
        "correct" => i18n::t(&language.get(), "puzzles.correct").to_owned(),
        "wrong" => format!(
            "Not the puzzle move. Solution: {}",
            puzzle_san.get().join(" ")
        ),
        "solved" => match puzzle_delta.get() {
            Some((before, after)) => format!("Solved! Rating {before} → {after}"),
            None => i18n::t(&language.get(), "puzzles.solved").to_owned(),
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
                                    light_color=Signal::derive(move || settings_light.get())
                                    dark_color=Signal::derive(move || settings_dark.get())
                                    coords=Signal::derive(move || settings_coords.get())
                                    lang=Signal::derive(move || language.get())
                />
            </div>
            <aside class="engine-pane">
                <div class="panel-head"><h2>{move || i18n::t(&language.get(), "nav.puzzles")}</h2></div>
                <div class="row-btns">
                    <select
                        prop:value=move || puzzle_theme.get()
                        on:change=move |ev| puzzle_theme.set(event_target_value(&ev))
                    >
                        <option value="">{move || i18n::t(&language.get(), "puzzles.any_theme")}</option>
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
                    <button on:click=load>{move || i18n::t(&language.get(), "puzzles.next")}</button>
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
                                            {move || {
                                                if show_solution.get() {
                                                    i18n::t(&language.get(), "puzzles.hide_solution")
                                                } else {
                                                    i18n::t(&language.get(), "puzzles.show_solution")
                                                }
                                            }}
                                        </button>
                                        <button on:click=retry>{move || i18n::t(&language.get(), "puzzles.retry")}</button>
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
                <div class="panel-sub">{move || i18n::t(&language.get(), "puzzles.recent")}</div>
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
                <div class="panel-sub">{move || i18n::t(&language.get(), "puzzles.import")}</div>
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
                                status.set(i18n::tf(&language.get(), "status.d18", &[("n", n.to_string())]));
                                puzzle_csv.set(String::new());
                                if let Ok(t) = api::puzzle_themes().await {
                                    puzzle_themes.set(t);
                                }
                            }
                            Err(e) => status.set(e),
                        }
                    });
                }>{move || i18n::t(&language.get(), "puzzles.import_csv")}</button>
            </aside>
        </section>
    }
    .into_any()
}
