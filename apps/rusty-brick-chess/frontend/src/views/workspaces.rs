use leptos::prelude::*;

use crate::i18n;
use crate::model::Workspace;
use crate::views::AppCtx;

/// The tab strip above the board: switch, rename, reorder and close workspaces.
pub fn workspaces_bar(ctx: AppCtx) -> impl IntoView {
    let AppCtx {
        workspaces,
        active_ws,
        renaming_ws,
        rename_text,
        new_tab_kind,
        language,
        ..
    } = ctx;
    let activate_workspace = move |key| ctx.activate_workspace.run(key);
    let close_workspace = move |key| ctx.close_workspace.run(key);
    let commit_rename = move || ctx.commit_rename.run(());
    let move_workspace = move |key, delta| ctx.move_workspace.run((key, delta));
    let new_workspace = move || ctx.new_workspace.run(());
    let open_page_tab = move |kind| ctx.open_page_tab.run(kind);
    view! {
        <div class="workspaces">
            <span class="ws-label">{move || i18n::t(&language.get(), "ws.label")}</span>
            <For
                each=move || workspaces.get()
                key=|ws| (ws.key, ws.title.clone())
                children=move |ws: Workspace| {
                    let key = ws.key;
                    let title = ws.title.clone();
                    let kind = ws.kind.clone();
                    let kind_label = match kind.as_str() {
                        "database" => "DB",
                        "openings" => "OP",
                        "puzzles" => "PZ",
                        _ => "AN",
                    };
                    let title_shared = StoredValue::new(title.clone());
                    view! {
                        <div class=move || {
                            if active_ws.get() == key { "ws-tab active" } else { "ws-tab" }
                        }>
                            <span class=format!("ws-kind kind-{kind}")>{kind_label}</span>
                            {move || {
                                let title_click = title_shared;
                                let title_text = title_shared;
                                let title_dbl = title_shared;
                                if renaming_ws.get() == Some(key) {
                                    view! {
                                        <input
                                            class="ws-rename"
                                            prop:value=move || rename_text.get()
                                            on:input=move |ev| rename_text.set(event_target_value(&ev))
                                            on:change=move |_| commit_rename()
                                            on:keydown=move |ev| {
                                                if ev.key() == i18n::t(&language.get(), "common.enter") {
                                                    commit_rename();
                                                }
                                            }
                                        />
                                    }
                                        .into_any()
                                } else {
                                    view! {
                                        <button
                                            class="ws-name"
                                            title=title_click.get_value()
                                            on:click=move |_| activate_workspace(key)
                                            on:dblclick=move |_| {
                                                rename_text.set(title_dbl.get_value());
                                                renaming_ws.set(Some(key));
                                            }
                                        >
                                            {title_text.get_value()}
                                        </button>
                                    }
                                        .into_any()
                                }
                            }}
                            <button
                                class="ws-icon"
                                title={move || i18n::t(&language.get(), "common.rename")}
                                aria-label="Rename workspace"
                                on:click=move |_| {
                                    rename_text.set(title_shared.get_value());
                                    renaming_ws.set(Some(key));
                                }
                            >
                                "✎"
                            </button>
                            <button
                                class="ws-icon"
                                title={move || i18n::t(&language.get(), "ws.move_left")}
                                aria-label="Move workspace left"
                                on:click=move |_| move_workspace(key, -1)
                            >
                                "‹"
                            </button>
                            <button
                                class="ws-icon"
                                title={move || i18n::t(&language.get(), "ws.move_right")}
                                aria-label="Move workspace right"
                                on:click=move |_| move_workspace(key, 1)
                            >
                                "›"
                            </button>
                            <button
                                class="ws-close"
                                title={move || i18n::t(&language.get(), "ws.close")}
                                aria-label={move || i18n::t(&language.get(), "ws.close")}
                                on:click=move |_| close_workspace(key)
                            >
                                "×"
                            </button>
                        </div>
                    }
                }
            />
            <select
                class="ws-new"
                prop:value=move || new_tab_kind.get()
                on:change=move |ev| {
                    let kind = event_target_value(&ev);
                    new_tab_kind.set(String::new());
                    match kind.as_str() {
                        "analysis" => new_workspace(),
                        "database" | "openings" | "puzzles" => open_page_tab(kind.clone()),
                        _ => {}
                    }
                }
            >
                <option value="">"+ New tab"</option>
                <option value="analysis">"Analysis"</option>
                <option value="database">"Database"</option>
                <option value="openings">{move || i18n::t(&language.get(), "nav.openings")}</option>
                <option value="puzzles">{move || i18n::t(&language.get(), "nav.puzzles")}</option>
            </select>
        </div>
    }
}
