use leptos::prelude::*;

use crate::i18n;
use crate::model::BoardRef;
use crate::views::AppCtx;

/// The board strip above the board.
///
/// A workspace can hold several stored boards; this is how a person sees which
/// one is open and switches between them. The chips are also the board list —
/// the strip sits directly above the board, so a second copy in a rail would be
/// a second place to look for the same thing.
///
/// The actions mirror the repository: a new board is created in the workspace's
/// server workspace, a copy gets a new id and keeps its provenance, and a
/// locked board refuses edits until it is unlocked or copied.
pub fn boards_bar(ctx: AppCtx) -> impl IntoView {
    let AppCtx {
        active_ws,
        board_move_target,
        boards_busy,
        boards_msg,
        copy_active_board,
        language,
        move_active_board,
        new_board,
        select_board,
        toggle_active_board_lock,
        workspaces,
        ..
    } = ctx;

    let current = move || {
        workspaces
            .get()
            .into_iter()
            .find(|ws| ws.key == active_ws.get())
    };

    let chips = move || {
        let Some(ws) = current() else {
            return ().into_any();
        };
        let active = ws.active_board;
        ws.boards
            .iter()
            .enumerate()
            .map(|(index, board)| chip(board, index, index == active, select_board))
            .collect_view()
            .into_any()
    };

    view! {
        <div class="boards" role="group" aria-label=move || i18n::t(&language.get(), "boards.label")>
            <span class="boards-label">{move || i18n::t(&language.get(), "boards.label")}</span>
            {chips}
            <span class="boards-empty">
                {move || {
                    let empty = current().map(|ws| ws.boards.is_empty()).unwrap_or(true);
                    if empty {
                        i18n::t(&language.get(), "boards.empty").to_string()
                    } else {
                        String::new()
                    }
                }}
            </span>
            <button
                class="board-action"
                disabled=move || boards_busy.get()
                on:click=move |_| new_board.run(())
            >
                {move || i18n::t(&language.get(), "boards.new")}
            </button>
            <button
                class="board-action"
                disabled=move || boards_busy.get()
                on:click=move |_| copy_active_board.run(())
            >
                {move || i18n::t(&language.get(), "boards.copy")}
            </button>
            <button
                class="board-action"
                disabled=move || boards_busy.get()
                on:click=move |_| toggle_active_board_lock.run(())
            >
                {move || {
                    let locked = current()
                        .and_then(|ws| ws.active_board_ref().map(|board| board.locked()))
                        .unwrap_or(false);
                    i18n::t(
                        &language.get(),
                        if locked { "boards.unlock" } else { "boards.lock" },
                    )
                }}
            </button>
            <select
                class="board-move-target"
                aria-label=move || i18n::t(&language.get(), "boards.move_to")
                on:change=move |event| board_move_target.set(event_target_value(&event))
            >
                <option value="">{move || i18n::t(&language.get(), "boards.move_to")}</option>
                {move || {
                    let current = active_ws.get();
                    workspaces
                        .get()
                        .into_iter()
                        .filter(|ws| ws.key != current)
                        .map(|ws| {
                            view! { <option value=ws.key.to_string()>{ws.title.clone()}</option> }
                        })
                        .collect_view()
                }}
            </select>
            <button
                class="board-action"
                disabled=move || boards_busy.get() || board_move_target.get().is_empty()
                on:click=move |_| move_active_board.run(())
            >
                {move || i18n::t(&language.get(), "boards.move")}
            </button>
            <span class="boards-msg" aria-live="polite">{move || boards_msg.get()}</span>
        </div>
    }
}

/// One board chip: title, revision and whether it refuses writes.
fn chip(
    board: &BoardRef,
    index: usize,
    active: bool,
    select: UnsyncCallback<usize, ()>,
) -> impl IntoView {
    let title = board.title.clone();
    let revision = board.revision;
    let locked = board.locked();
    let class = if active {
        "board-chip active"
    } else {
        "board-chip"
    };
    let label = if locked {
        format!("{title}  r{revision} · locked")
    } else {
        format!("{title}  r{revision}")
    };
    let text = label.clone();
    view! {
        <button
            class=class
            title=label
            aria-pressed=if active { "true" } else { "false" }
            on:click=move |_| select.run(index)
        >
            {text}
        </button>
    }
}
