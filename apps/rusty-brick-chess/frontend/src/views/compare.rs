//! The Compare tab: pick two studies; the diff is a list of what changed.
//!
//! The scope is server-side on purpose. It is the same selection the assistant
//! sees as a compact directory, so the question you ask about a comparison is
//! about the comparison you are looking at.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, StudyInfo};
use crate::i18n;

#[component]
pub fn CompareView(language: RwSignal<String>, message: RwSignal<String>) -> impl IntoView {
    let studies = RwSignal::new(Vec::<StudyInfo>::new());
    let left = RwSignal::new(None::<String>);
    let right = RwSignal::new(None::<String>);
    let report = RwSignal::new(None::<serde_json::Value>);
    let active_scope = RwSignal::new(None::<(String, String)>);
    // The filter is what makes a long comparison readable: show only the kinds of
    // change you are interested in.
    let filter = RwSignal::new("changed".to_string());
    let focused = RwSignal::new(None::<String>);

    let load_scope = move || {
        spawn_local(async move {
            if let Ok(scope) = api::compare_scope().await
                && scope.get("active").and_then(|a| a.as_bool()) == Some(true)
            {
                let l = scope
                    .get("left")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                let r = scope
                    .get("right")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                if let (Some(l), Some(r)) = (l, r) {
                    left.set(Some(l.clone()));
                    right.set(Some(r.clone()));
                    active_scope.set(Some((l, r)));
                }
            }
        });
    };

    spawn_local(async move {
        if let Ok(list) = api::list_studies().await {
            studies.set(list);
        }
    });
    load_scope();

    let run = move || {
        let (Some(l), Some(r)) = (left.get_untracked(), right.get_untracked()) else {
            message.set("pick two studies".to_string());
            return;
        };
        spawn_local(async move {
            match api::set_compare_scope(&l, &r).await {
                Ok(_) => {
                    active_scope.set(Some((l.clone(), r.clone())));
                    match api::compare(&l, &r, "you").await {
                        Ok(diff) => report.set(Some(diff)),
                        Err(error) => message.set(error),
                    }
                }
                Err(error) => message.set(error),
            }
        });
    };

    let t = move |key: &'static str| i18n::t(&language.get(), key);

    view! {
        <div class="compare-view">
            <aside class="compare-pick">
                <h3>{move || t("compare.studies")}</h3>
                <ul class="compare-rows">
                    {move || studies.get().into_iter().map(|study| {
                        let id_left = study.id.clone();
                        let id_right = study.id.clone();
                        let title = study.title.clone();
                        let is_left = {
                            let id = study.id.clone();
                            move || left.get().as_deref() == Some(id.as_str())
                        };
                        let is_right = {
                            let id = study.id.clone();
                            move || right.get().as_deref() == Some(id.as_str())
                        };
                        view! {
                            <li class="compare-row">
                                <span class="compare-title">{title}</span>
                                <button
                                    class="pick-left"
                                    class:selected=is_left
                                    on:click=move |_| left.set(Some(id_left.clone()))
                                >"A"</button>
                                <button
                                    class="pick-right"
                                    class:selected=is_right
                                    on:click=move |_| right.set(Some(id_right.clone()))
                                >"B"</button>
                            </li>
                        }
                    }).collect_view()}
                </ul>
                <button class="compare-run" on:click=move |_| run()>
                    {move || t("compare.run")}
                </button>
                <button
                    class="compare-export"
                    on:click=move |_| {
                        let Some((l, r)) = active_scope.get_untracked() else {
                            message.set("compare something first".to_string());
                            return;
                        };
                        spawn_local(async move {
                            match api::compare_markdown(&l, &r, "you").await {
                                Ok(markdown) => download_markdown(&markdown),
                                Err(error) => message.set(error),
                            }
                        });
                    }
                >
                    {move || t("compare.export")}
                </button>
            </aside>

            <section class="compare-result">
                <div class="filter-row">
                    {[
                        ("changed", "compare.filter_changed"),
                        ("all", "compare.filter_all"),
                    ]
                    .into_iter()
                    .map(|(value, key)| {
                        let label = i18n::t(&language.get(), key);
                        view! {
                            <button
                                class="filter-button"
                                class:selected=move || filter.get() == value
                                on:click=move |_| filter.set(value.to_string())
                            >
                                {label}
                            </button>
                        }
                    })
                    .collect_view()}
                </div>
                <div class="scope-chip-row">
                    {move || active_scope.get().map(|(l, r)| {
                        let names = studies.get();
                        let name = |id: &str| {
                            names
                                .iter()
                                .find(|study| study.id == id)
                                .map(|study| study.title.clone())
                                .unwrap_or_else(|| id.to_string())
                        };
                        view! {
                            <span class="scope-chip">
                                {format!("{} ↔ {}", name(&l), name(&r))}
                            </span>
                            <button
                                class="scope-clear"
                                on:click=move |_| {
                                    spawn_local(async move {
                                        let _ = api::clear_compare_scope().await;
                                        active_scope.set(None);
                                        report.set(None);
                                        left.set(None);
                                        right.set(None);
                                    });
                                }
                            >{move || t("compare.clear")}</button>
                        }.into_any()
                    })}
                </div>
                {move || {
                    // Tracked on purpose: the filter is part of what the list
                    // shows, so a render closure that ignores it renders stale
                    // rows.
                    let _current_filter = filter.get();
                    match report.get() {
                    None => view! {
                        <p class="compare-empty">{t("compare.pick")}</p>
                    }.into_any(),
                    Some(diff) => {
                        let identical = diff.get("identical").and_then(|v| v.as_bool()).unwrap_or(false);
                        let chapters = diff
                            .get("chapters")
                            .and_then(|c| c.as_array())
                            .cloned()
                            .unwrap_or_default();
                        view! {
                            <p class="compare-summary">
                                {if identical { t("compare.same") } else { t("compare.different") }}
                            </p>
                            <ul class="diff-rows">
                                {chapters.into_iter().filter(|chapter| {
                                    if _current_filter == "all" {
                                        return true;
                                    }
                                    chapter.get("change").and_then(|c| c.as_str()) != Some("unchanged")
                                }).map(|chapter| {
                                    let change = chapter
                                        .get("change")
                                        .and_then(|c| c.as_str())
                                        .unwrap_or("unchanged")
                                        .to_string();
                                    let title = chapter
                                        .get("title")
                                        .and_then(|t| t.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let boards_changed = chapter
                                        .get("boards_changed")
                                        .and_then(|b| b.as_array())
                                        .map(|boards| {
                                            boards
                                                .iter()
                                                .filter(|board| {
                                                    board.get("change").and_then(|c| c.as_str())
                                                        != Some("unchanged")
                                                })
                                                .count()
                                        })
                                        .unwrap_or(0);
                                    let title_for_click = title.clone();
                                    let id_for_click = chapter
                                        .get("left_id")
                                        .and_then(|v| v.as_str())
                                        .or_else(|| chapter.get("right_id").and_then(|v| v.as_str()))
                                        .unwrap_or_default()
                                        .to_string();
                                    let is_focused = {
                                        let id = id_for_click.clone();
                                        move || focused.get().as_deref() == Some(id.as_str())
                                    };
                                    view! {
                                        <li
                                            class=format!("diff-row {change}")
                                            class:focused=is_focused
                                            on:click=move |_| focused.set(Some(id_for_click.clone()))
                                        >
                                            <span class="diff-change">{change.clone()}</span>
                                            <span class="diff-title">{title_for_click}</span>
                                            <span class="diff-detail">
                                                {format!("{boards_changed} board change(s)")}
                                            </span>
                                        </li>
                                    }
                                }).collect_view()}
                            </ul>
                        }.into_any()
                    }
                    }
                }}
            </section>
        </div>
    }
}

/// Hand the Markdown to the browser as a download.
fn download_markdown(markdown: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(blob) = web_sys::Blob::new_with_str_sequence(&js_sys::Array::of1(&markdown.into()))
    else {
        return;
    };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else {
        return;
    };
    if let Some(document) = window.document() {
        if let Ok(element) = document.create_element("a") {
            let _ = element.set_attribute("href", &url);
            let _ = element.set_attribute("download", "diff.md");
            if let Some(anchor) =
                wasm_bindgen::JsCast::dyn_ref::<web_sys::HtmlAnchorElement>(&element)
            {
                anchor.click();
            }
        }
    }
    web_sys::Url::revoke_object_url(&url).ok();
}
