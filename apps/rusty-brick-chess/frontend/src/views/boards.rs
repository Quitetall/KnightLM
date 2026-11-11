use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::ai_api;
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
        ai_session_id,
        ai_target_board,
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

    // The board the assistant is writing to, which may not be the one on screen.
    let ai_target = move || ai_target_board.get();
    let ai_target_title = move || {
        let target = ai_target_board.get()?;
        let ws = current()?;
        ws.boards
            .iter()
            .find(|board| board.id == target)
            .map(|board| board.title.clone())
    };

    let chips = move || {
        let Some(ws) = current() else {
            return ().into_any();
        };
        let active = ws.active_board;
        ws.boards
            .iter()
            .enumerate()
            .map(|(index, board)| {
                let target = ai_target_board.get().as_deref() == Some(board.id.as_str());
                chip(board, index, index == active, target, select_board)
            })
            .collect_view()
            .into_any()
    };

    view! {
        <div class="boards" role="group" aria-label=move || i18n::t(&language.get(), "boards.label")>
            <span class="boards-label">{move || i18n::t(&language.get(), "boards.label")}</span>
            {chips}
            {move || {
                let target = ai_target();
                let visible = current()
                    .and_then(|ws| ws.active_board_ref().map(|board| board.id.clone()));
                if target.is_some() && target != visible {
                    let title = ai_target_title().unwrap_or_else(|| "another board".to_string());
                    let target_id = target.clone();
                    view! {
                        <button
                            class="board-target-badge"
                            title=move || i18n::t(&language.get(), "boards.target_badge").to_string()
                            on:click=move |_| {
                                // Focusing is explicit: the badge names the board
                                // and this is the request to look at it.
                                // Switching to it is the explicit request; the
                                // badge is only a prompt until then.
                                if let Some(target) = target_id.clone()
                                    && let Some(ws) = current()
                                        && let Some(index) =
                                            ws.boards.iter().position(|board| board.id == target)
                                        {
                                            select_board.run(index);
                                        }
                            }
                        >
                            {format!("AI → {title}")}
                        </button>
                    }.into_any()
                } else {
                    ().into_any()
                }
            }}
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
                disabled=move || boards_busy.get() || current().is_none_or(|ws| ws.backend().is_none())
                on:click=move |_| new_board.run(())
            >
                {move || i18n::t(&language.get(), "boards.new")}
            </button>
            <button
                class="board-action board-set-target"
                disabled=move || ai_session_id.get().is_none() || boards_busy.get()
                title=move || i18n::t(&language.get(), "boards.set_target").to_string()
                on:click=move |_| {
                    let Some(session) = ai_session_id.get_untracked() else { return };
                    let Some(board) = current()
                        .and_then(|ws| ws.active_board_ref().map(|board| board.id.clone()))
                    else {
                        return;
                    };
                    spawn_local(async move {
                        if ai_api::set_ai_target(&session, Some(&board)).await.is_ok() {
                            ai_target_board.set(Some(board));
                        }
                    });
                }
            >
                {move || i18n::t(&language.get(), "boards.set_target")}
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
    target: bool,
    select: UnsyncCallback<usize, ()>,
) -> impl IntoView {
    let title = board.title.clone();
    let revision = board.revision;
    let locked = board.locked();
    let class = match (active, target) {
        (true, true) => "board-chip active target",
        (true, false) => "board-chip active",
        (false, true) => "board-chip target",
        (false, false) => "board-chip",
    };
    let mut label = if locked {
        format!("{title}  r{revision} · locked")
    } else {
        format!("{title}  r{revision}")
    };
    if target {
        label.push_str("  · AI target");
    }
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
