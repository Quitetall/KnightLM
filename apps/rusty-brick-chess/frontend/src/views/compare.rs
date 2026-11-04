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
pub fn CompareView(
    language: RwSignal<String>,
    message: RwSignal<String>,
) -> impl IntoView {
    let studies = RwSignal::new(Vec::<StudyInfo>::new());
    let left = RwSignal::new(None::<String>);
    let right = RwSignal::new(None::<String>);
    let report = RwSignal::new(None::<serde_json::Value>);
    let active_scope = RwSignal::new(None::<(String, String)>);

    let load_scope = move || {
        spawn_local(async move {
            if let Ok(scope) = api::compare_scope().await
                && scope.get("active").and_then(|a| a.as_bool()) == Some(true)
            {
                let l = scope.get("left").and_then(|v| v.as_str()).map(str::to_string);
                let r = scope.get("right").and_then(|v| v.as_str()).map(str::to_string);
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
            </aside>

            <section class="compare-result">
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
                {move || match report.get() {
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
                                {chapters.into_iter().map(|chapter| {
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
                                    view! {
                                        <li class=format!("diff-row {change}")>
                                            <span class="diff-change">{change.clone()}</span>
                                            <span class="diff-title">{title}</span>
                                            <span class="diff-detail">
                                                {format!("{boards_changed} board change(s)")}
                                            </span>
                                        </li>
                                    }
                                }).collect_view()}
                            </ul>
                        }.into_any()
                    }
                }}
            </section>
        </div>
    }
}
