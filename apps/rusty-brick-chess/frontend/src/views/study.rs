//! The Study tab: chapters on the left, the chapter's boards in the middle,
//! its markdown on the right.
//!
//! A study is a document about positions, not a pile of boards, so the layout
//! follows the design's three columns. The rules the editor enforces come from
//! the model, not from here: a published study refuses writes in the repository,
//! and duplicating a chapter shares its boards rather than cloning them.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, ChapterInfo, StudyInfo};
use crate::i18n;

#[component]
pub fn StudyView(
    /// The language signal, so labels follow the rest of the app.
    language: RwSignal<String>,
    /// Reports a problem to the toast area.
    message: RwSignal<String>,
) -> impl IntoView {
    let studies = RwSignal::new(Vec::<StudyInfo>::new());
    let open_study = RwSignal::new(None::<StudyInfo>);
    let chapters = RwSignal::new(Vec::<ChapterInfo>::new());
    let active_chapter = RwSignal::new(None::<String>);
    let active_board = RwSignal::new(None::<String>);
    let notes = RwSignal::new(String::new());
    let preview = RwSignal::new(false);
    let new_chapter = RwSignal::new(String::new());

    let reload_studies = move || {
        spawn_local(async move {
            if let Ok(list) = api::list_studies().await {
                studies.set(list);
            }
        });
    };

    let load_chapters = move |study_id: String| {
        spawn_local(async move {
            match api::list_chapters(&study_id).await {
                Ok(list) => {
                    // Keep the selection if it still exists; otherwise take the first.
                    let still_there = active_chapter
                        .get_untracked()
                        .filter(|id| list.iter().any(|c| &c.id == id));
                    let chosen = still_there.or_else(|| list.first().map(|c| c.id.clone()));
                    if let Some(chapter) = list.iter().find(|c| Some(c.id.clone()) == chosen) {
                        notes.set(chapter.notes_md.clone());
                        let board = board_ids_of(chapter)
                            .first()
                            .cloned()
                            .unwrap_or_else(|| chapter.primary_board_id.clone());
                        active_board.set(Some(board));
                    }
                    active_chapter.set(chosen);
                    chapters.set(list);
                }
                Err(error) => message.set(error),
            }
        });
    };

    reload_studies();

    let t = move |key: &'static str| i18n::t(&language.get(), key);

    view! {
        <div class="study-view">
            <aside class="study-list">
                <h3>{move || t("study.studies")}</h3>
                <div class="study-create">
                    <button
                        class="study-new"
                        on:click=move |_| {
                            spawn_local(async move {
                                match api::create_study("New study", "").await {
                                    Ok(study) => {
                                        reload_studies();
                                        open_study.set(Some(study.clone()));
                                        active_chapter.set(None);
                                        chapters.set(Vec::new());
                                        load_chapters(study.id);
                                    }
                                    Err(error) => message.set(error),
                                }
                            });
                        }
                    >
                        {move || t("study.new")}
                    </button>
                </div>
                <ul class="study-rows">
                    {move || studies.get().into_iter().map(|study| {
                        let id = study.id.clone();
                        let title = study.title.clone();
                        let selected = move || open_study.get().map(|s| s.id == id).unwrap_or(false);
                        view! {
                            <li
                                class="study-row"
                                class:selected=selected
                                on:click=move |_| {
                                    let id = study.id.clone();
                                    open_study.set(Some(study.clone()));
                                    load_chapters(id);
                                }
                            >
                                <span class="study-row-title">{title.clone()}</span>
                                <span class="study-row-state">
                                    {if study.lifecycle == "published" {
                                        format!(" v{}", study.current_version)
                                    } else {
                                        " draft".to_string()
                                    }}
                                </span>
                            </li>
                        }
                    }).collect_view()}
                </ul>
            </aside>

            <section class="study-chapters">
                {move || match open_study.get() {
                    None => view! {
                        <p class="study-empty">{t("study.pick")}</p>
                    }.into_any(),
                    Some(study) => {
                        let study_id = study.id.clone();
                        let study_id_add = std::sync::Arc::new(study.id.clone());
                        let published = study.lifecycle == "published";
                        view! {
                            <div class="study-head">
                                <input
                                    class="study-title"
                                    prop:value=move || study.title.clone()
                                    prop:disabled=published
                                    on:change={
                                        let id = study_id.clone();
                                        move |event| {
                                        let title = event_target_value(&event);
                                        let id = id.clone();
                                        spawn_local(async move {
                                            match api::update_study(&id, Some(&title), None).await {
                                                Ok(updated) => open_study.set(Some(updated)),
                                                Err(error) => message.set(error),
                                            }
                                        });
                                    }}
                                />
                                <span class="study-lifecycle">
                                    {if published { "published" } else { "draft" }}
                                </span>
                            </div>
                            <div class="chapter-actions">
                                <input
                                    class="chapter-title"
                                    prop:disabled=published
                                    placeholder=move || t("study.chapter_name")
                                    prop:value=move || new_chapter.get()
                                    on:input=move |event| new_chapter.set(event_target_value(&event))
                                />
                                <button
                                    class="chapter-add"
                                    prop:disabled=published
                                    on:click={
                                        let study_id = study_id_add.clone();
                                        move |_| {
                                            let title = new_chapter.get_untracked();
                                            let title = if title.trim().is_empty() { "Chapter".to_string() } else { title };
                                            let id = (*study_id).clone();
                                            spawn_local(async move {
                                                match api::create_chapter(&id, &title, "").await {
                                                    Ok(_) => {
                                                        new_chapter.set(String::new());
                                                        load_chapters(id);
                                                    }
                                                    Err(error) => message.set(error),
                                                }
                                            });
                                        }
                                    }
                                >
                                    {move || t("study.add_chapter")}
                                </button>
                            </div>
                            <ul class="chapter-rows">
                                {move || {
                                    let list = chapters.get();
                                    let count = list.len();
                                    list.into_iter().enumerate().map(|(index, chapter)| {
                                        let id = chapter.id.clone();
                                        let id_dup = chapter.id.clone();
                                        let id_del = chapter.id.clone();
                                        let id_up = chapter.id.clone();
                                        let title = chapter.title.clone();
                                        let study_for_actions = study_id.clone();
                                        let selected = move || active_chapter.get().as_deref() == Some(id.as_str());
                                        view! {
                                            <li
                                                class="chapter-row"
                                                class:selected=selected
                                                on:click={
                                                    let id = chapter.id.clone();
                                                    move |_| {
                                                        if let Some(study) = open_study.get_untracked() {
                                                            let _ = &study;
                                                        }
                                                        active_chapter.set(Some(id.clone()));
                                                        let notes_text = chapters
                                                            .get_untracked()
                                                            .iter()
                                                            .find(|c| c.id == id)
                                                            .map(|c| c.notes_md.clone())
                                                            .unwrap_or_default();
                                                        notes.set(notes_text);
                                                    }
                                                }
                                            >
                                                <span class="chapter-ordinal">{index + 1}</span>
                                                <span class="chapter-name">{title}</span>
                                                <span class="chapter-buttons">
                                                    <button
                                                        class="chapter-up"
                                                        title=move || t("study.move_up")
                                                        prop:disabled=index == 0 || published
                                                        on:click={
                                                            let study_id = study_for_actions.clone();
                                                            let id_up = id_up.clone();
                                                            move |event| {
                                                                event.stop_propagation();
                                                                let mut ids: Vec<String> = chapters
                                                                    .get_untracked()
                                                                    .iter()
                                                                    .map(|c| c.id.clone())
                                                                    .collect();
                                                                if let Some(at) = ids.iter().position(|c| c == &id_up) {
                                                                    if at > 0 {
                                                                        ids.swap(at, at - 1);
                                                                    }
                                                                }
                                                                let study_id = study_id.clone();
                                                                spawn_local(async move {
                                                                    match api::reorder_chapters(&study_id, &ids).await {
                                                                        Ok(_) => load_chapters(study_id),
                                                                        Err(error) => message.set(error),
                                                                    }
                                                                });
                                                            }
                                                        }
                                                    >"↑"</button>
                                                    <button
                                                        class="chapter-down"
                                                        title=move || t("study.move_down")
                                                        prop:disabled=index + 1 == count || published
                                                        on:click={
                                                            let study_id = study_for_actions.clone();
                                                            let id_down = chapter.id.clone();
                                                            move |event| {
                                                                event.stop_propagation();
                                                                let mut ids: Vec<String> = chapters
                                                                    .get_untracked()
                                                                    .iter()
                                                                    .map(|c| c.id.clone())
                                                                    .collect();
                                                                if let Some(at) = ids.iter().position(|c| c == &id_down) {
                                                                    if at + 1 < ids.len() {
                                                                        ids.swap(at, at + 1);
                                                                    }
                                                                }
                                                                let study_id = study_id.clone();
                                                                spawn_local(async move {
                                                                    match api::reorder_chapters(&study_id, &ids).await {
                                                                        Ok(_) => load_chapters(study_id),
                                                                        Err(error) => message.set(error),
                                                                    }
                                                                });
                                                            }
                                                        }
                                                    >"↓"</button>
                                                    <button
                                                        class="chapter-duplicate"
                                                        prop:disabled=published
                                                        on:click={
                                                            let study_id = study_for_actions.clone();
                                                            move |event| {
                                                                event.stop_propagation();
                                                                let study_id = study_id.clone();
                                                                let id = id_dup.clone();
                                                                spawn_local(async move {
                                                                    match api::duplicate_chapter(&study_id, &id).await {
                                                                        Ok(_) => load_chapters(study_id),
                                                                        Err(error) => message.set(error),
                                                                    }
                                                                });
                                                            }
                                                        }
                                                    >{move || t("study.duplicate")}</button>
                                                    <button
                                                        class="chapter-delete"
                                                        prop:disabled=published
                                                        on:click={
                                                            let study_id = study_for_actions.clone();
                                                            move |event| {
                                                                event.stop_propagation();
                                                                let study_id = study_id.clone();
                                                                let id = id_del.clone();
                                                                spawn_local(async move {
                                                                    match api::delete_chapter(&study_id, &id).await {
                                                                        Ok(_) => load_chapters(study_id),
                                                                        Err(error) => message.set(error),
                                                                    }
                                                                });
                                                            }
                                                        }
                                                    >"✕"</button>
                                                </span>
                                            </li>
                                        }
                                    }).collect_view()
                                }}
                            </ul>
                        }.into_any()
                    }
                }}
            </section>

            <section class="study-boards">
                {move || match (open_study.get(), active_chapter.get()) {
                    (Some(study), Some(chapter_id)) => {
                        let chapter = chapters
                            .get()
                            .into_iter()
                            .find(|c| c.id == chapter_id);
                        let boards = chapter.as_ref().map(board_ids_of).unwrap_or_default();
                        let primary = chapter
                            .as_ref()
                            .map(|c| c.primary_board_id.clone())
                            .unwrap_or_default();
                        let study_id = study.id.clone();
                        let study_id_add = study.id.clone();
                        let chapter_for_add = chapter_id.clone();
                        let published = study.lifecycle == "published";
                        view! {
                            <div class="board-strip">
                                {boards.into_iter().enumerate().map(|(index, board_id)| {
                                    let id = board_id.clone();
                                    let id_for_click = board_id.clone();
                                    let label = format!("{}", index + 1);
                                    let is_primary = board_id == primary;
                                    let selected = move || active_board.get().as_deref() == Some(id.as_str());
                                    view! {
                                        <button
                                            class="strip-board"
                                            class:selected=selected
                                            class:primary=is_primary
                                            on:click=move |_| active_board.set(Some(id_for_click.clone()))
                                        >
                                            {label}
                                            {if is_primary { " ★" } else { "" }}
                                        </button>
                                    }
                                }).collect_view()}
                                <button
                                    class="strip-add"
                                    prop:disabled=published
                                    on:click={
                                        let study_id = study_id_add.clone();
                                        let chapter_id = chapter_for_add.clone();
                                        move |_| {
                                            let study_id = study_id.clone();
                                            let chapter_id = chapter_id.clone();
                                            spawn_local(async move {
                                                match api::add_chapter_board(&study_id, &chapter_id, "Board", None).await {
                                                    Ok(_) => load_chapters(study_id),
                                                    Err(error) => message.set(error),
                                                }
                                            });
                                        }
                                    }
                                >{move || t("study.add_board")}</button>
                                <button
                                    class="strip-primary"
                                    prop:disabled=published
                                    on:click={
                                        let study_id = study_id.clone();
                                        let chapter_id = chapter_id.clone();
                                        move |_| {
                                            let Some(board_id) = active_board.get_untracked() else {
                                                message.set("choose a board first".to_string());
                                                return;
                                            };
                                            let study_id = study_id.clone();
                                            let chapter_id = chapter_id.clone();
                                            spawn_local(async move {
                                                match api::set_chapter_primary(&study_id, &chapter_id, &board_id).await {
                                                    Ok(_) => load_chapters(study_id),
                                                    Err(error) => message.set(error),
                                                }
                                            });
                                        }
                                    }
                                >{move || t("study.set_primary")}</button>
                            </div>
                            <div class="study-board-slot">
                                <p class="board-hint">
                                    {move || match active_board.get() {
                                        Some(board) => format!("board {board}"),
                                        None => "no board selected".to_string(),
                                    }}
                                </p>
                                <a
                                    class="board-open"
                                    href=move || format!("/#board={}", active_board.get().unwrap_or_default())
                                >
                                    {move || t("study.open_board")}
                                </a>
                            </div>
                        }.into_any()
                    }
                    _ => view! { <p class="study-empty">{t("study.no_chapter")}</p> }.into_any(),
                }}
            </section>

            <section class="study-notes">
                {move || match (open_study.get(), active_chapter.get()) {
                    (Some(study), Some(chapter_id)) => {
                        let published = study.lifecycle == "published";
                        let study_id = study.id.clone();
                        let chapter_save = chapter_id.clone();
                        view! {
                            <div class="notes-head">
                                <span>{move || t("study.notes")}</span>
                                <button
                                    class="notes-preview-toggle"
                                    on:click=move |_| preview.update(|on| *on = !*on)
                                >
                                    {move || if preview.get() { t("study.edit") } else { t("study.preview") }}
                                </button>
                                <button
                                    class="notes-save"
                                    prop:disabled=published
                                    on:click={
                                        let study_id = study_id.clone();
                                        let chapter_save = chapter_save.clone();
                                        move |_| {
                                            let text = notes.get_untracked();
                                            let study_id = study_id.clone();
                                            let chapter_save = chapter_save.clone();
                                            spawn_local(async move {
                                                match api::update_chapter(&study_id, &chapter_save, None, Some(&text)).await {
                                                    Ok(_) => message.set("saved".to_string()),
                                                    Err(error) => message.set(error),
                                                }
                                            });
                                        }
                                    }
                                >{move || t("study.save")}</button>
                            </div>
                            {move || if preview.get() {
                                // Preview as text: the Markdown renderer lands with
                                // the notes ticket, and showing raw Markdown as a
                                // paragraph is honest about that.
                                view! { <pre class="notes-preview">{notes.get()}</pre> }.into_any()
                            } else {
                                view! {
                                    <textarea
                                        class="notes-editor"
                                        prop:disabled=published
                                        prop:value=move || notes.get()
                                        on:input=move |event| notes.set(event_target_value(&event))
                                    ></textarea>
                                }.into_any()
                            }}
                            <div class="study-meta">
                                <label>
                                    {move || t("study.description")}
                                    <textarea
                                        class="study-description"
                                        prop:value={
                                            let study = study.clone();
                                            move || study.description_md.clone()
                                        }
                                        prop:disabled=published
                                        on:change=move |event| {
                                            let description = event_target_value(&event);
                                            let id = study_id_for(&open_study);
                                            spawn_local(async move {
                                                if let Some(id) = id {
                                                    match api::update_study(&id, None, Some(&description)).await {
                                                        Ok(updated) => open_study.set(Some(updated)),
                                                        Err(error) => message.set(error),
                                                    }
                                                }
                                            });
                                        }
                                    ></textarea>
                                </label>
                                <button
                                    class="study-publish"
                                    prop:disabled=published
                                    on:click=move |_| {
                                        let Some(study) = open_study.get_untracked() else {
                                            return;
                                        };
                                        let id = study.id.clone();
                                        let version = study.current_version;
                                        spawn_local(async move {
                                            match api::publish_study(&id, version).await {
                                                Ok(published) => {
                                                    message.set(format!("published v{}", published.version));
                                                    if let Ok(list) = api::list_studies().await {
                                                        studies.set(list.clone());
                                                        if let Some(updated) = list.into_iter().find(|s| s.id == id) {
                                                            open_study.set(Some(updated));
                                                        }
                                                    }
                                                }
                                                Err(error) => message.set(error),
                                            }
                                        });
                                    }
                                >
                                    {move || t("study.publish")}
                                </button>
                            </div>
                        }.into_any()
                    }
                    _ => view! { <p class="study-empty">{t("study.no_chapter")}</p> }.into_any(),
                }}
            </section>
        </div>
    }
}

/// The boards a chapter lists, read out of its JSON column.
fn board_ids_of(chapter: &ChapterInfo) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(&chapter.board_ids_json).unwrap_or_default()
}

fn study_id_for(open: &RwSignal<Option<StudyInfo>>) -> Option<String> {
    open.get_untracked().map(|study| study.id)
}
